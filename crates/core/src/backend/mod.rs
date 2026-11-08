use std::sync::Arc;

use crate::candidate::CandidateConfig;

#[cfg(feature = "avx2")]
pub mod avx2;
#[cfg(feature = "cuda")]
pub mod cuda;
#[cfg(feature = "scalar")]
pub mod scalar;
#[cfg(any(feature = "cuda", feature = "wgpu"))]
pub mod utils;
#[cfg(feature = "wgpu")]
pub mod wgpu;

pub const SEARCH_CHUNK_SIZE: u64 = 1 << 16;

#[derive(Clone, Copy, Debug)]
#[allow(clippy::exhaustive_structs)]
pub struct SearchConfig {
    pub candidate: CandidateConfig,
    pub start_index: u64,
    pub max_index: u64,
    pub leading_zero_bits: u32,
}

#[derive(Clone, Debug)]
#[allow(clippy::exhaustive_structs, clippy::module_name_repetitions)]
pub struct BackendOutcome {
    pub hit: Option<u64>,
    pub processed: u64,
    pub evaluated: u64,
    pub backend_name: String,
    pub device_name: Option<String>,
    pub fallback_reasons: Vec<String>,
    pub search_elapsed: Option<std::time::Duration>,
    pub kernel_config: Option<String>,
    pub tuning_elapsed: Option<std::time::Duration>,
}

pub type ProgressReporter = Arc<dyn Fn(u64) + Send + Sync>;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::exhaustive_enums)]
pub enum SearchError {
    #[error("backend unavailable: {0}")]
    Unavailable(String),
    #[error("invalid search configuration: {0}")]
    InvalidConfig(String),
    #[error("backend runtime error: {0}")]
    Runtime(String),
}
