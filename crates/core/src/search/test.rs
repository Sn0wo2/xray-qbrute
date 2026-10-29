#![allow(clippy::expect_used, clippy::panic, clippy::print_stderr)]

use std::sync::Arc;

#[cfg(feature = "wgpu")]
use clap::ValueEnum;

use super::*;
use crate::candidate::CandidateConfig;

const CONFIG: CandidateConfig = CandidateConfig {
    commit: 0xeb36_6895,
    node_suffix: 0xebac_62b9,
};

fn config() -> SearchConfig {
    SearchConfig {
        candidate: CONFIG,
        start_index: 0,
        max_index: 256,
        leading_zero_bits: 0,
    }
}

#[test]
#[cfg(feature = "scalar")]
fn scalar_selection_is_explicit() {
    let progress: ProgressReporter = Arc::new(|_| {});
    let outcome = search(&config(), BackendKind::Scalar, progress).expect("scalar works");
    assert_eq!(outcome.backend_name, "scalar");
    assert!(outcome.fallback_reasons.is_empty());
}

#[test]
#[cfg(feature = "scalar")]
fn prepared_scalar_search_is_reusable() {
    let config = config();
    let prepared = prepare(&config, BackendKind::Scalar).expect("scalar preparation works");
    let progress: ProgressReporter = Arc::new(|_| {});
    let first = prepared
        .search(&config, &progress)
        .expect("first search works");
    let second = prepared
        .search(&config, &progress)
        .expect("second search works");

    assert_eq!(second.hit, first.hit);
    assert_eq!(second.processed, first.processed);
    assert_eq!(second.evaluated, first.evaluated);
}

#[test]
#[cfg(feature = "scalar")]
fn invalid_range_is_rejected_before_dispatch() {
    let progress: ProgressReporter = Arc::new(|_| {});
    let mut invalid = config();
    invalid.max_index = MAX_UUID_INDEX + 1;
    let error = search(&invalid, BackendKind::Scalar, progress).expect_err("range is invalid");
    assert!(matches!(error, SearchError::InvalidConfig(_)));
}

#[test]
#[cfg(feature = "scalar")]
fn start_index_exceeding_max_index_is_rejected() {
    let progress: ProgressReporter = Arc::new(|_| {});
    let mut invalid = config();
    invalid.start_index = invalid.max_index + 1;
    let error = search(&invalid, BackendKind::Scalar, progress).expect_err("start exceeds max");
    assert!(matches!(error, SearchError::InvalidConfig(_)));
}

#[test]
#[cfg(feature = "wgpu")]
fn gpu_backend_cli_names_are_explicit() {
    let cases = [
        (BackendKind::WgpuVulkan, "wgpu-vulkan"),
        (BackendKind::WgpuMetal, "wgpu-metal"),
    ];

    for (backend, name) in cases {
        assert_eq!(backend.to_string(), name);
        assert_eq!(BackendKind::from_str(name, false), Ok(backend));
    }
    assert!(BackendKind::from_str("wgpu", false).is_err());
}

#[test]
#[cfg(feature = "scalar")]
fn available_backends_return_the_same_minimum_match() {
    let config = config();
    let scalar = search(&config, BackendKind::Scalar, Arc::new(|_| {})).expect("scalar works");
    assert_eq!(scalar.hit, Some(backend::SearchHit { index: 0 }));
    assert_eq!(scalar.processed, 1);

    #[cfg(feature = "avx2")]
    if backend::avx2::is_available() {
        let avx2 = search(&config, BackendKind::Avx2, Arc::new(|_| {})).expect("AVX2 works");
        assert_eq!(avx2.backend_name, "avx2-4x");
        assert_eq!(avx2.hit, scalar.hit);
        assert_eq!(avx2.processed, scalar.processed);
    }

    #[cfg(feature = "wgpu")]
    match search(&config, BackendKind::WgpuVulkan, Arc::new(|_| {})) {
        Ok(wgpu) => {
            assert_eq!(wgpu.hit, scalar.hit);
            assert_eq!(wgpu.processed, scalar.processed);
        }
        Err(SearchError::Unavailable(reason)) => {
            eprintln!("Skipping WGPU parity assertion: {reason}");
        }
        Err(error) => panic!("WGPU search failed unexpectedly: {error}"),
    }
}
