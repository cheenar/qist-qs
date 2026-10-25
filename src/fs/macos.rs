//! Darwin's bulk metadata path. Packed records are decoded with checked byte
//! reads, never aligned pointer casts. Filesystems without these attributes
//! automatically fall back to fstatat / readdir in the parent module.
use super::*;
use std::{fs::File, os::fd::AsRawFd};

#[repr(C)]
struct AttrList {
    count: u16,
    reserved: u16,
    common: u32,
    volume: u32,
    directory: u32,
    file: u32,
    fork: u32,
}

const COMMON: u32 = 0x8000_0000
    | 0x2000_0000
    | 0x0000_0001
    | 0x0000_0002
    | 0x0000_0008
    | 0x0000_0200
    | 0x0000_0400
    | 0x0000_0800
    | 0x0000_1000
    | 0x0000_8000
    | 0x0001_0000
    | 0x0002_0000
    | 0x0004_0000
    | 0x0200_0000;
const DIRECTORY: u32 = 0x1 | 0x4 | 0x8 | 0x20;
const FILE: u32 = 0x1 | 0x4 | 0x20 | 0x200;

pub(super) unsafe fn open_with_whiteouts(path: &CStr) -> *mut libc::DIR {
    unsafe extern "C" {
        #[cfg_attr(target_arch = "x86_64", link_name = "__opendir2$INODE64")]
        fn __opendir2(path: *const libc::c_char, flags: libc::c_int) -> *mut libc::DIR;
    }
    // Ordinary opendir enables DTF_HIDEW. -W must clear it while retaining
    // DTF_NODUP and DTF_REWIND for union-directory iteration, like fts.
    unsafe { __opendir2(path.as_ptr(), 0x2 | 0x4) }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn array<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let bytes = self.bytes.get(self.at..self.at + N).ok_or_else(invalid)?;
        self.at += N;
        Ok(bytes.try_into().unwrap())
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_ne_bytes(self.array()?))
    }
    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_ne_bytes(self.array()?))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_ne_bytes(self.array()?))
    }
    fn timestamp(&mut self) -> io::Result<Timestamp> {
        Ok(Timestamp {
            seconds: i64::from_ne_bytes(self.array()?),
            nanoseconds: i64::from_ne_bytes(self.array()?),
        })
    }
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid getattrlistbulk record")
}

pub(super) fn read_dir(path: &Path, o: &Options) -> io::Result<Vec<Entry>> {
    read_dir_counted(path, o).map(|(entries, _)| entries)
}

fn read_dir_counted(path: &Path, o: &Options) -> io::Result<(Vec<Entry>, usize)> {
    let directory = File::open(path)?;
    let fd = directory.as_raw_fd();
    let mut attrs = AttrList {
        count: 5,
        reserved: 0,
        common: COMMON,
        volume: 0,
        directory: DIRECTORY,
        file: FILE,
        fork: 0,
    };
    let mut buffer = vec![0u8; 128 * 1024];
    let mut entries = Vec::with_capacity(128);
    let mut fallbacks = 0;
    loop {
        // FSOPT_PACK_INVAL_ATTRS keeps the requested field layout fixed even for
        // inapplicable fields (directory fields on regular files and vice versa).
        let count = unsafe {
            libc::getattrlistbulk(
                fd,
                (&mut attrs as *mut AttrList).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                0x8,
            )
        };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        if count == 0 {
            break;
        }
        let mut offset = 0;
        for _ in 0..count {
            let length = u32::from_ne_bytes(
                buffer
                    .get(offset..offset + 4)
                    .ok_or_else(invalid)?
                    .try_into()
                    .unwrap(),
            ) as usize;
            if length < 132 {
                return Err(invalid());
            }
            let record = buffer.get(offset..offset + length).ok_or_else(invalid)?;
            offset += length;
            let mut c = Cursor {
                bytes: record,
                at: 4,
            };
            let common = c.u32()?;
            let _volume = c.u32()?;
            let dir_attrs = c.u32()?;
            let file_attrs = c.u32()?;
            let _fork = c.u32()?;
            let error = c.u32()?;
            let name_reference = c.at;
            let name_offset = c.i32()?;
            let name_length = c.u32()? as usize;
            let start = (name_reference as i64)
                .checked_add(name_offset as i64)
                .filter(|n| *n >= 0)
                .ok_or_else(invalid)? as usize;
            let name_bytes = record
                .get(start..start.checked_add(name_length).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
            let name = CStr::from_bytes_with_nul(name_bytes).map_err(|_| invalid())?;
            if !include(name.to_bytes(), name, o) {
                continue;
            }
            let device = c.u32()? as u64;
            let object_type = c.u32()?;
            let created = c.timestamp()?;
            let modified = c.timestamp()?;
            let changed = c.timestamp()?;
            let accessed = c.timestamp()?;
            let uid = c.u32()?;
            let gid = c.u32()?;
            let access = c.u32()?;
            let flags = c.u32()?;
            let inode = c.u64()?;
            // Darwin omits the entire inapplicable group even when packing
            // invalid attributes: directories have directory fields, all other
            // vnode types have file fields.
            let (links, mount, allocation, rdev, size) = if object_type == 2 {
                (c.u32()? as u64, c.u32()?, c.u64()?, 0, c.u64()?)
            } else {
                (c.u32()? as u64, 0, c.u64()?, c.u32()? as u64, c.u64()?)
            };
            let mode_type = match object_type {
                1 => 0o100000,
                2 => 0o040000,
                3 => 0o060000,
                4 => 0o020000,
                5 => 0o120000,
                6 => 0o140000,
                7 => 0o010000,
                _ => 0,
            };
            let kind = Kind::from_mode(mode_type);
            let os_name = OsString::from_vec(name.to_bytes().to_vec());
            let mut entry = Entry::new(path.join(&os_name), os_name, kind, inode);
            // Mountpoints and firmlinks must describe the mounted/target root,
            // rather than the underlying object returned by the bulk syscall.
            let required = COMMON & !(0x8000_0000 | 0x2000_0000 | 0x0000_0200);
            let complete = common & required == required
                && if kind == Kind::Directory {
                    dir_attrs & DIRECTORY == DIRECTORY
                } else {
                    file_attrs & (FILE & !0x20) == (FILE & !0x20)
                };
            if error != 0
                || !complete
                || kind == Kind::Unknown
                || kind == Kind::Symlink
                // ATTR_DIR_LINKCOUNT excludes synthetic . and .., whereas
                // stat's nlink is filesystem-specific. Preserve ls's value.
                || kind == Kind::Directory && (o.long || o.format == crate::options::Format::Json)
                || mount & 1 != 0
                || flags & 0x0080_0000 != 0
            {
                fallbacks += 1;
                if let Err(error) = hydrate(&mut entry, fd, name, o.follow == Follow::All, o) {
                    entry.error = Some(error.to_string());
                }
            } else {
                entry.stat = Some(Stat {
                    inode,
                    device,
                    mode: mode_type | access,
                    links,
                    uid,
                    gid,
                    size,
                    blocks: allocation / 512,
                    rdev,
                    modified,
                    accessed,
                    changed,
                    created: (common & 0x200 != 0).then_some(created),
                    flags,
                });
                if o.long || o.xattrs || o.acl || o.context {
                    crate::platform::attributes(&mut entry, o, false);
                }
            }
            entries.push(entry);
        }
    }
    Ok((entries, fallbacks))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bulk_metadata_matches_stat_and_keeps_regular_files_batched() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("regular"), b"hello").unwrap();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        let o = Options::parse(vec!["-ln".into()], &crate::config::Config::default()).unwrap();
        let (entries, fallbacks) = read_dir_counted(dir.path(), &o).unwrap();
        assert_eq!(
            fallbacks, 1,
            "only the directory should require a stat fallback for its link count"
        );
        for entry in entries {
            let native = stat_at(libc::AT_FDCWD, &c_path(&entry.path).unwrap(), false).unwrap();
            let bulk = entry.stat.unwrap();
            assert_eq!(bulk.size, native.size);
            assert_eq!(bulk.blocks, native.blocks);
            assert_eq!(bulk.mode, native.mode);
            assert_eq!(bulk.links, native.links);
            assert_eq!(bulk.inode, native.inode);
            assert_eq!(bulk.device, native.device);
            assert_eq!(bulk.modified.seconds, native.modified.seconds);
            assert_eq!(bulk.modified.nanoseconds, native.modified.nanoseconds);
        }
    }
}
