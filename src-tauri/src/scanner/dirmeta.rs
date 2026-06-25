//! Bulk directory metadata: `getattrlistbulk` (macOS),
//! `GetFileInformationByHandleEx` (Windows), or `None` (other platforms fall
//! back to per-entry `entry.metadata()`).

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
    /// True when this entry is a reparse point / junction (Windows only).
    /// Always `false` on macOS; callers should not recurse into reparse points.
    pub is_reparse: bool,
}

// ── macOS implementation ──────────────────────────────────────────────────────

// getattrlistbulk attributes — from <sys/attr.h>, verified against the macOS SDK.
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

#[cfg(target_os = "macos")]
const MACOS_BUF_SIZE: usize = 256 * 1024;

// The getattrlistbulk buffer (256 KB) is reused across every call on the same
// rayon worker thread.  Allocating it per-call on 82k dirs wastes ~21 GB of
// malloc/memset churn.  Safe: the buffer is fully parsed into an owned Vec before
// bulk_dir_meta_fd returns, so no borrow outlives the call and there is no
// re-entrant borrow risk (rayon workers steal work only when idle, not mid-call).
#[cfg(target_os = "macos")]
thread_local! {
    static BULK_BUF: std::cell::RefCell<Vec<u8>> =
        std::cell::RefCell::new(vec![0u8; MACOS_BUF_SIZE]);
}

/// Enumerate `dir` using `getattrlistbulk`.  Opens the fd internally.
#[cfg(target_os = "macos")]
pub fn bulk_dir_meta(dir: &Path) -> Option<Vec<(String, RawMeta)>> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let dir_cstr = CString::new(dir.as_os_str().as_bytes()).ok()?;
    let fd = unsafe { libc::open(dir_cstr.as_ptr(), libc::O_RDONLY) };
    if fd < 0 { return None; }
    let result = bulk_dir_meta_fd(fd);
    unsafe { libc::close(fd) };
    result
}

/// Enumerate the directory referred to by the already-open `fd` using
/// `getattrlistbulk`.  The fd is **not** closed — caller is responsible.
/// Returns `None` on I/O error so callers can fall back to `readdir_meta`.
#[cfg(target_os = "macos")]
pub fn bulk_dir_meta_fd(fd: libc::c_int) -> Option<Vec<(String, RawMeta)>> {
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

    BULK_BUF.with(|cell| {
        let mut buf = cell.borrow_mut();
        let mut result: Vec<(String, RawMeta)> = Vec::new();

        loop {
            let n = unsafe {
                getattrlistbulk(
                    fd,
                    &alist,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    MACOS_BUF_SIZE,
                    FSOPT_NOFOLLOW | FSOPT_PACK_INVAL_ATTRS,
                )
            };
            if n == 0 { break; }
            if n < 0 { return None; }

            // Safety: getattrlistbulk wrote `n` complete records into buf.
            // read_unaligned is used throughout because getattrlistbulk packs
            // attributes without alignment padding.
            let mut ptr = buf.as_ptr();

            for _ in 0..n {
                let record_start = ptr;

                let total_len = unsafe { (ptr as *const u32).read_unaligned() } as usize;
                ptr = unsafe { ptr.add(4) };
                // attribute_set_t: 5 × u32 = 20 bytes — skipped (PACK_INVAL_ATTRS
                // guarantees all requested attrs are present in every record).
                ptr = unsafe { ptr.add(20) };

                // attrreference_t NAME: i32 dataoffset (from this field) + u32 length.
                let name_ref_ptr = ptr;
                let name_offset  = unsafe { (ptr as *const i32).read_unaligned() };
                let name_len     = unsafe { (ptr.add(4) as *const u32).read_unaligned() } as usize;
                ptr = unsafe { ptr.add(8) };

                let devid = unsafe { (ptr as *const i32).read_unaligned() };
                ptr = unsafe { ptr.add(4) };
                let objtype = unsafe { (ptr as *const u32).read_unaligned() };
                ptr = unsafe { ptr.add(4) };
                let tv_sec = unsafe { (ptr as *const i64).read_unaligned() };
                ptr = unsafe { ptr.add(16) };
                let fileid = unsafe { (ptr as *const u64).read_unaligned() };
                ptr = unsafe { ptr.add(8) };
                let nlink = unsafe { (ptr as *const u32).read_unaligned() };
                ptr = unsafe { ptr.add(4) };
                let allocsize = unsafe { (ptr as *const i64).read_unaligned() };

                // name_offset is relative to name_ref_ptr; name_len includes '\0'.
                let name_data = unsafe { name_ref_ptr.offset(name_offset as isize) };
                let name_byte_len = if name_len > 0 { name_len - 1 } else { 0 };
                let name_bytes = unsafe { std::slice::from_raw_parts(name_data, name_byte_len) };
                // APFS/HFS+ guarantees UTF-8 filenames; lossy is lossless in practice.
                let name = String::from_utf8_lossy(name_bytes).into_owned();

                let is_dir = objtype == VDIR;
                let size = if is_dir { 0 } else { allocsize.max(0) as u64 };

                if !name.is_empty() {
                    result.push((name, RawMeta {
                        size,
                        mtime: tv_sec,
                        is_dir,
                        dev: devid as u32 as u64, // dev_t is i32; zero-extend via u32
                        ino: fileid,
                        nlink,
                        is_reparse: false,
                    }));
                }

                ptr = unsafe { record_start.add(total_len) };
            }
        }

        Some(result)
    })
}

// ── Windows implementation ────────────────────────────────────────────────────

// 64 KB reused per rayon worker thread — avoids per-call malloc churn.
#[cfg(target_os = "windows")]
const WIN_BUF_SIZE: usize = 64 * 1024;

#[cfg(target_os = "windows")]
thread_local! {
    static WIN_BUF: std::cell::RefCell<Vec<u8>> =
        std::cell::RefCell::new(vec![0u8; WIN_BUF_SIZE]);
}

#[cfg(target_os = "windows")]
pub fn bulk_dir_meta(dir: &Path) -> Option<Vec<(String, RawMeta)>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
        ERROR_NO_MORE_FILES,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, GetFileInformationByHandleEx,
        BY_HANDLE_FILE_INFORMATION, FILE_ID_BOTH_DIR_INFO,
        FileIdBothDirectoryInfo,
        FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };

    // ── Open the directory ─────────────────────────────────────────────────
    // FILE_FLAG_BACKUP_SEMANTICS is required to open a directory handle.
    // FILE_FLAG_OPEN_REPARSE_POINT prevents following junctions/symlinks.
    let wide: Vec<u16> = dir.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0u16))
        .collect();

    let handle: HANDLE = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE { return None; }

    // ── Volume serial number for dev ───────────────────────────────────────
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };
    let dev = if ok != 0 { info.dwVolumeSerialNumber as u64 } else { 0u64 };

    // ── Bulk enumeration using the thread-local buffer ─────────────────────
    // FILETIME epoch offset: 100-ns ticks from 1601-01-01 to 1970-01-01.
    const FILETIME_TO_UNIX_SECS: i64 = 11_644_473_600i64;

    // The closure returns None on mid-enumeration error; CloseHandle is always
    // called after it regardless of which path exits the closure.
    let result = WIN_BUF.with(|cell| {
        let mut buf = cell.borrow_mut();
        let mut result: Vec<(String, RawMeta)> = Vec::new();

        loop {
            let ok = unsafe {
                GetFileInformationByHandleEx(
                    handle,
                    FileIdBothDirectoryInfo,
                    buf.as_mut_ptr() as *mut _,
                    WIN_BUF_SIZE as u32,
                )
            };
            if ok == 0 {
                // ERROR_NO_MORE_FILES means we've seen everything.
                if unsafe { GetLastError() } == ERROR_NO_MORE_FILES { break; }
                // Any other error (e.g. ERROR_MORE_DATA if a record doesn't fit the
                // buffer) — return None so the caller can fall back to readdir_meta
                // rather than silently returning a truncated child list.
                return None;
            }

            // Walk the chain of FILE_ID_BOTH_DIR_INFO records in the buffer.
            // Each record's NextEntryOffset gives the byte distance to the next;
            // 0 means this is the last record in this buffer fill.
            let mut offset = 0usize;
            loop {
                // Safety: buf is WIN_BUF_SIZE bytes; offset advances by NextEntryOffset
                // which is set by the OS and guaranteed to keep records in-bounds.
                let rec_ptr = unsafe {
                    buf.as_ptr().add(offset) as *const FILE_ID_BOTH_DIR_INFO
                };
                let rec = unsafe { &*rec_ptr };

                let next = rec.NextEntryOffset as usize;

                // FILE_ID_BOTH_DIR_INFO::FileName is a flexible array member;
                // FileNameLength is in bytes (UTF-16 units × 2).
                let name_len_chars = rec.FileNameLength as usize / 2;
                let name_ptr = rec.FileName.as_ptr();
                // Safety: name_ptr points into buf which is live for this loop body.
                let name_wide = unsafe { std::slice::from_raw_parts(name_ptr, name_len_chars) };
                let name = String::from_utf16_lossy(name_wide);

                // Skip the mandatory "." and ".." entries that Win32 always includes.
                if name != "." && name != ".." && !name.is_empty() {
                    let attrs = rec.FileAttributes;
                    let is_dir     = attrs & FILE_ATTRIBUTE_DIRECTORY != 0;
                    let is_reparse = attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0;

                    // AllocationSize: i64 (LARGE_INTEGER). 0 for dirs.
                    let alloc = rec.AllocationSize;
                    let size  = if is_dir { 0 } else { alloc.max(0) as u64 };

                    // LastWriteTime: FILETIME stored as i64 (100-ns ticks since 1601).
                    let ft    = rec.LastWriteTime;
                    let mtime = (ft / 10_000_000) - FILETIME_TO_UNIX_SECS;

                    // FileId: i64 unique file ID within the volume.
                    let ino = rec.FileId as u64;

                    result.push((name, RawMeta {
                        size,
                        mtime,
                        is_dir,
                        dev,
                        ino,
                        nlink: 1, // Not available in FileIdBothDirectoryInfo
                        is_reparse,
                    }));
                }

                if next == 0 { break; }
                offset += next;
            }
        }

        Some(result)
    });

    unsafe { CloseHandle(handle) };
    result
}

// ── Non-macOS, non-Windows stub ───────────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn bulk_dir_meta(_dir: &Path) -> Option<Vec<(String, RawMeta)>> {
    None
}

// ── Cross-platform readdir fallback ──────────────────────────────────────────

/// Enumerate `dir` via `std::fs::read_dir` + `symlink_metadata`, returning the
/// same `Vec<(String, RawMeta)>` shape as `bulk_dir_meta`.
///
/// This is the *fallback* path: called by both native walkers when
/// `bulk_dir_meta` returns `None` (open-failure or mid-enumeration error),
/// so the walker can still list the directory's children rather than dropping
/// the whole subtree.  The semantics deliberately mirror what the jwalk path
/// does internally — `symlink_metadata` (no link following), allocated-block
/// size on Unix / logical size on Windows — so results converge with the
/// `Walker::Jwalk` baseline.
///
/// Returns `None` only if `read_dir` itself fails (dir is genuinely unreadable
/// by any path, matching what jwalk would skip).
pub fn readdir_meta(dir: &Path) -> Option<Vec<(String, RawMeta)>> {
    use std::time::UNIX_EPOCH;

    let rd = std::fs::read_dir(dir).ok()?;
    let mut result = Vec::new();

    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();

        // symlink_metadata = lstat: never follows symlinks/junctions.
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let ft       = meta.file_type();
        let is_dir   = ft.is_dir();
        // On Windows, is_symlink() covers both symlinks and junctions (reparse
        // points) — these are the entries the walker must not recurse into.
        // On Unix, symlinks to directories are also caught here.
        let is_reparse = ft.is_symlink();

        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Size semantics mirror the jwalk path in mod.rs:
        //   Unix  → allocated blocks × 512  (mod.rs:433)
        //   Windows → logical file length   (mod.rs:454)
        //   Dirs  → 0 always (child-aggregated later)
        let size = if is_dir {
            0u64
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                meta.blocks().saturating_mul(512)
            }
            #[cfg(not(unix))]
            {
                meta.len()
            }
        };

        // dev/ino/nlink for hardlink dedup and visited-set checks.
        // macOS: dev_t is i32 — zero-extend via u32 to match bulk_dir_meta's
        //   `devid as u32 as u64` convention so the visited set stays consistent.
        // Windows: nlink is always 1 (not available cheaply); dev/ino unused
        //   since the Windows walker performs no directory dedup.
        #[cfg(target_os = "macos")]
        let (dev, ino, nlink) = {
            use std::os::unix::fs::MetadataExt;
            (meta.dev() as u32 as u64, meta.ino(), meta.nlink() as u32)
        };
        #[cfg(all(unix, not(target_os = "macos")))]
        let (dev, ino, nlink) = {
            use std::os::unix::fs::MetadataExt;
            (meta.dev(), meta.ino(), meta.nlink() as u32)
        };
        #[cfg(not(unix))]
        let (dev, ino, nlink) = (0u64, 0u64, 1u32);

        result.push((
            name,
            RawMeta { size, mtime, is_dir, dev, ino, nlink, is_reparse },
        ));
    }

    Some(result)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    #[cfg(target_os = "macos")]
    fn dirmeta_parity() {
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
            let rm = result.iter().find(|(n, _)| n == name)
                .map(|(_, m)| m)
                .unwrap_or_else(|| panic!("{name} missing from bulk result"));
            let meta = fs::symlink_metadata(dir.join(name)).unwrap();

            assert!(!rm.is_dir, "{name}: should not be dir");
            assert_eq!(rm.ino, meta.ino(),           "{name}: inode mismatch");
            assert_eq!(rm.size, meta.blocks() * 512, "{name}: size mismatch");
            assert!((rm.mtime - meta.mtime()).abs() <= 1, "{name}: mtime mismatch");
            assert!(!rm.is_reparse, "{name}: regular file is not a reparse point");
        }

        // Directory entry
        let sub_rm = result.iter().find(|(n, _)| n == "sub")
            .map(|(_, m)| m)
            .expect("sub dir missing from bulk result");
        let sub_meta = fs::symlink_metadata(dir.join("sub")).unwrap();
        assert!(sub_rm.is_dir, "sub: should be dir");
        assert_eq!(sub_rm.ino, sub_meta.ino(), "sub: inode mismatch");
        assert!(!sub_rm.is_reparse, "sub: regular dir is not a reparse point");

        let _ = fs::remove_dir_all(&dir);
    }

    /// Windows dirmeta: basic sanity check (file/dir detection, non-zero size,
    /// mtime in a plausible range). Run on Windows only.
    #[test]
    #[cfg(target_os = "windows")]
    fn dirmeta_windows_basic() {
        let dir = std::env::temp_dir()
            .join(format!("diskviz_dirmeta_win_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("a.txt"), vec![0u8; 4096]).unwrap();
        fs::create_dir(dir.join("sub")).unwrap();

        let result = bulk_dir_meta(&dir)
            .expect("bulk_dir_meta should succeed on Windows");

        let a = result.iter().find(|(n, _)| n == "a.txt")
            .map(|(_, m)| m)
            .expect("a.txt missing from result");
        assert!(!a.is_dir, "a.txt should not be a directory");
        assert!(a.size > 0, "a.txt should have non-zero allocated size");
        // mtime should be a reasonably recent unix timestamp (after year 2000).
        assert!(a.mtime > 946_684_800, "mtime should be after year 2000");
        assert!(!a.is_reparse, "a.txt is not a reparse point");

        let sub = result.iter().find(|(n, _)| n == "sub")
            .map(|(_, m)| m)
            .expect("sub dir missing from result");
        assert!(sub.is_dir, "sub should be a directory");
        assert_eq!(sub.size, 0, "dir size is always 0 from bulk path");
        assert!(!sub.is_reparse, "regular dir is not a reparse point");

        // Neither "." nor ".." should appear.
        assert!(!result.iter().any(|(n, _)| n == "."),  ". must be filtered");
        assert!(!result.iter().any(|(n, _)| n == ".."), ".. must be filtered");

        let _ = fs::remove_dir_all(&dir);
    }

    /// `readdir_meta` and `bulk_dir_meta` must agree on entry names and
    /// is_dir classification for a fixture tree.  Sizes may differ (allocated
    /// blocks vs. logical length) so we only compare counts and dir/file flags.
    #[test]
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn readdir_meta_parity() {
        let dir = std::env::temp_dir()
            .join(format!("diskviz_rdmeta_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"),  vec![0u8; 4096]).unwrap();
        fs::write(dir.join("b.bin"),  vec![0u8; 8192]).unwrap();
        fs::create_dir(dir.join("sub")).unwrap();

        let bulk = bulk_dir_meta(&dir).expect("bulk_dir_meta should succeed");
        let rdir = readdir_meta(&dir).expect("readdir_meta should succeed");

        // Same set of names (readdir never returns "." or "..").
        let mut bulk_names: Vec<_> = bulk.iter().map(|(n, _)| n.clone()).collect();
        let mut rdir_names: Vec<_> = rdir.iter().map(|(n, _)| n.clone()).collect();
        bulk_names.sort();
        rdir_names.sort();
        assert_eq!(bulk_names, rdir_names, "bulk and readdir must return same entry names");

        // is_dir must agree for every entry.
        for name in ["a.txt", "b.bin", "sub"] {
            let b = bulk.iter().find(|(n, _)| n == name).map(|(_, m)| m)
                .unwrap_or_else(|| panic!("{name} missing from bulk"));
            let r = rdir.iter().find(|(n, _)| n == name).map(|(_, m)| m)
                .unwrap_or_else(|| panic!("{name} missing from readdir"));
            assert_eq!(b.is_dir, r.is_dir, "{name}: is_dir mismatch between bulk and readdir");
        }

        let _ = fs::remove_dir_all(&dir);
    }
}
