#!/usr/bin/env bash
set -euo pipefail

MOS_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${MOS_ROOT}/bin"
RUNTIME_DIR="${MOS_ROOT}/runtime"
KERNELS_DIR="${RUNTIME_DIR}/kernels"
ROOTFS_DIR="${RUNTIME_DIR}/base-rootfs"

mkdir -p "${BIN_DIR}" "${KERNELS_DIR}" "${ROOTFS_DIR}"

FC_VERSION="v1.10.1"
ARCH="x86_64"

echo "[1/3] Downloading Firecracker ${FC_VERSION} (${ARCH})..."
if [ ! -f "${BIN_DIR}/firecracker" ]; then
    FC_TAR="firecracker-${FC_VERSION}-${ARCH}.tgz"
    FC_URL="https://github.com/firecracker-microvm/firecracker/releases/download/${FC_VERSION}/${FC_TAR}"
    
    TMP_DIR=$(mktemp -d)
    curl -sSL "${FC_URL}" -o "${TMP_DIR}/${FC_TAR}"
    tar -xzf "${TMP_DIR}/${FC_TAR}" -C "${TMP_DIR}"
    
    cp "${TMP_DIR}/release-${FC_VERSION}-${ARCH}/firecracker-${FC_VERSION}-${ARCH}" "${BIN_DIR}/firecracker"
    cp "${TMP_DIR}/release-${FC_VERSION}-${ARCH}/jailer-${FC_VERSION}-${ARCH}" "${BIN_DIR}/jailer"
    chmod +x "${BIN_DIR}/firecracker" "${BIN_DIR}/jailer"
    rm -rf "${TMP_DIR}"
    echo "Firecracker installed to ${BIN_DIR}/firecracker"
else
    echo "Firecracker already exists at ${BIN_DIR}/firecracker"
fi

echo "[2/3] Downloading Firecracker guest kernel (vmlinux)..."
KERNEL_PATH="${KERNELS_DIR}/vmlinux-5.10.bin"
if [ ! -f "${KERNEL_PATH}" ]; then
    KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.10/${ARCH}/vmlinux-5.10.217"
    curl -sSL "${KERNEL_URL}" -o "${KERNEL_PATH}"
    echo "Kernel downloaded to ${KERNEL_PATH}"
else
    echo "Kernel already exists at ${KERNEL_PATH}"
fi

echo "[3/3] Downloading base Alpine rootfs..."
ROOTFS_PATH="${ROOTFS_DIR}/alpine-rootfs.ext4"
if [ ! -f "${ROOTFS_PATH}" ]; then
    ROOTFS_URL="https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.10/${ARCH}/alpine.rootfs.ext4"
    curl -sSL "${ROOTFS_URL}" -o "${ROOTFS_PATH}"
    echo "Rootfs downloaded to ${ROOTFS_PATH}"
else
    echo "Rootfs already exists at ${ROOTFS_PATH}"
fi

echo "=== Firecracker environment setup complete! ==="
"${BIN_DIR}/firecracker" --version
