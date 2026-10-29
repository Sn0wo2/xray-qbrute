use bytemuck::{Pod, Zeroable};
use std::mem::size_of;
#[cfg(not(test))]
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::candidate::CandidateConfig;

use super::{BackendOutcome, ProgressReporter, SearchConfig, SearchError, SearchHit};

const MAX_BATCH: u64 = 1 << 28;
const SEARCH_RESULT_SIZE: u64 = size_of::<GpuResult>() as u64;
const TUNING_DIFFICULTY_BITS: u32 = 64;
const TUNING_CANDIDATE: CandidateConfig = CandidateConfig {
    commit: 0xa5a5_5a5a,
    node_suffix: 0x5a5a_a5a5,
};
const TUNING_WORKGROUP_SIZES: [u32; 3] = [64, 128, 256];
const TUNING_CANDIDATES_PER_INVOCATION: [u32; 2] = [8, 16];
#[cfg(not(test))]
const LEGACY_PIPELINE_CACHE_SCHEMAS: [&str; 2] = ["sha512-unrolled-v1", "sha512-unrolled-v2"];

#[cfg(not(test))]
const TUNING_WARMUP_CANDIDATES: u64 = 1 << 20;
#[cfg(test)]
const TUNING_WARMUP_CANDIDATES: u64 = 1 << 12;

#[cfg(not(test))]
const TUNING_SAMPLE_CANDIDATES: u64 = 1 << 24;
#[cfg(test)]
const TUNING_SAMPLE_CANDIDATES: u64 = 1 << 14;

const SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sha512_search.wgsl"));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct KernelConfig {
    workgroup_size: u32,
    candidates_per_invocation: u32,
}

impl KernelConfig {
    fn candidates_per_workgroup(self) -> u64 {
        u64::from(self.workgroup_size) * u64::from(self.candidates_per_invocation)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::exhaustive_enums, clippy::module_name_repetitions)]
pub enum WgpuBackend {
    Vulkan,
    Metal,
}

impl WgpuBackend {
    const fn name(self) -> &'static str {
        match self {
            Self::Vulkan => "wgpu-vulkan",
            Self::Metal => "wgpu-metal",
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct GpuParams {
    base_lo: u32,
    base_hi: u32,
    candidate_count: u32,
    reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct GpuResult {
    min_offset: u32,
    _pad: [u32; 3],
}

struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    search_pipeline: wgpu::ComputePipeline,
    kernel_config: KernelConfig,
    tuning_elapsed: Duration,
    max_workgroups_per_dimension: u32,
    hash_pipeline: wgpu::ComputePipeline,
    search_bind_group_layout: wgpu::BindGroupLayout,
    hash_bind_group_layout: wgpu::BindGroupLayout,
    adapter_name: String,
    backend_name: &'static str,
}

struct SearchBuffers {
    params: wgpu::Buffer,
    result: wgpu::Buffer,
    readback: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

#[allow(clippy::module_name_repetitions)]
pub struct WgpuSearchSession {
    context: GpuContext,
    buffers: SearchBuffers,
}

impl std::fmt::Debug for WgpuSearchSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuSearchSession")
            .field("adapter_name", &self.context.adapter_name)
            .field("backend_name", &self.context.backend_name)
            .finish_non_exhaustive()
    }
}

pub fn search(
    config: &SearchConfig,
    selected_backend: WgpuBackend,
    progress: &ProgressReporter,
) -> Result<BackendOutcome, SearchError> {
    WgpuSearchSession::new(selected_backend, *config)?.search(config, progress)
}

impl WgpuSearchSession {
    pub fn new(selected_backend: WgpuBackend, config: SearchConfig) -> Result<Self, SearchError> {
        let context = pollster::block_on(create_context_async(selected_backend, config))?;
        context.verify_sha512(config.candidate)?;
        let buffers = create_search_buffers(&context.device, &context.search_bind_group_layout);

        Ok(Self { context, buffers })
    }

    pub fn warm_up(&self) -> Result<(), SearchError> {
        self.context
            .run_search_batch(&self.buffers, 0, 1 << 20)
            .map(|_| ())
    }

    pub fn search(
        &self,
        config: &SearchConfig,
        progress: &ProgressReporter,
    ) -> Result<BackendOutcome, SearchError> {
        let mut base = config.start_index;
        let mut evaluated = 0_u64;
        let search_start = Instant::now();
        let max_batch = MAX_BATCH.min(
            u64::from(self.context.max_workgroups_per_dimension)
                * self.context.kernel_config.candidates_per_workgroup(),
        );

        while base < config.max_index {
            let batch_count = (config.max_index - base).min(max_batch);
            let result = self
                .context
                .run_search_batch(&self.buffers, base, batch_count)?;

            if result.min_offset != u32::MAX {
                if u64::from(result.min_offset) >= batch_count {
                    return Err(SearchError::Runtime(format!(
                        "GPU shader returned offset {} outside a {batch_count}-candidate batch",
                        result.min_offset
                    )));
                }
                let index = base + u64::from(result.min_offset);
                let processed = index + 1;
                progress(processed - base);
                return Ok(BackendOutcome {
                    hit: Some(SearchHit { index }),
                    processed,
                    evaluated: evaluated + batch_count,
                    backend_name: self.context.backend_name.to_owned(),
                    device_name: Some(self.context.adapter_name.clone()),
                    fallback_reasons: Vec::new(),
                    search_elapsed: Some(search_start.elapsed()),
                    kernel_config: Some(format!(
                        "workgroup={}, candidates/thread={}",
                        self.context.kernel_config.workgroup_size,
                        self.context.kernel_config.candidates_per_invocation
                    )),
                    tuning_elapsed: Some(self.context.tuning_elapsed),
                });
            }

            evaluated += batch_count;
            progress(batch_count);
            base += batch_count;
        }

        Ok(BackendOutcome {
            hit: None,
            processed: config.max_index,
            evaluated,
            backend_name: self.context.backend_name.to_owned(),
            device_name: Some(self.context.adapter_name.clone()),
            fallback_reasons: Vec::new(),
            search_elapsed: Some(search_start.elapsed()),
            kernel_config: Some(format!(
                "workgroup={}, candidates/thread={}",
                self.context.kernel_config.workgroup_size,
                self.context.kernel_config.candidates_per_invocation
            )),
            tuning_elapsed: Some(self.context.tuning_elapsed),
        })
    }
}

#[allow(clippy::single_call_fn)]
async fn create_context_async(
    selected_backend: WgpuBackend,
    config: SearchConfig,
) -> Result<GpuContext, SearchError> {
    let backend_name = selected_backend.name();
    let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = match selected_backend {
        WgpuBackend::Vulkan => wgpu::Backends::VULKAN,
        WgpuBackend::Metal => wgpu::Backends::METAL,
    };
    let instance = wgpu::Instance::new(instance_descriptor);

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|error| {
            SearchError::Unavailable(format!("{backend_name} adapter request failed: {error}"))
        })?;
    let info = adapter.get_info();
    if !matches!(
        (selected_backend, info.backend),
        (WgpuBackend::Vulkan, wgpu::Backend::Vulkan) | (WgpuBackend::Metal, wgpu::Backend::Metal)
    ) {
        return Err(SearchError::Unavailable(format!(
            "{backend_name} requested but adapter {} uses {:?}",
            info.name, info.backend
        )));
    }
    let limits = adapter.limits();
    let adapter_features = adapter.features();
    let mut required_features = wgpu::Features::SHADER_INT64;
    if !adapter_features.contains(wgpu::Features::SHADER_INT64) {
        return Err(SearchError::Unavailable(format!(
            "{backend_name} adapter {} does not expose SHADER_INT64",
            info.name
        )));
    }
    if adapter_features.contains(wgpu::Features::PIPELINE_CACHE) {
        required_features |= wgpu::Features::PIPELINE_CACHE;
    }

    let device_label = format!("xray-qbrute {backend_name} device");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some(&device_label),
            required_features,
            required_limits: limits.clone(),
            ..Default::default()
        })
        .await
        .map_err(|error| {
            SearchError::Unavailable(format!("{backend_name} device request failed: {error}"))
        })?;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("xray-qbrute SHA-512 WGSL"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });

    let search_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("xray-qbrute search bind group layout"),
            entries: &[storage_binding(0, true), storage_binding(1, false)],
        });
    let hash_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("xray-qbrute hash bind group layout"),
            entries: &[storage_binding(0, true), storage_binding(2, false)],
        });

    let search_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("xray-qbrute search pipeline layout"),
        bind_group_layouts: &[Some(&search_bind_group_layout)],
        immediate_size: 0,
    });
    let hash_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("xray-qbrute hash pipeline layout"),
        bind_group_layouts: &[Some(&hash_bind_group_layout)],
        immediate_size: 0,
    });

    let tuning_start = Instant::now();
    #[cfg(not(test))]
    cleanup_legacy_pipeline_caches(&info);
    #[cfg(test)]
    let pipeline_cache = None;
    #[cfg(not(test))]
    let pipeline_cache = device
        .features()
        .contains(wgpu::Features::PIPELINE_CACHE)
        .then(|| unsafe {
            device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                label: Some("xray-qbrute pipeline cache"),
                data: None,
                fallback: true,
            })
        });
    let cache = pipeline_cache.as_ref();
    let kernel_config = tune_kernel(
        &device,
        &queue,
        &shader,
        &search_pipeline_layout,
        &search_bind_group_layout,
        cache,
        backend_name,
    )?;
    let search_pipeline = create_search_pipeline(
        &device,
        &shader,
        &search_pipeline_layout,
        config.candidate,
        config.leading_zero_bits,
        kernel_config,
        cache,
        "xray-qbrute search pipeline",
    );
    let tuning_elapsed = tuning_start.elapsed();

    let hash_constants =
        pipeline_constants(config.candidate, config.leading_zero_bits, kernel_config);
    let hash_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("xray-qbrute hash pipeline"),
        layout: Some(&hash_pipeline_layout),
        module: &shader,
        entry_point: Some("hash_main"),
        compilation_options: wgpu::PipelineCompilationOptions {
            constants: &hash_constants,
            zero_initialize_workgroup_memory: false,
        },
        cache,
    });

    Ok(GpuContext {
        device,
        queue,
        search_pipeline,
        kernel_config,
        tuning_elapsed,
        max_workgroups_per_dimension: limits.max_compute_workgroups_per_dimension,
        hash_pipeline,
        search_bind_group_layout,
        hash_bind_group_layout,
        adapter_name: info.name,
        backend_name,
    })
}

const fn storage_binding(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

#[allow(clippy::single_call_fn)]
#[cfg(not(test))]
fn cleanup_legacy_pipeline_caches(adapter_info: &wgpu::AdapterInfo) {
    let Some(cache_key) = wgpu::util::pipeline_cache_key(adapter_info) else {
        return;
    };
    let Some(cache_dir) = pipeline_cache_dir() else {
        return;
    };
    for schema in LEGACY_PIPELINE_CACHE_SCHEMAS {
        let path = cache_dir.join(format!("{cache_key}-{schema}.bin"));
        drop(std::fs::remove_file(path));
    }
    drop(std::fs::remove_dir(cache_dir));
}

#[cfg(all(not(test), target_os = "windows"))]
#[allow(clippy::single_call_fn)]
fn pipeline_cache_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("xray-qbrute").join("gpu-cache"))
}

#[cfg(all(not(test), target_os = "macos"))]
#[allow(clippy::single_call_fn)]
fn pipeline_cache_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|path| path.join("Library").join("Caches").join("xray-qbrute"))
}

#[cfg(all(not(test), not(any(target_os = "windows", target_os = "macos"))))]
#[allow(clippy::single_call_fn)]
fn pipeline_cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|path| path.join("xray-qbrute"))
}

fn pipeline_constants(
    candidate: CandidateConfig,
    leading_zero_bits: u32,
    kernel: KernelConfig,
) -> [(&'static str, f64); 5] {
    [
        ("COMMIT", f64::from(candidate.commit)),
        ("NODE_SUFFIX", f64::from(candidate.node_suffix)),
        ("LEADING_ZERO_BITS", f64::from(leading_zero_bits)),
        ("WORKGROUP_SIZE", f64::from(kernel.workgroup_size)),
        (
            "CANDIDATES_PER_INVOCATION",
            f64::from(kernel.candidates_per_invocation),
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn create_search_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    candidate: CandidateConfig,
    leading_zero_bits: u32,
    kernel: KernelConfig,
    cache: Option<&wgpu::PipelineCache>,
    label: &str,
) -> wgpu::ComputePipeline {
    let constants = pipeline_constants(candidate, leading_zero_bits, kernel);
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        module: shader,
        entry_point: Some("search_main"),
        compilation_options: wgpu::PipelineCompilationOptions {
            constants: &constants,
            zero_initialize_workgroup_memory: false,
        },
        cache,
    })
}

fn create_search_buffers(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> SearchBuffers {
    let params = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("xray-qbrute search params"),
        size: size_of::<GpuParams>() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let result = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("xray-qbrute search result"),
        size: SEARCH_RESULT_SIZE,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("xray-qbrute search readback"),
        size: SEARCH_RESULT_SIZE,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("xray-qbrute search bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: result.as_entire_binding(),
            },
        ],
    });

    SearchBuffers {
        params,
        result,
        readback,
        bind_group,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_search_pipeline(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    kernel: KernelConfig,
    buffers: &SearchBuffers,
    base: u64,
    candidate_count: u64,
    backend_name: &str,
) -> Result<GpuResult, SearchError> {
    let params = make_params(base, candidate_count);
    let initial_result = GpuResult {
        min_offset: u32::MAX,
        _pad: [0; 3],
    };
    queue.write_buffer(&buffers.params, 0, bytemuck::bytes_of(&params));
    queue.write_buffer(&buffers.result, 0, bytemuck::bytes_of(&initial_result));

    let workgroups = candidate_count.div_ceil(kernel.candidates_per_workgroup()) as u32;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("xray-qbrute search encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("xray-qbrute search pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &buffers.bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&buffers.result, 0, &buffers.readback, 0, SEARCH_RESULT_SIZE);
    queue.submit(Some(encoder.finish()));

    let bytes = readback(device, &buffers.readback, SEARCH_RESULT_SIZE, backend_name)?;
    Ok(bytemuck::pod_read_unaligned(&bytes))
}

struct KernelVariant {
    config: KernelConfig,
    pipeline: wgpu::ComputePipeline,
    elapsed: Duration,
}

#[allow(clippy::single_call_fn, clippy::too_many_arguments)]
fn tune_kernel(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    shader: &wgpu::ShaderModule,
    pipeline_layout: &wgpu::PipelineLayout,
    bind_group_layout: &wgpu::BindGroupLayout,
    cache: Option<&wgpu::PipelineCache>,
    backend_name: &str,
) -> Result<KernelConfig, SearchError> {
    let limits = device.limits();
    let buffers = create_search_buffers(device, bind_group_layout);
    let mut configs =
        Vec::with_capacity(TUNING_WORKGROUP_SIZES.len() * TUNING_CANDIDATES_PER_INVOCATION.len());

    for workgroup_size in TUNING_WORKGROUP_SIZES {
        for candidates_per_invocation in TUNING_CANDIDATES_PER_INVOCATION {
            let config = KernelConfig {
                workgroup_size,
                candidates_per_invocation,
            };
            if config.workgroup_size > limits.max_compute_invocations_per_workgroup
                || config.workgroup_size > limits.max_compute_workgroup_size_x
            {
                continue;
            }
            configs.push(config);
        }
    }

    if configs.is_empty() {
        return Err(SearchError::Unavailable(format!(
            "{backend_name} device does not support any kernel tuning candidate"
        )));
    }
    let mut variants = compile_kernel_variants(device, shader, pipeline_layout, cache, &configs)?;
    for variant in &variants {
        let result = run_search_pipeline(
            device,
            queue,
            &variant.pipeline,
            variant.config,
            &buffers,
            0,
            TUNING_WARMUP_CANDIDATES,
            backend_name,
        )?;
        validate_tuning_result(result, TUNING_WARMUP_CANDIDATES, variant.config)?;
    }
    benchmark_kernel_variants(
        device,
        queue,
        &buffers,
        backend_name,
        TUNING_SAMPLE_CANDIDATES,
        &mut variants,
    )?;

    variants
        .into_iter()
        .min_by_key(|variant| variant.elapsed)
        .map(|variant| variant.config)
        .ok_or_else(|| SearchError::Runtime("kernel tuner produced no result".to_owned()))
}

#[allow(clippy::single_call_fn)]
fn compile_kernel_variants(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    pipeline_layout: &wgpu::PipelineLayout,
    cache: Option<&wgpu::PipelineCache>,
    configs: &[KernelConfig],
) -> Result<Vec<KernelVariant>, SearchError> {
    std::thread::scope(|scope| {
        let handles = configs.iter().copied().map(|config| {
            scope.spawn(move || {
                let label = format!(
                    "xray-qbrute tuning pipeline {}x{}",
                    config.workgroup_size, config.candidates_per_invocation
                );
                let pipeline = create_search_pipeline(
                    device,
                    shader,
                    pipeline_layout,
                    TUNING_CANDIDATE,
                    TUNING_DIFFICULTY_BITS,
                    config,
                    cache,
                    &label,
                );
                KernelVariant {
                    config,
                    pipeline,
                    elapsed: Duration::ZERO,
                }
            })
        });

        handles
            .into_iter()
            .map(|handle| {
                handle.join().map_err(|_| {
                    SearchError::Runtime("GPU pipeline compilation thread panicked".to_owned())
                })
            })
            .collect()
    })
}

#[allow(clippy::single_call_fn, clippy::print_stderr)]
fn benchmark_kernel_variants(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffers: &SearchBuffers,
    backend_name: &str,
    sample_candidates: u64,
    variants: &mut [KernelVariant],
) -> Result<(), SearchError> {
    for variant in variants.iter_mut() {
        variant.elapsed = Duration::ZERO;
    }
    for reverse in [false, true] {
        for offset in 0..variants.len() {
            let index = if reverse {
                variants.len() - 1 - offset
            } else {
                offset
            };
            let variant = variants.get_mut(index).ok_or_else(|| {
                SearchError::Runtime("kernel tuning selected an invalid variant index".to_owned())
            })?;
            let start = Instant::now();
            let result = run_search_pipeline(
                device,
                queue,
                &variant.pipeline,
                variant.config,
                buffers,
                0,
                sample_candidates,
                backend_name,
            )?;
            variant.elapsed += start.elapsed();
            validate_tuning_result(result, sample_candidates, variant.config)?;
        }
    }
    if std::env::var_os("XRAY_QBRUTE_TUNING_LOG").is_some() {
        for variant in variants.iter() {
            let rate = (sample_candidates * 2) as f64
                / variant.elapsed.as_secs_f64().max(f64::EPSILON)
                / 1e6;
            eprintln!(
                "kernel tuning {}x{}: {rate:.1} M/s",
                variant.config.workgroup_size, variant.config.candidates_per_invocation
            );
        }
    }
    Ok(())
}

fn validate_tuning_result(
    result: GpuResult,
    expected_candidates: u64,
    kernel: KernelConfig,
) -> Result<(), SearchError> {
    if result.min_offset != u32::MAX {
        return Err(SearchError::Runtime(format!(
            "GPU kernel {}x{} unexpectedly matched offset {} while tuning {expected_candidates} candidates",
            kernel.workgroup_size, kernel.candidates_per_invocation, result.min_offset,
        )));
    }
    Ok(())
}

impl GpuContext {
    fn run_search_batch(
        &self,
        buffers: &SearchBuffers,
        base: u64,
        candidate_count: u64,
    ) -> Result<GpuResult, SearchError> {
        run_search_pipeline(
            &self.device,
            &self.queue,
            &self.search_pipeline,
            self.kernel_config,
            buffers,
            base,
            candidate_count,
            self.backend_name,
        )
    }

    fn hash_first_words(&self, base: u64, candidate_count: u64) -> Result<Vec<u64>, SearchError> {
        let params = make_params(base, candidate_count);
        let output_size = candidate_count * size_of::<u64>() as u64;
        let params_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xray-qbrute hash params"),
            size: size_of::<GpuParams>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xray-qbrute hash output"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xray-qbrute hash readback"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("xray-qbrute hash bind group"),
            layout: &self.hash_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        self.queue
            .write_buffer(&params_buffer, 0, bytemuck::bytes_of(&params));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("xray-qbrute hash encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("xray-qbrute hash pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.hash_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback_buffer, 0, output_size);
        self.queue.submit(Some(encoder.finish()));

        let bytes = readback(
            &self.device,
            &readback_buffer,
            output_size,
            self.backend_name,
        )?;
        Ok(bytemuck::cast_slice(&bytes).to_vec())
    }

    /// Runtime self-test: hashes known candidate indices on the GPU and
    /// compares the first digest word against the CPU reference implementation.
    /// Catches shader bugs, driver miscompiles, or GPU bit flips before the
    /// real search starts.
    fn verify_sha512(&self, candidate: CandidateConfig) -> Result<(), SearchError> {
        const SELF_TEST_COUNT: u64 = 256;
        const SELF_TEST_BASES: [u64; 4] = [0, 1 << 28, 1 << 30, (1 << 58) - 256];

        for base in SELF_TEST_BASES {
            let words = self.hash_first_words(base, SELF_TEST_COUNT)?;
            for (offset, &actual) in words.iter().enumerate() {
                let index = base + offset as u64;
                let hash = crate::candidate::digest(index, candidate);
                let expected = crate::candidate::digest_word(&hash);
                if actual != expected {
                    return Err(SearchError::Runtime(format!(
                        "GPU SHA-512 self-test failed at index {index}: \
                         shader produced 0x{actual:016x}, expected 0x{expected:016x}"
                    )));
                }
            }
        }
        Ok(())
    }
}

const fn make_params(base: u64, candidate_count: u64) -> GpuParams {
    GpuParams {
        base_lo: base as u32,
        base_hi: (base >> 32) as u32,
        candidate_count: candidate_count as u32,
        reserved: 0,
    }
}

fn readback(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
    size: u64,
    backend_name: &str,
) -> Result<Vec<u8>, SearchError> {
    let slice = buffer.slice(0..size);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| {
            SearchError::Unavailable(format!("{backend_name} device poll failed: {error}"))
        })?;
    let map_result = receiver.recv().map_err(|error| {
        SearchError::Unavailable(format!("{backend_name} map callback failed: {error}"))
    })?;
    map_result.map_err(|error| {
        SearchError::Unavailable(format!("{backend_name} readback map failed: {error}"))
    })?;

    let view = slice.get_mapped_range().map_err(|error| {
        SearchError::Unavailable(format!("{backend_name} mapped range failed: {error}"))
    })?;
    let bytes = view.to_vec();
    drop(view);
    buffer.unmap();
    Ok(bytes)
}

#[cfg(test)]
mod test;
