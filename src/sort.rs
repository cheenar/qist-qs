use crate::{
    fs::{Entry, Kind},
    options::{Options, Sort},
};
use std::{cmp::Ordering, ffi::CString, os::unix::ffi::OsStrExt};
use unicode_width::UnicodeWidthStr;

pub fn entries(entries: &mut [Entry], o: &Options) -> bool {
    if o.sort == Sort::None {
        return false;
    }
    if o.sort == Sort::Name && c_locale() {
        entries.sort_unstable_by(|a, b| {
            let mut order = a.name.as_bytes().cmp(b.name.as_bytes());
            if o.reverse {
                order = order.reverse();
            }
            if o.group_dirs {
                order = (b.kind == Kind::Directory)
                    .cmp(&(a.kind == Kind::Directory))
                    .then(order);
            }
            order
        });
        return false;
    }
    // Cache strxfrm locale keys once. Comparators do not allocate or call libc.
    let keys: Vec<_> = entries.iter().map(|e| key(e.name.as_bytes())).collect();
    // strxfrm cannot form a valid key for a name outside the active encoding.
    // Darwin strcoll has the same byte fallback as /bin/ls for those names.
    let collation_failed = keys.iter().any(Option::is_none);
    let native_names = collation_failed.then(|| {
        entries
            .iter()
            .map(|e| CString::new(e.name.as_bytes()).expect("Unix names cannot contain NUL"))
            .collect::<Vec<_>>()
    });
    let mut indices: Vec<_> = (0..entries.len()).collect();
    indices.sort_unstable_by(|&a, &b| {
        let ea = &entries[a];
        let eb = &entries[b];
        let name = || {
            (if let Some(names) = &native_names {
                unsafe { libc::strcoll(names[a].as_ptr(), names[b].as_ptr()) }.cmp(&0)
            } else {
                keys[a].cmp(&keys[b])
            })
            .then_with(|| ea.name.as_bytes().cmp(eb.name.as_bytes()))
        };
        let mut order = match o.sort {
            Sort::Size => eb
                .stat
                .as_ref()
                .map(|s| s.size)
                .cmp(&ea.stat.as_ref().map(|s| s.size))
                .then_with(name),
            Sort::Time => {
                let stamp = |e: &Entry| e.stamp(o.time).map(|s| (s.seconds, s.nanoseconds));
                stamp(eb).cmp(&stamp(ea)).then_with(|| {
                    if o.same_sort {
                        name().reverse()
                    } else {
                        name()
                    }
                })
            }
            Sort::Extension => extension(ea.name.as_bytes())
                .cmp(extension(eb.name.as_bytes()))
                .then_with(name),
            Sort::Version => version(ea.name.as_bytes(), eb.name.as_bytes()).then_with(name),
            Sort::Width => UnicodeWidthStr::width(ea.name.to_string_lossy().as_ref())
                .cmp(&UnicodeWidthStr::width(eb.name.to_string_lossy().as_ref()))
                .then_with(name),
            _ => name(),
        };
        if o.reverse {
            order = order.reverse();
        }
        if o.group_dirs {
            order = (eb.kind == Kind::Directory)
                .cmp(&(ea.kind == Kind::Directory))
                .then(order);
        }
        order
    });
    // Apply the permutation in place, without cloning path strings and metadata.
    let mut target = vec![0; entries.len()];
    for (new, old) in indices.into_iter().enumerate() {
        target[old] = new;
    }
    for i in 0..entries.len() {
        while target[i] != i {
            let j = target[i];
            entries.swap(i, j);
            target.swap(i, j);
        }
    }
    collation_failed && o.compat == crate::options::Compat::Bsd
}

fn key(bytes: &[u8]) -> Option<Vec<u8>> {
    // In the C/POSIX locale byte sorting is both correct and much faster.
    if c_locale() {
        return Some(bytes.to_vec());
    }
    let Ok(s) = CString::new(bytes) else {
        return None;
    };
    #[cfg(target_os = "macos")]
    let errno = unsafe { libc::__error() };
    #[cfg(target_os = "linux")]
    let errno = unsafe { libc::__errno_location() };
    unsafe {
        *errno = 0;
    }
    let size = unsafe { libc::strxfrm(std::ptr::null_mut(), s.as_ptr(), 0) };
    if unsafe { *errno } != 0 || size > 1024 * 1024 {
        return None;
    }
    let mut result = vec![0u8; size + 1];
    unsafe {
        libc::strxfrm(result.as_mut_ptr().cast(), s.as_ptr(), result.len());
    }
    result.truncate(size);
    if unsafe { *errno } != 0 {
        None
    } else {
        Some(result)
    }
}
fn c_locale() -> bool {
    let locale = unsafe { libc::setlocale(libc::LC_COLLATE, std::ptr::null()) };
    if locale.is_null() {
        return true;
    }
    matches!(
        unsafe { std::ffi::CStr::from_ptr(locale) }.to_bytes(),
        b"C" | b"POSIX" | b"C.UTF-8" | b"C.utf8"
    )
}
fn extension(name: &[u8]) -> &[u8] {
    name.iter()
        .rposition(|b| *b == b'.')
        .map_or(&[], |i| &name[i + 1..])
}

// Debian/GNU version ordering: digit runs compare numerically without integer
// conversion, '~' precedes everything, letters precede punctuation.
pub fn version(a: &[u8], b: &[u8]) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    for special in [b"".as_slice(), b".".as_slice(), b"..".as_slice()] {
        if a == special {
            return Ordering::Less;
        }
        if b == special {
            return Ordering::Greater;
        }
    }
    if a.starts_with(b".") != b.starts_with(b".") {
        return if a.starts_with(b".") {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let stem_a = &a[..version_stem(a)];
    let stem_b = &b[..version_stem(b)];
    version_parts(stem_a, stem_b).then_with(|| version_parts(a, b))
}

fn version_stem(name: &[u8]) -> usize {
    let mut end = name.len();
    while let Some(dot) = name[..end].iter().rposition(|b| *b == b'.') {
        let suffix = &name[dot + 1..end];
        if dot == 0
            || !suffix
                .first()
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'~')
            || !suffix
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || *c == b'~')
        {
            break;
        }
        end = dot;
    }
    end
}

fn version_parts(a: &[u8], b: &[u8]) -> Ordering {
    let mut i = 0;
    let mut j = 0;
    while i < a.len() || j < b.len() {
        while a.get(i).is_some_and(|c| !c.is_ascii_digit())
            || b.get(j).is_some_and(|c| !c.is_ascii_digit())
        {
            let order = weight(a.get(i).copied()).cmp(&weight(b.get(j).copied()));
            if order != Ordering::Equal {
                return order;
            }
            if i < a.len() {
                i += 1;
            }
            if j < b.len() {
                j += 1;
            }
        }
        while a.get(i) == Some(&b'0') {
            i += 1;
        }
        while b.get(j) == Some(&b'0') {
            j += 1;
        }
        let start_i = i;
        let start_j = j;
        while a.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        while b.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        let order = (i - start_i)
            .cmp(&(j - start_j))
            .then_with(|| a[start_i..i].cmp(&b[start_j..j]));
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}
fn weight(c: Option<u8>) -> i32 {
    match c {
        Some(b'~') => -1,
        None => 0,
        Some(c) if c.is_ascii_digit() => 0,
        Some(c) if c.is_ascii_alphabetic() => c as i32,
        Some(c) => c as i32 + 256,
    }
}
