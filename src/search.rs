use clap::ValueEnum;

use crate::backend::{self, BackendOutcome, ProgressReporter, SearchConfig, SearchError};
use crate::candidate::MAX_UUID_INDEX;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum BackendKind {
    Auto,
    Scalar,
    Avx2,
    Wgpu,
}

impl std::fmt::Display for BackendKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Auto => "auto",
            Self::Scalar => "scalar",
            Self::Avx2 => "avx2",
            Self::Wgpu => "wgpu",
        })
    }
}

pub fn search(
    config: &SearchConfig,
    backend: BackendKind,
    progress: ProgressReporter,
) -> Result<BackendOutcome, SearchError> {
    validate_config(config)?;

    match backend {
        BackendKind::Auto => search_auto(config, &progress),
        selected => {
            let mut outcome = run_backend(config, selected, &progress)?;
            outcome.fallback_reasons = Vec::new();
            Ok(outcome)
        }
    }
}

fn validate_config(config: &SearchConfig) -> Result<(), SearchError> {
    if config.max_index == 0 {
        return Err(SearchError::InvalidConfig(
            "max_index must be greater than zero".to_owned(),
        ));
    }
    if config.max_index > MAX_UUID_INDEX {
        return Err(SearchError::InvalidConfig(format!(
            "max_index must be at most {MAX_UUID_INDEX}"
        )));
    }
    if config.leading_zero_bits > 64 {
        return Err(SearchError::InvalidConfig(
            "leading_zero_bits must be between 0 and 64".to_owned(),
        ));
    }
    Ok(())
}

fn search_auto(
    config: &SearchConfig,
    progress: &ProgressReporter,
) -> Result<BackendOutcome, SearchError> {
    let mut fallback_reasons = Vec::new();
    for backend in [BackendKind::Wgpu, BackendKind::Avx2, BackendKind::Scalar] {
        match run_backend(config, backend, progress) {
            Ok(mut outcome) => {
                outcome.fallback_reasons = fallback_reasons;
                return Ok(outcome);
            }
            Err(SearchError::Unavailable(reason)) => {
                fallback_reasons.push(format!("{backend}: {reason}"));
            }
            Err(error) => return Err(error),
        }
    }

    Err(SearchError::Runtime(
        "auto backend selection exhausted without a usable backend".to_owned(),
    ))
}

fn run_backend(
    config: &SearchConfig,
    backend: BackendKind,
    progress: &ProgressReporter,
) -> Result<BackendOutcome, SearchError> {
    match backend {
        BackendKind::Auto => unreachable!("auto is expanded before backend dispatch"),
        BackendKind::Scalar => Ok(backend::scalar::search(config, progress)),
        BackendKind::Avx2 => backend::avx2::search(config, progress),
        BackendKind::Wgpu => backend::wgpu::search(config, progress),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::candidate::CandidateConfig;

    const CONFIG: CandidateConfig = CandidateConfig {
        commit: 0xeb366895,
        node_suffix: 0xebac62b9,
    };

    fn config() -> SearchConfig {
        SearchConfig {
            candidate: CONFIG,
            max_index: 256,
            leading_zero_bits: 0,
        }
    }

    #[test]
    fn scalar_selection_is_explicit() {
        let progress: ProgressReporter = Arc::new(|_| {});
        let outcome = search(&config(), BackendKind::Scalar, progress).expect("scalar works");
        assert_eq!(outcome.backend_name, "scalar");
        assert!(outcome.fallback_reasons.is_empty());
    }

    #[test]
    fn invalid_range_is_rejected_before_dispatch() {
        let progress: ProgressReporter = Arc::new(|_| {});
        let mut invalid = config();
        invalid.max_index = MAX_UUID_INDEX + 1;
        let error = search(&invalid, BackendKind::Scalar, progress).expect_err("range is invalid");
        assert!(matches!(error, SearchError::InvalidConfig(_)));
    }
}
