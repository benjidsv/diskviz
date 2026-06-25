//! Phase C: bulk directory metadata via `getattrlistbulk` (macOS) or None
//! (all other platforms fall back to per-entry `entry.metadata()`).

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;

/// Metadata for a single directory entry from the bulk path.
pub struct RawMeta {
    /// Allocated bytes on disk (= `meta.blocks() * 512` for files; 0 for dirs).
    pub size: u64,
    /// Modification time, unix seconds.
    pub mtime: i64,
    pub is_dir: bool,
    pub dev: u64,
    pub ino: u64,
    /// Hard-link count (0 for dirs — caller detects dirs via `is_dir`).
    pub nlink: u32,
}

// ── macOS implementation ──────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub fn bulk_dir_meta(dir: &Path) -> Option<HashMap<OsString, RawMeta>> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::ffi::OsStringExt;

    let dir_cstr = CString::new(dir.as_os_str().as_bytes()).ok()?;
    let fd = unsafe { libc::open(dir_cstr.as_ptr(), libc::O_RDONLY) };
    if fd < 0 { return None; }

    // All constants from <sys/attr.h> — verified against the macOS SDK header.
    //
    // Common attrs (pack order = ascending bit position, RETURNED_ATTRS always first):
    //   bit  0 (0x00000001): NAME       → attrreference_t (i32 offset + u32 len = 8 B)
    //   bit  1 (0x00000002): DEVID      → dev_t = i32 (4 B)
    //   bit  3 (0x00000008): OBJTYPE    → u_int32_t (4 B)
    //   bit 10 (0x00000400): MODTIME    → struct timespec (i64 tv_sec + i64 tv_nsec = 16 B)
    //   bit 25 (0x02000000): FILEID     → u_int64_t (8 B)
    //
    // File attrs (only present for VREG; zeroed for dirs with PACK_INVAL_ATTRS):
    //   bit  0 (0x00000001): LINKCOUNT  → u_int32_t (4 B)
    //   bit  2 (0x00000004): ALLOCSIZE  → off_t = i64 (8 B)
    //
    // Directory attrs: not requested — dir size is always 0 from this path
    // (negligible vs. child aggregation; avoids ATTR_DIR_ALLOCSIZE "not supported").

    const ATTR_BIT_MAP_COUNT:      u16 = 5;
    const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;
    const ATTR_CMN_NAME:           u32 = 0x0000_0001;
    const ATTR_CMN_DEVID:          u32 = 0x0000_0002;
    const ATTR_CMN_OBJTYPE:        u32 = 0x0000_0008;
    const ATTR_CMN_MODTIME:        u32 = 0x0000_0400;
    const ATTR_CMN_FILEID:         u32 = 0x0200_0000;
    const ATTR_FILE_LINKCOUNT:     u32 = 0x0000_0001;
    const ATTR_FILE_ALLOCSIZE:     u32 = 0x0000_0004;
    const FSOPT_NOFOLLOW:          u64 = 0x0000_0001;
    const FSOPT_PACK_INVAL_ATTRS:  u64 = 0x0000_0008;
    const VDIR: u32 = 2;

    #[repr(C)]
    struct AttrList {
        bitmapcount: u16,
        reserved:    u16,
        commonattr:  u32,
        volattr:     u32,
        dirattr:     u32,
        fileattr:    u32,
        forkattr:    u32,
    }

    extern "C" {
        fn getattrlistbulk(
            dirfd:           libc::c_int,
            alist:           *const AttrList,
            attributeBuffer: *mut libc::c_void,
            bufferSize:      libc::size_t,
            options:         u64,
        ) -> libc::c_int;
    }

    let alist = AttrList {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved:    0,
        commonattr:  ATTR_CMN_RETURNED_ATTRS
                   | ATTR_CMN_NAME
                   | ATTR_CMN_DEVID
                   | ATTR_CMN_OBJTYPE
                   | ATTR_CMN_MODTIME
                   | ATTR_CMN_FILEID,
        volattr:     0,
        dirattr:     0,
        fileattr:    ATTR_FILE_LINKCOUNT | ATTR_FILE_ALLOCSIZE,
        forkattr:    0,
    };

    // 256 KB — sufficient for ~2 000 typical entries per call.
    const BUF_SIZE: usize = 256 * 1024;
    let mut buf = vec![0u8; BUF_SIZE];
    let mut result: HashMap<OsString, RawMeta> = HashMap::new();

    loop {
        let n = unsafe {
            getattrlistbulk(
                fd,
                &alist,
                buf.as_mut_ptr() as *mut libc::c_void,
                BUF_SIZE,
                FSOPT_NOFOLLOW | FSOPT_PACK_INVAL_ATTRS,
            )
        };
        if n <= 0 { break; }

        // Safety: getattrlistbulk wrote `n` complete records into buf.
        // read_unaligned is used throughout because getattrlistbulk packs
        // attributes without alignment padding.
        let mut ptr = buf.as_ptr();

        for _ in 0..n {
            let record_start = ptr;

            // ── Fixed-size header ──────────────────────────────────────────
            // u32: total record length (covers fixed fields + variable name data).
            let total_len = unsafe { (ptr as *const u32).read_unaligned() } as usize;
            ptr = unsafe { ptr.add(4) };

            // attribute_set_t: 5 × u32 = 20 bytes. Skipped — PACK_INVAL_ATTRS
            // guarantees all requested attrs are present in every record.
            ptr = unsafe { ptr.add(20) };

            // attrreference_t NAME: i32 dataoffset (from this field) + u32 length.
            let name_ref_ptr = ptr;
            let name_offset  = unsafe { (ptr as *const i32).read_unaligned() };
            let name_len     = unsafe { (ptr.add(4) as *const u32).read_unaligned() } as usize;
            ptr = unsafe { ptr.add(8) };

            // dev_t DEVID (bit 1) — i32 on macOS.
            let devid = unsafe { (ptr as *const i32).read_unaligned() };
            ptr = unsafe { ptr.add(4) };

            // u_int32_t OBJTYPE (bit 3).
            let objtype = unsafe { (ptr as *const u32).read_unaligned() };
            ptr = unsafe { ptr.add(4) };

            // struct timespec MODTIME (bit 10): tv_sec i64 + tv_nsec i64 = 16 B.
            let tv_sec = unsafe { (ptr as *const i64).read_unaligned() };
            ptr = unsafe { ptr.add(16) };

            // u_int64_t FILEID (bit 25).
            let fileid = unsafe { (ptr as *const u64).read_unaligned() };
            ptr = unsafe { ptr.add(8) };

            // ── File attrs (fileattr group, bit order) ────────────────────
            // u_int32_t LINKCOUNT (file bit 0) — 0 for dirs with PACK_INVAL.
            let nlink = unsafe { (ptr as *const u32).read_unaligned() };
            ptr = unsafe { ptr.add(4) };

            // off_t ALLOCSIZE (file bit 2) — 0 for dirs with PACK_INVAL.
            let allocsize = unsafe { (ptr as *const i64).read_unaligned() };

            // ── Variable-length name ──────────────────────────────────────
            // name_offset is relative to name_ref_ptr; attr_length includes '\0'.
            let name_data = unsafe { name_ref_ptr.offset(name_offset as isize) };
            let name_byte_len = if name_len > 0 { name_len - 1 } else { 0 };
            let name_bytes = unsafe { std::slice::from_raw_parts(name_data, name_byte_len) };
            let name = OsString::from_vec(name_bytes.to_vec());

            let is_dir = objtype == VDIR;
            // Files: allocated blocks × 512 (off_t = signed, but always ≥ 0).
            // Dirs: 0 — their size comes from bottom-up child aggregation.
            let size = if is_dir { 0 } else { allocsize.max(0) as u64 };

            if !name.is_empty() {
                result.insert(name, RawMeta {
                    size,
                    mtime: tv_sec,
                    is_dir,
                    dev: devid as u32 as u64, // dev_t is i32; zero-extend via u32
                    ino: fileid,
                    nlink,
                });
            }

            // Advance to the next record using the authoritative total_len.
            ptr = unsafe { record_start.add(total_len) };
        }
    }

    unsafe { libc::close(fd) };
    Some(result)
}

// ── Non-macOS stub ────────────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
pub fn bulk_dir_meta(_dir: &Path) -> Option<HashMap<OsString, RawMeta>> {
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    #[cfg(target_os = "macos")]
    fn dirmeta_parity() {
        use std::ffi::OsStr;
        use std::os::unix::fs::MetadataExt;

        let dir = std::env::temp_dir()
            .join(format!("diskviz_dirmeta_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("a.txt"), vec![0u8; 4096]).unwrap();
        fs::write(dir.join("b.bin"), vec![0u8; 8192]).unwrap();
        fs::create_dir(dir.join("sub")).unwrap();

        let result = bulk_dir_meta(&dir).expect("bulk_dir_meta should succeed on macOS");

        for name in ["a.txt", "b.bin"] {
            let rm   = result.get(OsStr::new(name))
                .unwrap_or_else(|| panic!("{name} missing from bulk result"));
            let meta = fs::symlink_metadata(dir.join(name)).unwrap();

            assert!(!rm.is_dir, "{name}: should not be dir");
            assert_eq!(rm.ino, meta.ino(),        "{name}: inode mismatch");
            assert_eq!(rm.size, meta.blocks() * 512, "{name}: size mismatch");
            assert!((rm.mtime - meta.mtime()).abs() <= 1, "{name}: mtime mismatch");
        }

        // Directory entry
        let sub_rm   = result.get(OsStr::new("sub"))
            .expect("sub dir missing from bulk result");
        let sub_meta = fs::symlink_metadata(dir.join("sub")).unwrap();
        assert!(sub_rm.is_dir, "sub: should be dir");
        assert_eq!(sub_rm.ino, sub_meta.ino(), "sub: inode mismatch");

        let _ = fs::remove_dir_all(&dir);
    }
}
