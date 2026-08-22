use anyhow::{Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use tar::Archive;
use tracing::info;

/// 순수 Rust 기반 Rootless OCI 레이어 언패커
pub struct OciLayerUnpacker;

impl OciLayerUnpacker {
    /// OCI / Docker tar 아카이브를 대상 디렉터리에 풀면서 .wh. 화이트아웃 마커 적용
    pub fn unpack_layer<R: Read>(reader: R, target_dir: &Path) -> Result<()> {
        let mut archive = Archive::new(reader);
        fs::create_dir_all(target_dir)?;

        for entry in archive.entries()? {
            let mut entry = entry.context("Failed to read tar entry")?;
            let path = entry.path()?.to_path_buf();
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            // 1. Opaque 디렉터리 마커 (.wh..wh..opq) -> 즉시 해당 디렉터리의 기존 내용 비우기
            if file_name == ".wh..wh..opq" {
                if let Some(parent) = path.parent() {
                    let full_parent = target_dir.join(parent);
                    if full_parent.is_dir() {
                        if let Ok(entries) = fs::read_dir(&full_parent) {
                            for entry in entries.flatten() {
                                let p = entry.path();
                                if p.is_dir() {
                                    let _ = fs::remove_dir_all(p);
                                } else {
                                    let _ = fs::remove_file(p);
                                }
                            }
                        }
                    }
                }
                continue;
            }

            // 2. Whiteout 삭제 마커 (.wh.<target>) -> 즉시 대상 파일/디렉터리 삭제
            if let Some(target_name) = file_name.strip_prefix(".wh.") {
                if let Some(parent) = path.parent() {
                    let full_target = target_dir.join(parent).join(target_name);
                    if full_target.is_dir() {
                        let _ = fs::remove_dir_all(&full_target);
                    } else if full_target.exists() {
                        let _ = fs::remove_file(&full_target);
                    }
                }
                continue;
            }

            // 3. 일반 파일/디렉터리/심볼릭링크 추출
            let dest = target_dir.join(&path);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }

            entry
                .unpack(&dest)
                .with_context(|| format!("Failed to unpack tar entry to {}", dest.display()))?;
        }

        info!(
            target_dir = %target_dir.display(),
            "Successfully unpacked OCI layer with whiteouts applied"
        );
        Ok(())
    }
}

/// newc-cpio 포맷 initramfs 아카이브 생성기 (순수 Rust)
pub struct InitramfsBuilder;

impl InitramfsBuilder {
    /// 디렉터리 전체를 newc-cpio 형식으로 직렬화하고 gzip 압축하여 파일로 생성
    pub fn build_from_dir(root_dir: &Path, output_gz_path: &Path) -> Result<()> {
        let file = File::create(output_gz_path).with_context(|| {
            format!(
                "Failed to create output initramfs at {}",
                output_gz_path.display()
            )
        })?;
        let mut encoder = GzEncoder::new(file, Compression::default());

        Self::write_dir_to_cpio(root_dir, root_dir, &mut encoder)?;

        // CPIO TRAILER!!! 마커 작성
        Self::write_cpio_entry(&mut encoder, "TRAILER!!!", 0, 0o0777, &[])?;

        encoder.finish()?;
        info!(output = %output_gz_path.display(), "Initramfs built successfully");
        Ok(())
    }

    fn write_dir_to_cpio<W: Write>(
        base_dir: &Path,
        current_dir: &Path,
        writer: &mut W,
    ) -> Result<()> {
        for entry in fs::read_dir(current_dir)? {
            let entry = entry?;
            let path = entry.path();
            let relative_path = path.strip_prefix(base_dir)?;
            let path_str = relative_path.to_str().unwrap_or("").replace('\\', "/");

            if path.is_dir() {
                Self::write_cpio_entry(writer, &path_str, 0, 0o040755, &[])?;
                Self::write_dir_to_cpio(base_dir, &path, writer)?;
            } else if path.is_file() {
                let mut data = Vec::new();
                File::open(&path)?.read_to_end(&mut data)?;
                Self::write_cpio_entry(writer, &path_str, data.len() as u32, 0o100755, &data)?;
            }
        }
        Ok(())
    }

    fn write_cpio_entry<W: Write>(
        writer: &mut W,
        name: &str,
        file_size: u32,
        mode: u32,
        data: &[u8],
    ) -> Result<()> {
        let name_bytes = name.as_bytes();
        let name_len = (name_bytes.len() + 1) as u32; // includes null terminator

        // newc magic "070701" (110 bytes header)
        // Format: 8-char hex strings
        let header = format!(
            "070701{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}",
            0,         // ino
            mode,      // mode
            0,         // uid
            0,         // gid
            1,         // nlink
            0,         // mtime
            file_size, // filesize
            0,         // maj
            0,         // min
            0,         // rmaj
            0,         // rmin
            name_len,  // namesize
            0          // chksum
        );

        writer.write_all(header.as_bytes())?;
        writer.write_all(name_bytes)?;
        writer.write_all(&[0])?; // null terminator

        // Pad header + name to 4-byte boundary
        let head_len = 110 + name_len;
        let pad_head = (4 - (head_len % 4)) % 4;
        for _ in 0..pad_head {
            writer.write_all(&[0])?;
        }

        // Write file data
        if !data.is_empty() {
            writer.write_all(data)?;
            let pad_data = (4 - (data.len() % 4)) % 4;
            for _ in 0..pad_data {
                writer.write_all(&[0])?;
            }
        }

        Ok(())
    }
}
