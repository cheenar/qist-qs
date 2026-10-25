//! OS-specific extended metadata. No shell commands or external helper processes.
use crate::{
    fs::{Entry, c_path},
    options::Options,
};
#[cfg(target_os = "macos")]
use std::ffi::CStr;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::block_size;

pub fn attributes(entry: &mut Entry, o: &Options, follow: bool) {
    let Ok(path) = c_path(&entry.path) else {
        return;
    };
    #[cfg(target_os = "macos")]
    {
        let flags = if follow { 0 } else { libc::XATTR_NOFOLLOW };
        let count = unsafe { libc::listxattr(path.as_ptr(), std::ptr::null_mut(), 0, flags) };
        entry.has_xattrs = count > 0;
        if count > 0 && o.xattrs {
            let mut names = vec![0u8; count as usize];
            let read = unsafe {
                libc::listxattr(path.as_ptr(), names.as_mut_ptr().cast(), names.len(), flags)
            };
            if read > 0 {
                for name in names[..read as usize]
                    .split(|b| *b == 0)
                    .filter(|s| !s.is_empty())
                {
                    if let Ok(cname) = std::ffi::CString::new(name) {
                        let size = unsafe {
                            libc::getxattr(
                                path.as_ptr(),
                                cname.as_ptr(),
                                std::ptr::null_mut(),
                                0,
                                0,
                                flags,
                            )
                        };
                        entry
                            .xattrs
                            .push((OsString::from_vec(name.to_vec()), size.max(0) as u64));
                    }
                }
            }
        }
        if o.long || o.acl {
            entry.acl = mac_acl(
                &path,
                follow,
                o.numeric,
                entry.kind == crate::fs::Kind::Directory,
            );
        }
    }
    #[cfg(target_os = "linux")]
    {
        let list = if follow {
            libc::listxattr
        } else {
            libc::llistxattr
        };
        let get = if follow {
            libc::getxattr
        } else {
            libc::lgetxattr
        };
        let count = unsafe { list(path.as_ptr(), std::ptr::null_mut(), 0) };
        entry.has_xattrs = count > 0;
        if count > 0 && (o.xattrs || o.long || o.acl) {
            let mut names = vec![0u8; count as usize];
            let read = unsafe { list(path.as_ptr(), names.as_mut_ptr().cast(), names.len()) };
            if read > 0 {
                for name in names[..read as usize]
                    .split(|b| *b == 0)
                    .filter(|s| !s.is_empty())
                {
                    if let Ok(cname) = std::ffi::CString::new(name) {
                        let size =
                            unsafe { get(path.as_ptr(), cname.as_ptr(), std::ptr::null_mut(), 0) };
                        if o.xattrs {
                            entry
                                .xattrs
                                .push((OsString::from_vec(name.to_vec()), size.max(0) as u64));
                        }
                        if (name == b"system.posix_acl_access"
                            || name == b"system.posix_acl_default")
                            && size > 4
                        {
                            let mut data = vec![0u8; size as usize];
                            if unsafe {
                                get(
                                    path.as_ptr(),
                                    cname.as_ptr(),
                                    data.as_mut_ptr().cast(),
                                    data.len(),
                                )
                            } == size
                            {
                                let text = linux_acl(&data, name.ends_with(b"default"));
                                entry.acl.get_or_insert_with(String::new).push_str(&text);
                            }
                        }
                    }
                }
            }
        }
        if o.context {
            let key = c"security.selinux";
            let count = unsafe { get(path.as_ptr(), key.as_ptr(), std::ptr::null_mut(), 0) };
            if count > 0 {
                let mut data = vec![0u8; count as usize];
                let read = unsafe {
                    get(
                        path.as_ptr(),
                        key.as_ptr(),
                        data.as_mut_ptr().cast(),
                        data.len(),
                    )
                };
                if read > 0 {
                    entry.context = Some(
                        String::from_utf8_lossy(&data[..read as usize])
                            .trim_end_matches('\0')
                            .into(),
                    );
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn mac_acl(path: &CStr, follow: bool, numeric: bool, directory: bool) -> Option<String> {
    unsafe extern "C" {
        fn acl_get_file(path: *const libc::c_char, kind: libc::c_int) -> *mut libc::c_void;
        fn acl_get_link_np(path: *const libc::c_char, kind: libc::c_int) -> *mut libc::c_void;
        fn acl_to_text(acl: *mut libc::c_void, length: *mut libc::ssize_t) -> *mut libc::c_char;
        fn acl_free(ptr: *mut libc::c_void) -> libc::c_int;
    }
    unsafe {
        let acl = if follow {
            acl_get_file(path.as_ptr(), 0x100)
        } else {
            acl_get_link_np(path.as_ptr(), 0x100)
        };
        if acl.is_null() {
            return None;
        }
        let text = acl_to_text(acl, std::ptr::null_mut());
        let result = if text.is_null() {
            None
        } else {
            let raw = CStr::from_ptr(text).to_string_lossy();
            let lines = raw
                .lines()
                .filter(|line| !line.starts_with("!#") && !line.is_empty())
                .map(|line| mac_acl_line(line, numeric, directory))
                .collect::<Vec<_>>()
                .join("\n");
            acl_free(text.cast());
            (!lines.is_empty()).then_some(lines)
        };
        acl_free(acl);
        result
    }
}

#[cfg(target_os = "macos")]
fn mac_acl_line(line: &str, numeric: bool, directory: bool) -> String {
    let fields: Vec<_> = line.splitn(6, ':').collect();
    if fields.len() != 6 {
        return line.into();
    }
    let name = if numeric || fields[2].is_empty() {
        fields[1].to_ascii_uppercase()
    } else {
        format!("{}:{}", fields[0], fields[2])
    };
    let flags: Vec<_> = fields[4].split(',').collect();
    let tag = flags
        .iter()
        .find(|f| matches!(**f, "allow" | "deny"))
        .copied()
        .unwrap_or("unknown");
    let mut permissions = Vec::new();
    for permission in fields[5].split(',').filter(|p| !p.is_empty()) {
        let permission = match (directory, permission) {
            (true, "read") => "list",
            (true, "write") => "add_file",
            (true, "execute") => "search",
            (true, "append") => "add_subdirectory",
            (false, "delete_child") | (_, "synchronize") => continue,
            (_, permission) => permission,
        };
        permissions.push(permission);
    }
    for flag in [
        "file_inherit",
        "directory_inherit",
        "limit_inherit",
        "only_inherit",
    ] {
        if flags.contains(&flag) && (directory || flag == "limit_inherit") {
            permissions.push(flag);
        }
    }
    format!(
        "{name}{} {tag} {}",
        if flags.contains(&"inherited") {
            " inherited"
        } else {
            ""
        },
        permissions.join(",")
    )
}

#[cfg(target_os = "linux")]
fn linux_acl(data: &[u8], default: bool) -> String {
    let mut output = String::new();
    for item in data[4..].chunks_exact(8) {
        let tag = u16::from_le_bytes([item[0], item[1]]);
        let perm = u16::from_le_bytes([item[2], item[3]]);
        let id = u32::from_le_bytes(item[4..8].try_into().unwrap());
        let who = match tag {
            1 => "user:".into(),
            2 => format!("user:{id}"),
            4 => "group:".into(),
            8 => format!("group:{id}"),
            16 => "mask:".into(),
            32 => "other:".into(),
            _ => continue,
        };
        output.push_str(&format!(
            "{}{who}:{}{}{}\n",
            if default { "default:" } else { "" },
            if perm & 4 != 0 { 'r' } else { '-' },
            if perm & 2 != 0 { 'w' } else { '-' },
            if perm & 1 != 0 { 'x' } else { '-' }
        ));
    }
    output
}

pub fn flags(value: u32) -> String {
    #[cfg(target_os = "macos")]
    {
        macos::flags(value)
    }
    #[cfg(not(target_os = "macos"))]
    {
        portable_flags(value)
    }
}

#[cfg(not(target_os = "macos"))]
fn portable_flags(value: u32) -> String {
    let mut names = vec![];
    let known = [
        (0x1, "nodump"),
        (0x2, "uchg"),
        (0x4, "uappnd"),
        (0x8, "opaque"),
        (0x20, "compressed"),
        (0x40, "tracked"),
        (0x80, "datavault"),
        (0x8000, "hidden"),
        (0x10000, "arch"),
        (0x20000, "schg"),
        (0x40000, "sappnd"),
        (0x80000, "restricted"),
        (0x100000, "sunlnk"),
        (0x800000, "firmlink"),
        (0x40000000, "dataless"),
    ];
    let mut remaining = value;
    for (bit, name) in known {
        if value & bit != 0 {
            names.push(name.to_owned());
            remaining &= !bit;
        }
    }
    if remaining != 0 {
        names.push(format!("0x{remaining:x}"));
    }
    if names.is_empty() {
        "-".into()
    } else {
        names.join(",")
    }
}

pub fn device_parts(device: u64) -> (u64, u64) {
    #[cfg(target_os = "macos")]
    {
        ((device >> 24) & 0xff, device & 0xffffff)
    }
    #[cfg(target_os = "linux")]
    {
        (
            ((device >> 8) & 0xfff) | ((device >> 32) & 0xfffff000),
            (device & 0xff) | ((device >> 12) & 0xffffff00),
        )
    }
}
