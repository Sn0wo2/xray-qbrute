use std::fmt;
#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use cudarc::driver::{CudaContext, DeviceRepr, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::compile_ptx;

use crate::candidate::{self, CandidateConfig};

use super::{BackendOutcome, ProgressReporter, SearchConfig, SearchError, SearchHit};

const CANDIDATES_PER_INVOCATION: u32 = 32;
const BLOCK_SIZE: u32 = 256;
const MAX_BATCH: u64 = 1 << 28;
const SELF_TEST_COUNT: u64 = 256;
const SELF_TEST_BASES: [u64; 4] = [0, 1 << 28, 1 << 30, (1 << 58) - 256];

const KERNEL_SOURCE: &str = include_str!(concat!(env!("OUT_DIR"), "/sha512_search.cu"));

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GpuResult {
    min_offset: u32,
    _pad: [u32; 3],
}

unsafe impl DeviceRepr for GpuResult {}

#[repr(C)]
#[derive(Clone, Copy)]
struct GpuParams {
    base_lo: u32,
    base_hi: u32,
    candidate_count: u32,
    reserved: u32,
}

unsafe impl DeviceRepr for GpuParams {}

impl GpuParams {
    const fn new(base: u64, candidate_count: u64) -> Self {
        Self {
            base_lo: base as u32,
            base_hi: (base >> 32) as u32,
            candidate_count: candidate_count as u32,
            reserved: 0,
        }
    }
}

#[allow(clippy::module_name_repetitions)]
pub struct CudaSearchSession {
    context: Arc<CudaContext>,
    search_kernel: cudarc::driver::CudaFunction,
    hash_kernel: cudarc::driver::CudaFunction,
    commit: u64,
    node_suffix: u64,
    leading_zero_bits: u32,
}

impl fmt::Debug for CudaSearchSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CudaSearchSession")
            .field("commit", &self.commit)
            .field("node_suffix", &self.node_suffix)
            .field("leading_zero_bits", &self.leading_zero_bits)
            .finish_non_exhaustive()
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
#[allow(clippy::single_call_fn)]
fn locate_cuda_bin_dir() -> Option<PathBuf> {
    let mut roots = Vec::new();

    for key in ["CUDA_PATH", "CUDA_HOME"] {
        if let Ok(path) = std::env::var(key) {
            roots.push(PathBuf::from(path));
        }
    }
    #[cfg(target_os = "windows")]
    {
        for major in 9..=13_u32 {
            for minor in 0..=9_u32 {
                if let Ok(path) = std::env::var(format!("CUDA_PATH_V{major}_{minor}")) {
                    roots.push(PathBuf::from(path));
                }
            }
        }
        for drive in ['C', 'D', 'E', 'F'] {
            let sep = std::path::MAIN_SEPARATOR;
            roots.push(PathBuf::from(format!("{drive}:{sep}CUDA")));
            roots.push(PathBuf::from(format!(
                "{drive}:{sep}Program Files{sep}NVIDIA GPU Computing Toolkit{sep}CUDA"
            )));
        }
    }
    #[cfg(target_os = "linux")]
    {
        roots.push(PathBuf::from("/usr/local/cuda"));
        roots.push(PathBuf::from("/opt/cuda"));
        for parent in ["/usr/local", "/opt"] {
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with("cuda-") || name.starts_with("cuda_") {
                        roots.push(entry.path());
                    }
                }
            }
        }
    }

    let mut probes = Vec::new();
    for root in roots {
        probes.push(root.clone());
        #[cfg(target_os = "windows")]
        {
            probes.push(root.join("bin"));
            probes.push(root.join("bin").join("x64"));
        }
        #[cfg(target_os = "linux")]
        {
            probes.push(root.join("lib64"));
            probes.push(root.join("lib"));
        }
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                #[cfg(target_os = "windows")]
                let is_version_dir = name.len() > 1
                    && name.starts_with('v')
                    && name[1..].chars().all(|c| c.is_ascii_digit() || c == '.');
                #[cfg(target_os = "linux")]
                let is_version_dir = name.starts_with("cuda-") || name.starts_with("cuda_");
                if is_version_dir {
                    probes.push(entry.path());
                    #[cfg(target_os = "windows")]
                    {
                        probes.push(entry.path().join("bin"));
                        probes.push(entry.path().join("bin").join("x64"));
                    }
                    #[cfg(target_os = "linux")]
                    {
                        probes.push(entry.path().join("lib64"));
                        probes.push(entry.path().join("lib"));
                    }
                }
            }
        }
    }

    probes.into_iter().find(|dir| {
        std::fs::read_dir(dir).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                #[cfg(target_os = "windows")]
                {
                    name.starts_with("nvrtc64_") && name.ends_with(".dll")
                }
                #[cfg(target_os = "linux")]
                {
                    name.starts_with("libnvrtc.so")
                }
            })
        })
    })
}

#[cfg(target_os = "windows")]
fn ensure_cuda_bin_on_path() {
    let Some(dir) = locate_cuda_bin_dir() else {
        return;
    };
    let dir_str = dir.to_string_lossy().into_owned();
    let current = std::env::var("PATH").unwrap_or_default();
    if !current.split(';').any(|p| p.eq_ignore_ascii_case(&dir_str)) {
        unsafe {
            std::env::set_var("PATH", format!("{dir_str};{current}"));
        }
    }
}

#[cfg(target_os = "linux")]
fn ensure_cuda_bin_on_path() {
    let Some(dir) = locate_cuda_bin_dir() else {
        return;
    };
    let dir_str = dir.to_string_lossy().into_owned();
    let current = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
    if !current.split(':').any(|p| p == dir_str) {
        unsafe {
            std::env::set_var("LD_LIBRARY_PATH", format!("{dir_str}:{current}"));
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn ensure_cuda_bin_on_path() {}

pub fn is_available() -> bool {
    ensure_cuda_bin_on_path();
    catch_silent(|| CudaContext::new(0).is_ok()).unwrap_or(false)
}

fn catch_silent<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> std::thread::Result<T> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(f);
    std::panic::set_hook(previous);
    result
}
impl CudaSearchSession {
    pub fn new(config: SearchConfig) -> Result<Self, SearchError> {
        ensure_cuda_bin_on_path();

        let ptx = catch_silent(|| compile_ptx(KERNEL_SOURCE))
            .map_err(|_| {
                SearchError::Unavailable(
                    "NVRTC shared library not found (CUDA toolkit not installed)".to_owned(),
                )
            })?
            .map_err(|error| {
                SearchError::Runtime(format!("NVRTC kernel compilation failed: {error:?}"))
            })?;

        let context = CudaContext::new(0).map_err(|error| {
            SearchError::Unavailable(format!("CUDA device initialization failed: {error:?}"))
        })?;

        let module = context
            .load_module(ptx)
            .map_err(|error| SearchError::Runtime(format!("CUDA module load failed: {error:?}")))?;

        let search_kernel = module.load_function("search_main").map_err(|error| {
            SearchError::Runtime(format!("CUDA search kernel load failed: {error:?}"))
        })?;

        let hash_kernel = module.load_function("hash_main").map_err(|error| {
            SearchError::Runtime(format!("CUDA hash kernel load failed: {error:?}"))
        })?;

        let session = Self {
            context,
            search_kernel,
            hash_kernel,
            commit: u64::from(config.candidate.commit),
            node_suffix: u64::from(config.candidate.node_suffix),
            leading_zero_bits: config.leading_zero_bits,
        };

        session.verify_sha512(config.candidate)?;

        Ok(session)
    }

    pub fn warm_up(&self) -> Result<(), SearchError> {
        self.run_search_batch(0, 1 << 16, self.leading_zero_bits)?;
        Ok(())
    }

    pub fn search(
        &self,
        config: &SearchConfig,
        progress: &ProgressReporter,
    ) -> Result<BackendOutcome, SearchError> {
        let mut base = config.start_index;
        let mut evaluated = 0_u64;
        let search_start = Instant::now();

        while base < config.max_index {
            let batch_count = (config.max_index - base).min(MAX_BATCH);
            let result = self.run_search_batch(base, batch_count, config.leading_zero_bits)?;

            if result.min_offset != u32::MAX {
                if u64::from(result.min_offset) >= batch_count {
                    return Err(SearchError::Runtime(format!(
                        "CUDA kernel returned offset {} outside a {batch_count}-candidate batch",
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
                    backend_name: "cuda".to_owned(),
                    device_name: Some("NVIDIA CUDA".to_owned()),
                    fallback_reasons: Vec::new(),
                    search_elapsed: Some(search_start.elapsed()),
                    kernel_config: Some(format!(
                        "block={BLOCK_SIZE}, candidates/thread={CANDIDATES_PER_INVOCATION}"
                    )),
                    tuning_elapsed: None,
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
            backend_name: "cuda".to_owned(),
            device_name: Some("NVIDIA CUDA".to_owned()),
            fallback_reasons: Vec::new(),
            search_elapsed: Some(search_start.elapsed()),
            kernel_config: Some(format!(
                "block={BLOCK_SIZE}, candidates/thread={CANDIDATES_PER_INVOCATION}"
            )),
            tuning_elapsed: None,
        })
    }

    fn run_search_batch(
        &self,
        base: u64,
        candidate_count: u64,
        leading_zero_bits: u32,
    ) -> Result<GpuResult, SearchError> {
        let stream = self.context.default_stream();

        let params = GpuParams::new(base, candidate_count);
        let params_buf = stream.clone_htod(&[params]).map_err(|error| {
            SearchError::Runtime(format!("CUDA params upload failed: {error:?}"))
        })?;

        let result_buf = stream
            .clone_htod(&[GpuResult {
                min_offset: u32::MAX,
                _pad: [0; 3],
            }])
            .map_err(|error| {
                SearchError::Runtime(format!("CUDA result buffer init failed: {error:?}"))
            })?;

        let threads_needed = (candidate_count as u32).div_ceil(CANDIDATES_PER_INVOCATION);
        let blocks = threads_needed.div_ceil(BLOCK_SIZE);
        let cfg = LaunchConfig {
            grid_dim: (blocks.max(1), 1, 1),
            block_dim: (BLOCK_SIZE, 1, 1),
            shared_mem_bytes: 0,
        };

        unsafe {
            stream
                .launch_builder(&self.search_kernel)
                .arg(&result_buf)
                .arg(&params_buf)
                .arg(&self.commit)
                .arg(&self.node_suffix)
                .arg(&leading_zero_bits)
                .arg(&CANDIDATES_PER_INVOCATION)
                .launch(cfg)
                .map_err(|error| {
                    SearchError::Runtime(format!("CUDA kernel launch failed: {error:?}"))
                })?;
        }

        let results: Vec<GpuResult> = stream.clone_dtoh(&result_buf).map_err(|error| {
            SearchError::Runtime(format!("CUDA result readback failed: {error:?}"))
        })?;

        results.into_iter().next().ok_or_else(|| {
            SearchError::Runtime("CUDA result readback returned no values".to_owned())
        })
    }

    fn verify_sha512(&self, candidate: CandidateConfig) -> Result<(), SearchError> {
        for base in SELF_TEST_BASES {
            let words = self.hash_first_words(base, SELF_TEST_COUNT)?;
            for (offset, &actual) in words.iter().enumerate() {
                let index = base + offset as u64;
                let hash = candidate::digest(index, candidate);
                let expected = candidate::digest_word(&hash);
                if actual != expected {
                    return Err(SearchError::Runtime(format!(
                        "CUDA SHA-512 self-test failed at index {index}: \
                         kernel produced 0x{actual:016x}, expected 0x{expected:016x}"
                    )));
                }
            }
        }
        Ok(())
    }

    fn hash_first_words(&self, base: u64, count: u64) -> Result<Vec<u64>, SearchError> {
        let stream = self.context.default_stream();

        let params = GpuParams::new(base, count);
        let params_buf = stream.clone_htod(&[params]).map_err(|error| {
            SearchError::Runtime(format!("CUDA params upload failed: {error:?}"))
        })?;

        let output_buf = stream.alloc_zeros::<u64>(count as usize).map_err(|error| {
            SearchError::Runtime(format!("CUDA output alloc failed: {error:?}"))
        })?;

        let blocks = (count as u32).div_ceil(BLOCK_SIZE);
        let cfg = LaunchConfig {
            grid_dim: (blocks.max(1), 1, 1),
            block_dim: (BLOCK_SIZE, 1, 1),
            shared_mem_bytes: 0,
        };

        unsafe {
            stream
                .launch_builder(&self.hash_kernel)
                .arg(&output_buf)
                .arg(&params_buf)
                .arg(&self.commit)
                .arg(&self.node_suffix)
                .launch(cfg)
                .map_err(|error| {
                    SearchError::Runtime(format!("CUDA hash kernel launch failed: {error:?}"))
                })?;
        }

        stream
            .clone_dtoh(&output_buf)
            .map_err(|error| SearchError::Runtime(format!("CUDA hash readback failed: {error:?}")))
    }
}
#[cfg(test)]
mod test;
