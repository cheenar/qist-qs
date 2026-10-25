//! Native Unix directory iteration. Names come from readdir's buffered kernel reads;
//! metadata, when needed, is resolved relative to the open directory with fstatat.
use crate::options::{Follow, Indicator, Options, Time};
use serde::Serialize;
use std::{
    ffi::{CStr, CString, OsStr, OsString},
    io,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
#[cfg(target_os = "macos")]
mod macos;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    File,
    Directory,
    Symlink,
    Fifo,
    Socket,
    BlockDevice,
    CharDevice,
    Whiteout,
    Unknown,
}
impl Kind {
    #[allow(clippy::unnecessary_cast)] // mode_t is u16 on Darwin and u32 on Linux.
    fn from_mode(mode: u32) -> Self {
        match mode & libc::S_IFMT as u32 {
            x if x == libc::S_IFREG as u32 => Self::File,
            x if x == libc::S_IFDIR as u32 => Self::Directory,
            x if x == libc::S_IFLNK as u32 => Self::Symlink,
            x if x == libc::S_IFIFO as u32 => Self::Fifo,
            x if x == libc::S_IFSOCK as u32 => Self::Socket,
            x if x == libc::S_IFBLK as u32 => Self::BlockDevice,
            x if x == libc::S_IFCHR as u32 => Self::CharDevice,
            0o160000 => Self::Whiteout,
            _ => Self::Unknown,
        }
    }
    fn from_dtype(dtype: u8) -> Self {
        match dtype {
            libc::DT_REG => Self::File,
            libc::DT_DIR => Self::Directory,
            libc::DT_LNK => Self::Symlink,
            libc::DT_FIFO => Self::Fifo,
            libc::DT_SOCK => Self::Socket,
            libc::DT_BLK => Self::BlockDevice,
            libc::DT_CHR => Self::CharDevice,
            14 => Self::Whiteout,
            _ => Self::Unknown,
        }
    }
    pub fn letter(self) -> char {
        match self {
            Self::File => '-',
            Self::Directory => 'd',
            Self::Symlink => 'l',
            Self::Fifo => 'p',
            Self::Socket => 's',
            Self::BlockDevice => 'b',
            Self::CharDevice => 'c',
            Self::Whiteout => 'w',
            Self::Unknown => '?',
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Timestamp {
    pub seconds: i64,
    pub nanoseconds: i64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Stat {
    pub inode: u64,
    pub device: u64,
    pub mode: u32,
    pub links: u64,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub blocks: u64,
    pub rdev: u64,
    pub modified: Timestamp,
    pub accessed: Timestamp,
    pub changed: Timestamp,
    pub created: Option<Timestamp>,
    pub flags: u32,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: OsString,
    pub path: PathBuf,
    pub kind: Kind,
    pub inode: u64,
    pub stat: Option<Stat>,
    pub link: Option<PathBuf>,
    pub broken: bool,
    pub error: Option<String>,
    pub xattrs: Vec<(OsString, u64)>,
    pub has_xattrs: bool,
    pub acl: Option<String>,
    pub context: Option<String>,
}
impl Entry {
    fn new(path: PathBuf, name: OsString, kind: Kind, inode: u64) -> Self {
        Self {
            path,
            name,
            kind,
            inode,
            stat: None,
            link: None,
            broken: false,
            error: None,
            xattrs: vec![],
            has_xattrs: false,
            acl: None,
            context: None,
        }
    }
    pub fn stamp(&self, time: Time) -> Option<Timestamp> {
        self.stat.as_ref().and_then(|s| match time {
            Time::Modified => Some(s.modified),
            Time::Accessed => Some(s.accessed),
            Time::Changed => Some(s.changed),
            Time::Birth => s.created,
        })
    }
    pub fn executable(&self) -> bool {
        self.kind == Kind::File && self.stat.as_ref().is_some_and(|s| s.mode & 0o111 != 0)
    }
    pub fn indicator(&self, style: Indicator) -> &'static str {
        if style == Indicator::None {
            return "";
        }
        if self.kind == Kind::Directory {
            return "/";
        }
        if style == Indicator::Slash {
            return "";
        }
        match self.kind {
            Kind::Symlink => "@",
            Kind::Socket => "=",
            Kind::Fifo => "|",
            Kind::Whiteout => "%",
            _ if style == Indicator::Classify && self.executable() => "*",
            _ => "",
        }
    }
    pub fn identity(&self) -> Option<(u64, u64)> {
        self.stat.as_ref().map(|s| (s.device, s.inode))
    }
    pub fn dataless(&self) -> bool {
        self.stat
            .as_ref()
            .is_some_and(|s| s.flags & 0x40000000 != 0)
    }
}

pub fn c_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))
}

#[allow(clippy::unnecessary_cast)] // libc's stat field widths differ across supported targets.
fn stat_at(fd: i32, name: &CStr, follow: bool) -> io::Result<Stat> {
    let mut raw = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: name is NUL terminated, raw points to a correctly sized writable stat.
    let result = unsafe {
        libc::fstatat(
            fd,
            name.as_ptr(),
            raw.as_mut_ptr(),
            if follow { 0 } else { libc::AT_SYMLINK_NOFOLLOW },
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    let s = unsafe { raw.assume_init() };
    #[cfg(target_os = "macos")]
    let (modified, accessed, changed, created, flags) = (
        Timestamp {
            seconds: s.st_mtime,
            nanoseconds: s.st_mtime_nsec,
        },
        Timestamp {
            seconds: s.st_atime,
            nanoseconds: s.st_atime_nsec,
        },
        Timestamp {
            seconds: s.st_ctime,
            nanoseconds: s.st_ctime_nsec,
        },
        Some(Timestamp {
            seconds: s.st_birthtime,
            nanoseconds: s.st_birthtime_nsec,
        }),
        s.st_flags,
    );
    #[cfg(target_os = "linux")]
    let (modified, accessed, changed, created, flags) = (
        Timestamp {
            seconds: s.st_mtime as i64,
            nanoseconds: s.st_mtime_nsec as i64,
        },
        Timestamp {
            seconds: s.st_atime as i64,
            nanoseconds: s.st_atime_nsec as i64,
        },
        Timestamp {
            seconds: s.st_ctime as i64,
            nanoseconds: s.st_ctime_nsec as i64,
        },
        None,
        0,
    );
    Ok(Stat {
        inode: s.st_ino as u64,
        device: s.st_dev as u64,
        mode: s.st_mode as u32,
        links: s.st_nlink as u64,
        uid: s.st_uid,
        gid: s.st_gid,
        size: s.st_size.max(0) as u64,
        blocks: s.st_blocks.max(0) as u64,
        rdev: s.st_rdev as u64,
        modified,
        accessed,
        changed,
        created,
        flags,
    })
}

#[cfg(target_os = "linux")]
fn birth_at(fd: i32, name: &CStr, follow: bool) -> Option<Timestamp> {
    let mut data = std::mem::MaybeUninit::<libc::statx>::zeroed();
    // statx is called through syscall so older glibc versions remain link compatible.
    let result = unsafe {
        libc::syscall(
            libc::SYS_statx,
            fd,
            name.as_ptr(),
            if follow { 0 } else { libc::AT_SYMLINK_NOFOLLOW },
            libc::STATX_BTIME,
            data.as_mut_ptr(),
        )
    };
    if result != 0 {
        return None;
    }
    let data = unsafe { data.assume_init() };
    (data.stx_mask & libc::STATX_BTIME != 0).then_some(Timestamp {
        seconds: data.stx_btime.tv_sec,
        nanoseconds: data.stx_btime.tv_nsec as i64,
    })
}

fn hydrate(entry: &mut Entry, fd: i32, name: &CStr, follow: bool, o: &Options) -> io::Result<()> {
    let mut stat = match stat_at(fd, name, follow) {
        Ok(stat) => stat,
        Err(error)
            if follow
                && !(o.compat == crate::options::Compat::Bsd && o.recursive)
                && (o.compat == crate::options::Compat::Bsd
                    || !o.long
                        && !o.blocks
                        && !matches!(
                            o.sort,
                            crate::options::Sort::Size | crate::options::Sort::Time
                        )) =>
        {
            // BSD describes a dangling link even under -L. GNU's name-only
            // listing also keeps it when no target metadata is required.
            stat_at(fd, name, false).map_err(|_| error)?
        }
        Err(error) => return Err(error),
    };
    #[cfg(target_os = "linux")]
    if o.time == Time::Birth || o.format == crate::options::Format::Json {
        stat.created = birth_at(fd, name, follow);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = &mut stat;
    entry.kind = Kind::from_mode(stat.mode);
    entry.inode = stat.inode;
    entry.stat = Some(stat);
    if entry.kind == Kind::Symlink {
        if o.long
            || o.format == crate::options::Format::Json
            || o.format == crate::options::Format::Tree
        {
            entry.link = std::fs::read_link(&entry.path).ok();
        }
        // Only needed for symlink color / target reporting, never for ordinary files.
        entry.broken = stat_at(fd, name, true).is_err();
    }
    if o.long || o.xattrs || o.acl || o.context {
        crate::platform::attributes(entry, o, follow);
    }
    Ok(())
}

pub fn operand(path: &Path, o: &Options, color: bool) -> Entry {
    let mut entry = Entry::new(
        path.to_owned(),
        path.as_os_str().to_owned(),
        Kind::Unknown,
        0,
    );
    let result = (|| {
        let name = c_path(path)?;
        let lstat = stat_at(libc::AT_FDCWD, &name, false)?;
        let kind = Kind::from_mode(lstat.mode);
        let follow = match o.follow {
            Follow::All => true,
            Follow::Arguments if o.compat == crate::options::Compat::Bsd => {
                kind == Kind::Symlink
                    && stat_at(libc::AT_FDCWD, &name, true)
                        .is_ok_and(|s| Kind::from_mode(s.mode) == Kind::Directory)
            }
            Follow::Arguments => true,
            Follow::Never => false,
            Follow::ArgumentDirs => {
                kind == Kind::Symlink
                    && stat_at(libc::AT_FDCWD, &name, true)
                        .is_ok_and(|s| Kind::from_mode(s.mode) == Kind::Directory)
            }
            Follow::Default => !o.directory && !o.long && o.indicator != Indicator::Classify,
        };
        if kind == Kind::Symlink && follow {
            // Default dereference only applies to links to directories (GNU default).
            if o.follow != Follow::Default
                || o.compat == crate::options::Compat::Bsd
                || stat_at(libc::AT_FDCWD, &name, true)
                    .is_ok_and(|s| Kind::from_mode(s.mode) == Kind::Directory)
            {
                return hydrate(&mut entry, libc::AT_FDCWD, &name, true, o);
            }
        }
        entry.kind = kind;
        entry.inode = lstat.inode;
        entry.stat = Some(lstat);
        if o.needs_metadata() || color || kind == Kind::Symlink {
            hydrate(&mut entry, libc::AT_FDCWD, &name, false, o)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        entry.error = Some(e.to_string());
    }
    entry
}

struct Dir(*mut libc::DIR);
impl Drop for Dir {
    fn drop(&mut self) {
        unsafe {
            libc::closedir(self.0);
        }
    }
}

unsafe fn errno_ptr() -> *mut libc::c_int {
    #[cfg(target_os = "macos")]
    {
        unsafe { libc::__error() }
    }
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::__errno_location() }
    }
}

pub fn read_dir(path: &Path, o: &Options, color: bool) -> io::Result<Vec<Entry>> {
    #[cfg(target_os = "macos")]
    if (o.needs_metadata() || color)
        && !o.all
        && !o.whiteouts
        && o.sort != crate::options::Sort::None
        && std::env::var_os("QIST_NO_BULK").is_none()
        && let Ok(entries) = macos::read_dir(path, o)
    {
        return Ok(entries);
    }
    let cpath = c_path(path)?;
    // opendir owns a descriptor and readdir batches getdents64 (Linux) /
    // getdirentries64 (Darwin); Dir closes it even on read/stat failures.
    #[cfg(target_os = "macos")]
    let raw = unsafe {
        if o.whiteouts {
            macos::open_with_whiteouts(&cpath)
        } else {
            libc::opendir(cpath.as_ptr())
        }
    };
    #[cfg(not(target_os = "macos"))]
    let raw = unsafe { libc::opendir(cpath.as_ptr()) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    let dir = Dir(raw);
    let fd = unsafe { libc::dirfd(dir.0) };
    let mut entries = Vec::with_capacity(128);
    let metadata = o.needs_metadata() || color;
    loop {
        unsafe {
            *errno_ptr() = 0;
        }
        let item = unsafe { libc::readdir(dir.0) };
        if item.is_null() {
            let error = unsafe { *errno_ptr() };
            if error != 0 {
                return Err(io::Error::from_raw_os_error(error));
            }
            break;
        }
        // Copy before the next readdir invalidates this borrowed buffer.
        let item = unsafe { &*item };
        let name = unsafe { CStr::from_ptr(item.d_name.as_ptr()) };
        let bytes = name.to_bytes();
        if !include(bytes, name, o) {
            continue;
        }
        let kind = Kind::from_dtype(item.d_type);
        if kind == Kind::Whiteout && !o.whiteouts {
            continue;
        }
        let os_name = OsString::from_vec(bytes.to_vec());
        let mut entry = Entry::new(path.join(&os_name), os_name, kind, item.d_ino as u64);
        if metadata
            || kind == Kind::Unknown
            || o.follow == Follow::All
            || (o.recursive || o.format == crate::options::Format::Tree) && kind == Kind::Directory
        {
            if let Err(e) = hydrate(&mut entry, fd, name, o.follow == Follow::All, o) {
                entry.error = Some(e.to_string());
            }
        } else if kind == Kind::Symlink && o.format == crate::options::Format::Tree {
            entry.link = std::fs::read_link(&entry.path).ok();
        }
        entries.push(entry);
    }
    Ok(entries)
}

fn include(bytes: &[u8], name: &CStr, o: &Options) -> bool {
    let dot = bytes == b"." || bytes == b"..";
    if dot && !o.all {
        return false;
    }
    if bytes.first() == Some(&b'.') && !o.all && !o.almost_all {
        return false;
    }
    if o.ignore_backups && bytes.ends_with(b"~") {
        return false;
    }
    for pattern in &o.ignore {
        if matches_glob(pattern, name) {
            return false;
        }
    }
    if !o.all && !o.almost_all {
        for pattern in &o.hide {
            if matches_glob(pattern, name) {
                return false;
            }
        }
    }
    true
}

fn matches_glob(pattern: &OsStr, name: &CStr) -> bool {
    CString::new(pattern.as_bytes())
        .is_ok_and(|pattern| unsafe { libc::fnmatch(pattern.as_ptr(), name.as_ptr(), 0) } == 0)
}

pub fn permissions(entry: &Entry) -> String {
    let Some(s) = entry.stat.as_ref() else {
        return format!("{}?????????", entry.kind.letter());
    };
    let mut chars = [
        entry.kind.letter(),
        '-',
        '-',
        '-',
        '-',
        '-',
        '-',
        '-',
        '-',
        '-',
    ];
    for group in 0..3 {
        let shift = (2 - group) * 3;
        if s.mode & (4 << shift) != 0 {
            chars[1 + group * 3] = 'r';
        }
        if s.mode & (2 << shift) != 0 {
            chars[2 + group * 3] = 'w';
        }
        if s.mode & (1 << shift) != 0 {
            chars[3 + group * 3] = 'x';
        }
    }
    for (index, bit, on, off) in [
        (3, 0o4000, 's', 'S'),
        (6, 0o2000, 's', 'S'),
        (9, 0o1000, 't', 'T'),
    ] {
        if s.mode & bit != 0 {
            chars[index] = if chars[index] == 'x' { on } else { off };
        }
    }
    chars.iter().collect()
}
