use m_os::HostProvisioner;
use std::fs;

#[test]
fn test_host_preflight_and_directory_provisioning() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let base_dir = temp_dir.path().join("mos_var_lib");

    let provisioner = HostProvisioner::new(&base_dir);

    // 1. Run preflight
    let report = provisioner.run_preflight();
    assert!(report.storage_writable);

    // 2. Provision directories
    provisioner
        .provision_directories()
        .expect("Provisioning directories failed");

    assert!(base_dir.join("kernels").exists());
    assert!(base_dir.join("rootfs").exists());
    assert!(base_dir.join("snapshots").exists());
    assert!(base_dir.join("instances").exists());
    assert!(base_dir.join("config").exists());
    assert!(base_dir.join("logs").exists());

    // 3. Generate and write Systemd unit
    let bin_path = base_dir.join("bin/mos");
    let config_path = base_dir.join("config/mos.toml");
    let unit_content = provisioner.generate_systemd_unit(&bin_path, &config_path);

    assert!(unit_content.contains("Description=MOS (MicroVM Operating Service) Node Daemon"));
    assert!(unit_content.contains(&bin_path.display().to_string()));
    assert!(unit_content.contains(&config_path.display().to_string()));

    let unit_dest = temp_dir.path().join("systemd/mos-node.service");
    provisioner
        .write_systemd_unit(&unit_dest, &unit_content)
        .expect("Writing unit failed");

    let written = fs::read_to_string(&unit_dest).unwrap();
    assert_eq!(written, unit_content);
}
