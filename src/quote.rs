use crate::options::Quote;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

pub fn name(name: &OsStr, style: Quote) -> Vec<u8> {
    bytes(name.as_bytes(), style)
}

pub fn bytes(raw: &[u8], style: Quote) -> Vec<u8> {
    #[cfg(target_os = "macos")]
    if matches!(style, Quote::Question | Quote::Octal | Quote::EscapeBsd) {
        return darwin::quote(raw, style);
    }
    let ascii_locale = ascii_locale();
    match style {
        Quote::Literal => raw.to_vec(),
        Quote::Shell | Quote::ShellAlways | Quote::ShellEscape | Quote::ShellEscapeAlways => {
            shell(raw, style)
        }
        Quote::Question => {
            if ascii_locale {
                return raw
                    .iter()
                    .map(|b| {
                        if b.is_ascii_control() || *b >= 128 {
                            b'?'
                        } else {
                            *b
                        }
                    })
                    .collect();
            }
            // Invalid UTF-8 must become '?' individually, preserving printable Unicode.
            let mut out = Vec::new();
            let mut rest = raw;
            while !rest.is_empty() {
                match std::str::from_utf8(rest) {
                    Ok(s) => {
                        for c in s.chars() {
                            push_question(&mut out, c);
                        }
                        break;
                    }
                    Err(e) => {
                        let prefix =
                            unsafe { std::str::from_utf8_unchecked(&rest[..e.valid_up_to()]) };
                        for c in prefix.chars() {
                            push_question(&mut out, c);
                        }
                        let n = e.error_len().unwrap_or(rest.len() - e.valid_up_to());
                        out.extend(std::iter::repeat_n(b'?', n));
                        rest = &rest[e.valid_up_to() + n..];
                    }
                }
            }
            out
        }
        Quote::Locale | Quote::CLocale => {
            let quoted = bytes(raw, Quote::C);
            let (left, right): (&[u8], &[u8]) = if utf8_locale() {
                ("‘".as_bytes(), "’".as_bytes())
            } else if style == Quote::Locale {
                (b"'", b"'")
            } else {
                (b"\"", b"\"")
            };
            let mut out = left.to_vec();
            for &byte in &quoted[1..quoted.len() - 1] {
                if right == b"'" && byte == b'\'' {
                    out.push(b'\\');
                }
                out.push(byte);
            }
            out.extend_from_slice(right);
            out
        }
        _ => {
            let mut out = Vec::new();
            let quoted = style == Quote::C
                || style == Quote::EscapeMaybe
                    && raw.iter().any(|b| {
                        b.is_ascii_control()
                            || *b == b'"'
                            || *b == b'\\'
                            || ascii_locale && *b >= 128
                    })
                || style == Quote::EscapeMaybe && std::str::from_utf8(raw).is_err();
            if quoted {
                out.push(b'"');
            }
            let mut offset = 0;
            while offset < raw.len() {
                let b = raw[offset];
                if b >= 128 {
                    if ascii_locale {
                        out.extend_from_slice(format!("\\{b:03o}").as_bytes());
                        offset += 1;
                        continue;
                    }
                    let tail = &raw[offset..];
                    let valid = match std::str::from_utf8(tail) {
                        Ok(s) => s.len(),
                        Err(e) => e.valid_up_to(),
                    };
                    if valid > 0 {
                        out.extend_from_slice(&tail[..valid]);
                        offset += valid;
                        continue;
                    }
                    out.extend_from_slice(format!("\\{b:03o}").as_bytes());
                    offset += 1;
                    continue;
                }
                let escape = if style == Quote::Octal {
                    None
                } else {
                    match b {
                        b'\n' => Some("\\n"),
                        b'\r' => Some("\\r"),
                        b'\t' => Some("\\t"),
                        b'\x07' => Some("\\a"),
                        b'\x08' => Some("\\b"),
                        b'\x0b' => Some("\\v"),
                        b'\x0c' => Some("\\f"),
                        b'\\' => Some("\\\\"),
                        b'"' if quoted || style == Quote::EscapeBsd => Some("\\\""),
                        b' ' if !quoted && style != Quote::EscapeBsd => Some("\\ "),
                        _ => None,
                    }
                };
                if let Some(s) = escape {
                    out.extend_from_slice(s.as_bytes());
                } else if b.is_ascii_control() || style == Quote::Octal && matches!(b, b'\\' | b'"')
                {
                    out.extend_from_slice(format!("\\{b:03o}").as_bytes());
                } else {
                    out.push(b);
                }
                offset += 1;
            }
            if quoted {
                out.push(b'"');
            }
            out
        }
    }
}

pub fn display_width(raw: &[u8], compat: crate::options::Compat) -> usize {
    #[cfg(target_os = "macos")]
    if compat == crate::options::Compat::Bsd {
        return darwin::width(raw);
    }
    let _ = compat;
    unicode_width::UnicodeWidthStr::width(String::from_utf8_lossy(raw).as_ref())
}

#[cfg(target_os = "macos")]
mod darwin {
    use super::Quote;
    // Darwin's mbstate_t is a 128-byte opaque union aligned as a 64-bit integer
    // on both supported 64-bit architectures; libc doesn't expose it in Rust.
    #[repr(C, align(8))]
    struct MbState([u8; 128]);
    unsafe extern "C" {
        fn mbrtowc(
            wide: *mut libc::wchar_t,
            bytes: *const libc::c_char,
            size: usize,
            state: *mut MbState,
        ) -> usize;
        fn iswprint(wide: libc::c_int) -> libc::c_int;
        fn wcwidth(wide: libc::wchar_t) -> libc::c_int;
    }
    pub fn width(raw: &[u8]) -> usize {
        let mut state = MbState([0; 128]);
        let mut at = 0;
        let mut width = 0;
        while at < raw.len() {
            let mut wide = 0;
            let size = unsafe {
                mbrtowc(
                    &mut wide,
                    raw[at..].as_ptr().cast(),
                    raw.len() - at,
                    &mut state,
                )
            };
            if size == usize::MAX || size == usize::MAX - 1 || size == 0 {
                break;
            }
            width += unsafe { wcwidth(wide) }.max(0) as usize;
            at += size;
        }
        width
    }
    pub fn quote(raw: &[u8], style: Quote) -> Vec<u8> {
        let mut state = MbState([0; 128]);
        let mut at = 0;
        let mut output = Vec::with_capacity(raw.len());
        while at < raw.len() {
            let mut wide = 0;
            let size = unsafe {
                mbrtowc(
                    &mut wide,
                    raw[at..].as_ptr().cast(),
                    raw.len() - at,
                    &mut state,
                )
            };
            let valid = size != usize::MAX && size != usize::MAX - 1 && size != 0;
            let size = if valid {
                size
            } else {
                state = unsafe { std::mem::zeroed() };
                1
            };
            let printable = valid && unsafe { iswprint(wide) } != 0;
            let escaped_quote = size == 1 && matches!(raw[at], b'\\' | b'"');
            if printable && (style == Quote::Question || !escaped_quote) {
                output.extend_from_slice(&raw[at..at + size]);
            } else if style == Quote::Question {
                output.push(b'?');
            } else {
                for &byte in &raw[at..at + size] {
                    let short = if style == Quote::EscapeBsd {
                        match byte {
                            b'\n' => Some("\\n"),
                            b'\t' => Some("\\t"),
                            b'\r' => Some("\\r"),
                            7 => Some("\\a"),
                            8 => Some("\\b"),
                            11 => Some("\\v"),
                            12 => Some("\\f"),
                            b'\\' => Some("\\\\"),
                            b'"' => Some("\\\""),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    if let Some(text) = short {
                        output.extend_from_slice(text.as_bytes());
                    } else {
                        output.extend_from_slice(format!("\\{byte:03o}").as_bytes());
                    }
                }
            }
            at += size;
        }
        output
    }
}

fn utf8_locale() -> bool {
    let codeset = unsafe { libc::nl_langinfo(libc::CODESET) };
    !codeset.is_null()
        && unsafe { std::ffi::CStr::from_ptr(codeset) }
            .to_bytes()
            .eq_ignore_ascii_case(b"UTF-8")
}

fn ascii_locale() -> bool {
    let locale = unsafe { libc::setlocale(libc::LC_CTYPE, std::ptr::null()) };
    !locale.is_null()
        && matches!(
            unsafe { std::ffi::CStr::from_ptr(locale) }.to_bytes(),
            b"C" | b"POSIX"
        )
}

fn push_question(out: &mut Vec<u8>, c: char) {
    if c.is_control() {
        out.push(b'?');
    } else {
        let mut buf = [0; 4];
        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }
}

// Keep shell output directly usable as a single shell word, including names
// containing apostrophes and control bytes. ANSI-C quoting handles the latter.
fn shell(raw: &[u8], style: Quote) -> Vec<u8> {
    let always = matches!(style, Quote::ShellAlways | Quote::ShellEscapeAlways);
    let escapes = matches!(style, Quote::ShellEscape | Quote::ShellEscapeAlways);
    if escapes
        && (raw.iter().any(u8::is_ascii_control)
            || std::str::from_utf8(raw).is_err()
            || ascii_locale() && raw.iter().any(|b| *b >= 128))
    {
        let quoted = bytes(raw, Quote::C);
        let escaped = &quoted[1..quoted.len() - 1];
        let mut out = b"$'".to_vec();
        for b in escaped {
            if *b == b'\'' {
                out.push(b'\\');
            }
            out.push(*b);
        }
        out.push(b'\'');
        return out;
    }
    if !always
        && !raw.is_empty()
        && raw
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"_./-+,:@%".contains(b) || *b >= 128)
    {
        return raw.to_vec();
    }
    let mut out = vec![b'\''];
    for b in raw {
        if *b == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(*b);
        }
    }
    out.push(b'\'');
    out
}
