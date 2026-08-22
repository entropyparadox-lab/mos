use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use std::io::Read;
use std::path::Path;
use tracing::info;

/// arm64 EFI zboot 및 raw 커널 파서/언패커
pub struct KernelUnpacker;

impl KernelUnpacker {
    /// 주어진 커널 바이너리 바이트에서 raw uncompressed 커널 Image를 추출
    pub fn unpack_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
        // 1. 이미 raw arm64 Image인지 확인 (offset 0x38 == b"ARM\x64" / "ARMd")
        if bytes.len() > 0x3C && &bytes[0x38..0x3C] == b"ARM\x64" {
            info!("Kernel is already a raw uncompressed arm64 Image");
            return Ok(bytes.to_vec());
        }

        // 2. x86_64 raw ELF 커널인지 확인 (offset 0..4 == b"\x7fELF")
        if bytes.len() > 4 && &bytes[0..4] == b"\x7fELF" {
            info!("Kernel is an uncompressed x86_64 ELF vmlinux");
            return Ok(bytes.to_vec());
        }

        // 3. EFI zboot 형식인지 확인 ("MZ" at 0..2, "zimg" at 4..8)
        if bytes.len() > 64 && &bytes[0..2] == b"MZ" && &bytes[4..8] == b"zimg" {
            info!("Detected EFI zboot kernel wrapper. Parsing header...");
            let payload_offset = u32::from_le_bytes(
                bytes[8..12]
                    .try_into()
                    .context("Failed to read payload_offset")?,
            ) as usize;
            let payload_size = u32::from_le_bytes(
                bytes[12..16]
                    .try_into()
                    .context("Failed to read payload_size")?,
            ) as usize;

            let comp_end = bytes[24..32].iter().position(|&b| b == 0).unwrap_or(8);
            let compression = std::str::from_utf8(&bytes[24..24 + comp_end]).unwrap_or("");

            let end = payload_offset
                .checked_add(payload_size)
                .filter(|&e| e <= bytes.len())
                .context("zboot payload range exceeds file length")?;

            let raw_compressed = &bytes[payload_offset..end];
            info!(
                compression = %compression,
                payload_offset = payload_offset,
                payload_size = payload_size,
                "Decompressing zboot payload..."
            );

            let decompressed = match compression {
                "gzip" | "" => {
                    let mut decoder = GzDecoder::new(raw_compressed);
                    let mut buf = Vec::new();
                    decoder
                        .read_to_end(&mut buf)
                        .context("Failed to gunzip zboot payload")?;
                    buf
                }
                "zstd" => zstd::decode_all(raw_compressed)
                    .context("Failed to zstd decompress zboot payload")?,
                other => bail!("Unsupported zboot compression format: {}", other),
            };

            // 압축 해제된 결과물이 raw ARMd Image인지 검증
            if decompressed.len() > 0x3C && &decompressed[0x38..0x3C] == b"ARM\x64" {
                info!("Successfully extracted raw arm64 kernel Image from EFI zboot");
            } else {
                info!("Decompressed payload is ready (custom/generic format)");
            }

            return Ok(decompressed);
        }

        // 4. 일반 gzip 스트림인지 확인 (0x1f, 0x8b)
        if bytes.len() > 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
            info!("Detected raw gzip compressed kernel. Decompressing...");
            let mut decoder = GzDecoder::new(bytes);
            let mut buf = Vec::new();
            decoder
                .read_to_end(&mut buf)
                .context("Failed to gunzip kernel binary")?;
            return Ok(buf);
        }

        // 알 수 없는 포맷이지만 있는 그대로 반환
        info!("Kernel format not explicitly recognized, returning raw bytes");
        Ok(bytes.to_vec())
    }

    /// 파일 경로로부터 커널을 읽고 압축 해제된 raw 바이너리를 대상 파일에 저장
    pub async fn unpack_to_file(src_path: &Path, dst_path: &Path) -> Result<()> {
        let raw_bytes = tokio::fs::read(src_path)
            .await
            .with_context(|| format!("Failed to read kernel from {}", src_path.display()))?;

        let uncompressed = Self::unpack_bytes(&raw_bytes)?;

        if let Some(parent) = dst_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(dst_path, &uncompressed)
            .await
            .with_context(|| {
                format!(
                    "Failed to write uncompressed kernel to {}",
                    dst_path.display()
                )
            })?;

        Ok(())
    }
}
