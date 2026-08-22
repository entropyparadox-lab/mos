use m_os_orchestrator::{GpuDevice, GpuPoolError, GpuPoolManager};
use mos_core::InstanceId;

#[test]
fn test_gpu_pool_allocation_and_scale_to_zero() {
    let pool = GpuPoolManager::new();

    // 1. Register NVIDIA RTX 4090 (24,576 MB VRAM)
    let gpu0 = GpuDevice::new(0, "NVIDIA GeForce RTX 4090", "0000:01:00.0", 24576);
    pool.register_device(gpu0);

    let inst_llm_1 = InstanceId::new();
    let inst_llm_2 = InstanceId::new();

    // 2. Request 8GB (8192 MB) VRAM for Llama-3-8B MicroVM
    let binding1 = pool
        .bind_gpu_to_instance(&inst_llm_1, 8192)
        .expect("Binding 1 should succeed");

    assert_eq!(binding1.gpu_index, 0);
    assert_eq!(binding1.allocated_vram_mib, 8192);
    assert_eq!(pool.total_vram_in_use_mib(), 8192);

    // 3. Request 12GB (12288 MB) VRAM for Mistral-7B MicroVM
    let binding2 = pool
        .bind_gpu_to_instance(&inst_llm_2, 12288)
        .expect("Binding 2 should succeed");

    assert_eq!(binding2.gpu_index, 0);
    assert_eq!(pool.total_vram_in_use_mib(), 20480); // 8192 + 12288 = 20480MB

    // 4. Request 8GB again -> Insufficient VRAM (24576 - 20480 = 4096MB available)
    let inst_llm_3 = InstanceId::new();
    let err = pool.bind_gpu_to_instance(&inst_llm_3, 8192).unwrap_err();
    assert_eq!(
        err,
        GpuPoolError::InsufficientVram {
            requested_mib: 8192
        }
    );

    // 5. Scale-to-Zero trigger for inst_llm_1: MicroVM idles, releases 8GB GPU VRAM
    let freed = pool
        .scale_to_zero_detach(&inst_llm_1)
        .expect("Scale-to-Zero detach failed");

    assert_eq!(freed, 8192);
    assert_eq!(pool.total_vram_in_use_mib(), 12288); // inst_llm_2 remains (12288MB)

    // Now inst_llm_3 (8192MB) can be allocated! (24576 - 12288 = 12288MB available)
    let binding3 = pool
        .bind_gpu_to_instance(&inst_llm_3, 8192)
        .expect("Binding 3 should succeed after Scale-to-Zero detach");

    assert_eq!(binding3.gpu_index, 0);
    assert_eq!(binding3.allocated_vram_mib, 8192);
    assert_eq!(pool.total_vram_in_use_mib(), 20480);

    // 6. Scale-to-Zero for both remaining instances -> 0MB total VRAM
    pool.scale_to_zero_detach(&inst_llm_2).unwrap();
    pool.scale_to_zero_detach(&inst_llm_3).unwrap();
    assert_eq!(pool.total_vram_in_use_mib(), 0);
}
