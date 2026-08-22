use flate2::write::GzEncoder;
use flate2::Compression;
use m_os_builder::{CoWCloner, InitramfsBuilder, KernelUnpacker, OciLayerUnpacker};
use std::fs;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_kernel_unpacker_efi_zboot_arm64() {
    // 1. Synthetic raw arm64 kernel Image (with "ARMd" at offset 0x38)
    let mut raw_arm64_kernel = vec![0u8; 128];
    raw_arm64_kernel[0x38..0x3C].copy_from_slice(b"ARM\x64");
    raw_arm64_kernel[0x40..0x48].copy_from_slice(b"TESTKRNL");

    // 2. Gzip the raw kernel
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw_arm64_kernel).unwrap();
    let gzip_payload = encoder.finish().unwrap();

    // 3. Build synthetic EFI zboot header
    // Offset 0..2: "MZ", 4..8: "zimg"
    // Offset 8..12: payload_offset (64)
    // Offset 12..16: payload_size
    // Offset 24..32: "gzip\0\0\0\0"
    let mut zboot = vec![0u8; 64];
    zboot[0..2].copy_from_slice(b"MZ");
    zboot[4..8].copy_from_slice(b"zimg");
    zboot[8..12].copy_from_slice(&(64u32).to_le_bytes());
    zboot[12..16].copy_from_slice(&(gzip_payload.len() as u32).to_le_bytes());
    zboot[24..28].copy_from_slice(b"gzip");
    zboot.extend_from_slice(&gzip_payload);

    // 4. Unpack
    let extracted = KernelUnpacker::unpack_bytes(&zboot).expect("Unpack zboot failed");
    assert_eq!(extracted.len(), 128);
    assert_eq!(&extracted[0x38..0x3C], b"ARM\x64");
    assert_eq!(&extracted[0x40..0x48], b"TESTKRNL");
}

#[test]
fn test_oci_layer_unpacker_with_whiteouts() {
    let dir = tempdir().unwrap();
    let target_dir = dir.path().join("rootfs");
    fs::create_dir_all(&target_dir).unwrap();

    // Layer 1: Create /app/server.js, /app/old.txt, /app/config/secret.env
    let app_dir = target_dir.join("app");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(app_dir.join("server.js"), b"console.log('hello');").unwrap();
    fs::write(app_dir.join("old.txt"), b"old data").unwrap();

    let config_dir = app_dir.join("config");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("secret.env"), b"KEY=VAL").unwrap();

    assert!(app_dir.join("old.txt").exists());
    assert!(config_dir.join("secret.env").exists());

    // Layer 2 tarball:
    // - adds /app/new.txt
    // - deletes /app/old.txt via /app/.wh.old.txt
    // - opacifies /app/config via /app/config/.wh..wh..opq (and adds new /app/config/public.env)
    let mut tar_builder = tar::Builder::new(Vec::new());

    // /app/new.txt
    let mut header1 = tar::Header::new_gnu();
    header1.set_path("app/new.txt").unwrap();
    header1.set_size(12);
    header1.set_mode(0o644);
    header1.set_cksum();
    tar_builder
        .append(&header1, b"new content\n".as_slice())
        .unwrap();

    // /app/.wh.old.txt
    let mut header2 = tar::Header::new_gnu();
    header2.set_path("app/.wh.old.txt").unwrap();
    header2.set_size(0);
    header2.set_mode(0o644);
    header2.set_cksum();
    tar_builder.append(&header2, std::io::empty()).unwrap();

    // /app/config/.wh..wh..opq
    let mut header3 = tar::Header::new_gnu();
    header3.set_path("app/config/.wh..wh..opq").unwrap();
    header3.set_size(0);
    header3.set_mode(0o644);
    header3.set_cksum();
    tar_builder.append(&header3, std::io::empty()).unwrap();

    // /app/config/public.env
    let mut header4 = tar::Header::new_gnu();
    header4.set_path("app/config/public.env").unwrap();
    header4.set_size(10);
    header4.set_mode(0o644);
    header4.set_cksum();
    tar_builder
        .append(&header4, b"PUBLIC=YES".as_slice())
        .unwrap();

    let tar_bytes = tar_builder.into_inner().unwrap();

    // Unpack Layer 2
    OciLayerUnpacker::unpack_layer(tar_bytes.as_slice(), &target_dir).expect("Unpack layer failed");

    // Assertions:
    // 1. /app/server.js still exists (unmodified)
    assert!(app_dir.join("server.js").exists());
    // 2. /app/new.txt exists
    assert!(app_dir.join("new.txt").exists());
    // 3. /app/old.txt is deleted
    assert!(!app_dir.join("old.txt").exists());
    // 4. /app/config/secret.env is wiped by opaque marker
    assert!(!config_dir.join("secret.env").exists());
    // 5. /app/config/public.env is created
    assert!(config_dir.join("public.env").exists());
}

#[test]
fn test_initramfs_builder_and_cow_cloner() {
    let dir = tempdir().unwrap();
    let rootfs_dir = dir.path().join("rootfs_sample");
    fs::create_dir_all(&rootfs_dir).unwrap();

    fs::write(rootfs_dir.join("init"), b"#!/bin/sh\necho init").unwrap();
    let etc_dir = rootfs_dir.join("etc");
    fs::create_dir_all(&etc_dir).unwrap();
    fs::write(etc_dir.join("hostname"), b"mos-guest\n").unwrap();

    // 1. Build initramfs.cpio.gz
    let initramfs_gz = dir.path().join("initramfs.cpio.gz");
    InitramfsBuilder::build_from_dir(&rootfs_dir, &initramfs_gz).expect("Build initramfs failed");
    assert!(initramfs_gz.exists());
    assert!(fs::metadata(&initramfs_gz).unwrap().len() > 0);

    // 2. CoW Clone file
    let clone_dst = dir.path().join("initramfs_clone.cpio.gz");
    CoWCloner::clone_file(&initramfs_gz, &clone_dst).expect("CoW clone failed");
    assert!(clone_dst.exists());
    assert_eq!(
        fs::metadata(&initramfs_gz).unwrap().len(),
        fs::metadata(&clone_dst).unwrap().len()
    );
}
