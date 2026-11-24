//! WGPU device selection for the shared GPU salt search.
use super::{extract::StoredSection, gpu_kernel};
use anyhow::Result;
use cubecl::prelude::*;
use cubecl_wgpu::WgpuRuntime;
#[cfg(not(target_arch = "wasm32"))]
use cubecl_wgpu::{AutoGraphicsApi, RuntimeOptions, WgpuDevice, init_setup};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

pub type GpuProbe = gpu_kernel::GpuProbe<WgpuRuntime>;

#[cfg(not(target_arch = "wasm32"))]
pub fn probe(section: &StoredSection) -> Result<GpuProbe> {
    static CLIENT: OnceLock<ComputeClient<WgpuRuntime>> = OnceLock::new();
    let client = CLIENT.get_or_init(|| {
        let device = WgpuDevice::DiscreteGpu(0);
        let setup = init_setup::<AutoGraphicsApi>(&device, RuntimeOptions::default());
        eprintln!(
            "WGPU: {} ({:?})",
            setup.adapter.get_info().name,
            setup.backend
        );
        WgpuRuntime::client(&device)
    });
    GpuProbe::new(section, client.clone())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn search(sections: &[StoredSection], screen: &StoredSection) -> Result<Option<u32>> {
    gpu_kernel::search("WGPU", sections, probe(screen)?)
}

#[cfg(target_arch = "wasm32")]
async fn probe_async(screen: &StoredSection) -> Result<(GpuProbe, u64, wgpu::Device)> {
    use anyhow::{Context, ensure};
    use cubecl_wgpu::{RuntimeOptions, WgpuSetup, init_device};
    ensure!(
        wgpu::util::is_browser_webgpu_supported().await,
        "WebGPU is unavailable in this worker"
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await
        .context("request WebGPU adapter")?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            required_limits: adapter.limits(),
            ..Default::default()
        })
        .await
        .context("request WebGPU device")?;
    let limits = device.limits();
    ensure!(
        limits.max_compute_workgroup_storage_size >= 32_768
            && limits.max_compute_invocations_per_workgroup >= 128
            && limits.max_compute_workgroup_size_x >= 128,
        "WebGPU device cannot run the 128-lane RC4 kernel"
    );
    let bytes = (screen.ciphertext.len() as u64)
        .checked_mul(4)
        .context("GPU probe size overflow")?;
    ensure!(
        bytes
            <= limits
                .max_storage_buffer_binding_size
                .min(limits.max_buffer_size),
        "probe exceeds WebGPU storage buffer limit"
    );
    // Dispatch size controls scheduling only, not the searched salt range.
    let capacity = 65_536u64
        .min(u64::from(limits.max_compute_workgroups_per_dimension) * 128)
        .min(limits.max_storage_buffer_binding_size / 4)
        .min(limits.max_buffer_size / 4);
    let setup = WgpuSetup {
        instance,
        adapter,
        device: device.clone(),
        queue,
        backend: wgpu::Backend::BrowserWebGpu,
    };
    let id = init_device(setup, RuntimeOptions::default());
    let probe = GpuProbe::with_batch(screen, WgpuRuntime::client(&id), capacity)?;
    Ok((probe, capacity, device))
}

#[cfg(target_arch = "wasm32")]
pub async fn search_async(
    sections: &[StoredSection],
    screen: &StoredSection,
) -> Result<Option<u32>> {
    use super::{extract::verify_all, search::UINT32_END};
    use anyhow::ensure;
    let (mut probe, capacity, device) = probe_async(screen).await?;
    let result = async {
        let mut start = 0;
        let mut batch = 128u64.min(capacity);
        eprintln!("WebGPU: starting salt search");
        while start < UINT32_END {
            let end = (start + batch).min(UINT32_END);
            let time = web_time::Instant::now();
            let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let hits = probe.scan_async(start, end).await;
            let error = validation.pop().await;
            ensure!(error.is_none(), "WebGPU validation failed: {error:?}");
            for salt in hits? {
                if verify_all(sections, Some(salt)).is_ok() {
                    return Ok(Some(salt));
                }
            }
            start = end;
            eprintln!("WebGPU search: {start} / {UINT32_END} candidates");
            let seconds = time.elapsed().as_secs_f64().max(0.001);
            batch = ((batch as f64 * 0.1 / seconds) as u64)
                .clamp(1, capacity)
                .min(batch.saturating_mul(2));
        }
        Ok(None)
    }
    .await;
    device.destroy();
    result
}
