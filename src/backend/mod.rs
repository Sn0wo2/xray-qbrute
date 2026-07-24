use std::sync::Arc;

use crate::candidate::CandidateConfig;

pub mod avx2;
pub mod scalar;
pub mod wgpu;

pub const SEARCH_CHUNK_SIZE: u64 = 1 << 16;

#[derive(Clone, Copy, Debug)]
pub struct SearchConfig {
    pub candidate: CandidateConfig,
    pub max_index: u64,
    pub leading_zero_bits: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchHit {
    pub index: u64,
}

#[derive(Clone, Debug)]
pub struct BackendOutcome {
    pub hit: Option<SearchHit>,
    pub processed: u64,
    pub backend_name: String,
    pub device_name: Option<String>,
    pub fallback_reasons: Vec<String>,
}

pub type ProgressReporter = Arc<dyn Fn(u64) + Send + Sync>;

#[derive(Debug)]
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
