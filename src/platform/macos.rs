//! Darwin behavior implemented through the host's locale and filesystem APIs.
use std::ffi::CStr;

pub fn block_size() -> u64 {
    unsafe extern "C" {
        fn getbsize(header: *mut libc::c_int, block: *mut libc::c_long) -> *mut libc::c_char;
    }
    let mut header = 0;
    let mut size = 512;
    unsafe {
        getbsize(&mut header, &mut size);
    }
    // ls expresses its divisor in 512-byte kernel blocks, rounding down.
    size.max(512) as u64 / 512 * 512
}

pub fn flags(value: u32) -> String {
    unsafe extern "C" {
        fn fflagstostr(flags: libc::c_ulong) -> *mut libc::c_char;
    }
    let text = unsafe { fflagstostr(value as libc::c_ulong) };
    if text.is_null() {
        return "-".into();
    }
    let result = unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned();
    unsafe {
        libc::free(text.cast());
    }
    if result.is_empty() {
        "-".into()
    } else {
        result
    }
}
