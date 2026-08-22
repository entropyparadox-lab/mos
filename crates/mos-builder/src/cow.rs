use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tracing::info;

/// Copy-on-Write (CoW) 기반의 초고속 디스크 복제 유틸리티
pub struct CoWCloner;

impl CoWCloner {
    /// 원본 파일을 대상 경로에 CoW (macOS clonefile / Linux FICLONE) 복제
    pub fn clone_file(src: &Path, dst: &Path) -> Result<()> {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }

        // 1. macOS APFS clonefile(2) 시도
        #[cfg(target_os = "macos")]
        {
            use std::ffi::CString;
            use std::os::unix::ffi::OsStrExt;

            let src_c = CString::new(src.as_os_str().as_bytes())?;
            let dst_c = CString::new(dst.as_os_str().as_bytes())?;

            // libc::clonefile(src, dst, flags) (flags: 0 = regular clone)
            extern "C" {
                fn clonefile(
                    src: *const std::os::raw::c_char,
                    dst: *const std::os::raw::c_char,
                    flags: u32,
                ) -> std::os::raw::c_int;
            }

            let ret = unsafe { clonefile(src_c.as_ptr(), dst_c.as_ptr(), 0) };
            if ret == 0 {
                info!(src = %src.display(), dst = %dst.display(), "Successfully cloned file using macOS APFS clonefile");
                return Ok(());
            }
            // 실패 시 일반 복사로 fallback
        }

        // 2. Linux FICLONE ioctl 시도
        #[cfg(target_os = "linux")]
        {
            use std::fs::OpenOptions;
            use std::os::unix::fs::OpenOptionsExt;
            use std::os::unix::io::AsRawFd;

            let src_file = OpenOptions::new().read(true).open(src)?;
            let dst_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o644)
                .open(dst)?;

            // FICLONE ioctl: 0x40049409 (FICLONE)
            const FICLONE: u64 = 0x40049409;
            let ret =
                unsafe { libc::ioctl(dst_file.as_raw_fd(), FICLONE as _, src_file.as_raw_fd()) };
            if ret == 0 {
                info!(src = %src.display(), dst = %dst.display(), "Successfully cloned file using Linux FICLONE reflink");
                return Ok(());
            }
        }

        // 3. Fallback: 일반 파일 복사
        fs::copy(src, dst).with_context(|| {
            format!(
                "Failed to copy file from {} to {}",
                src.display(),
                dst.display()
            )
        })?;
        info!(src = %src.display(), dst = %dst.display(), "Cloned file using standard copy fallback");
        Ok(())
    }
}
