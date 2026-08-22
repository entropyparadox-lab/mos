#!/usr/bin/env bash
set -euo pipefail

MOS_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${MOS_ROOT}/bin/firecracker"
KERNEL="${MOS_ROOT}/runtime/kernels/vmlinux.bin"
BASE_ROOTFS="${MOS_ROOT}/runtime/base-rootfs/bionic.rootfs.ext4"
RUN_DIR="${MOS_ROOT}/runtime/instances/test-poc-1"

mkdir -p "${RUN_DIR}"
SOCK_PATH="${RUN_DIR}/firecracker.sock"
TEST_ROOTFS="${RUN_DIR}/rootfs.ext4"
LOG_FILE="${RUN_DIR}/vm.log"

# Clean up existing instance socket and rootfs copy
rm -f "${SOCK_PATH}" "${LOG_FILE}"
cp "${BASE_ROOTFS}" "${TEST_ROOTFS}"

echo "=== [MOS PoC] Starting Firecracker MicroVM ==="

# Start Firecracker process in background
"${BIN}" --api-sock "${SOCK_PATH}" > "${LOG_FILE}" 2>&1 &
FC_PID=$!

cleanup() {
    echo "Cleaning up Firecracker (PID: ${FC_PID})..."
    kill -9 "${FC_PID}" 2>/dev/null || true
    rm -f "${SOCK_PATH}"
}
trap cleanup EXIT

# Wait for socket readiness
while [ ! -e "${SOCK_PATH}" ]; do
    sleep 0.01
done

echo "[1/4] Socket ready: ${SOCK_PATH}"

# Set Boot Source
START_TIME=$(date +%s%N)
curl --unix-socket "${SOCK_PATH}" -s -X PUT 'http://localhost/boot-source' \
    -H 'Content-Type: application/json' \
    -d "{
        \"kernel_image_path\": \"${KERNEL}\",
        \"boot_args\": \"console=ttyS0 reboot=k panic=1 pci=off init=/bin/sh\"
    }"

echo "[2/4] Boot source configured."

# Set Rootfs Drive
curl --unix-socket "${SOCK_PATH}" -s -X PUT 'http://localhost/drives/rootfs' \
    -H 'Content-Type: application/json' \
    -d "{
        \"drive_id\": \"rootfs\",
        \"path_on_host\": \"${TEST_ROOTFS}\",
        \"is_root_device\": true,
        \"is_read_only\": false
    }"

echo "[3/4] Rootfs drive configured."

# Set Machine Config (1 vCPU, 128MB RAM)
curl --unix-socket "${SOCK_PATH}" -s -X PUT 'http://localhost/machine-config' \
    -H 'Content-Type: application/json' \
    -d "{
        \"vcpu_count\": 1,
        \"mem_size_mib\": 128,
        \"smt\": false
    }"

echo "[4/4] Machine config set (1 vCPU, 128 MiB RAM)."

# Instance Start
curl --unix-socket "${SOCK_PATH}" -s -X PUT 'http://localhost/actions' \
    -H 'Content-Type: application/json' \
    -d "{
        \"action_type\": \"InstanceStart\"
    }"
END_TIME=$(date +%s%N)

ELAPSED_MS=$(( (END_TIME - START_TIME) / 1000000 ))
echo "=== [MOS PoC] MicroVM started in ${ELAPSED_MS} ms! ==="

# Check process memory and state
ps -o pid,vsz,rss,comm -p "${FC_PID}"

echo "=== Firecracker MicroVM is RUNNING! ==="
sleep 1
cat "${LOG_FILE}" | head -n 30 || true
