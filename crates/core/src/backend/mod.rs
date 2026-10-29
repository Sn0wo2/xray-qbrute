use std::sync::Arc;

use crate::candidate::CandidateConfig;

#[cfg(feature = "avx2")]
pub mod avx2;
#[cfg(feature = "cuda")]
pub mod cuda;
#[cfg(feature = "scalar")]
pub mod scalar;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::exhaustive_structs)]
pub struct SearchHit {
    pub index: u64,
}

#[derive(Clone, Debug)]
#[allow(clippy::exhaustive_structs, clippy::module_name_repetitions)]
pub struct BackendOutcome {
    pub hit: Option<SearchHit>,
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

pub fn hit_probability(attempts: u64, leading_zero_bits: u32) -> f64 {
    let work_factor = 2_f64.powi(leading_zero_bits as i32);
    -(-(attempts as f64) / work_factor).exp_m1()
}

pub fn expected_search_seconds(leading_zero_bits: u32, rate_per_second: f64) -> f64 {
    2_f64.powi(leading_zero_bits as i32) / rate_per_second.max(f64::EPSILON)
}

#[derive(Debug)]
#[allow(clippy::exhaustive_enums)]
pub enum SearchError {
    Unavailable(String),
    InvalidConfig(String),
    Runtime(String),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) => write!(formatter, "backend unavailable: {message}"),
            Self::InvalidConfig(message) => {
                write!(formatter, "invalid search configuration: {message}")
            }
            Self::Runtime(message) => write!(formatter, "backend runtime error: {message}"),
        }
    }
}

impl std::error::Error for SearchError {}
