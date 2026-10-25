use std::sync::Arc;

use super::*;
use crate::candidate;

const CONFIG: CandidateConfig = CandidateConfig {
    commit: 0xeb366895,
    node_suffix: 0xebac62b9,
};
const ALTERNATE_CONFIG: CandidateConfig = CandidateConfig {
    commit: 0x01234567,
    node_suffix: 0x89abcdef,
};

#[cfg(target_os = "windows")]
const TEST_BACKENDS: &[WgpuBackend] = &[WgpuBackend::Vulkan];

#[cfg(target_os = "macos")]
const TEST_BACKENDS: &[WgpuBackend] = &[WgpuBackend::Metal];

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const TEST_BACKENDS: &[WgpuBackend] = &[WgpuBackend::Vulkan];

fn context_or_skip(backend: WgpuBackend, config: SearchConfig) -> Option<GpuContext> {
    match create_context(backend, config) {
        Ok(context) => Some(context),
        Err(SearchError::Unavailable(reason)) => {
            eprintln!("Skipping {} GPU test: {reason}", backend.name());
            None
        }
        Err(error) => panic!("GPU initialization failed unexpectedly: {error}"),
    }
}

#[test]
fn gpu_hashes_match_scalar() {
    for &backend in TEST_BACKENDS {
        for candidate_config in [CONFIG, ALTERNATE_CONFIG] {
            let search_config = SearchConfig {
                candidate: candidate_config,
                start_index: 0,
                max_index: 256,
                leading_zero_bits: 64,
            };
            let Some(context) = context_or_skip(backend, search_config) else {
                continue;
            };

            let bases = [0, 1 << 28, 1 << 30, 1 << 42, (1 << 58) - 256];
            for base in bases {
                let actual = context
                    .hash_first_words(base, 256)
                    .expect("GPU hash readback must succeed");
                assert_eq!(actual.len(), 256);
                for (offset, &actual_word) in actual.iter().enumerate() {
                    let index = base + offset as u64;
                    let expected =
                        candidate::digest_word(&candidate::digest(index, candidate_config));
                    assert_eq!(actual_word, expected, "{} index {index}", backend.name());
                }
            }
        }
    }
}

#[test]
fn gpu_search_finds_valid_hit() {
    let progress: ProgressReporter = Arc::new(|_| {});
    let config = SearchConfig {
        candidate: CONFIG,
        start_index: 0,
        max_index: 65_536,
        leading_zero_bits: 8,
    };

    for &backend in TEST_BACKENDS {
        let outcome = match search(&config, backend, &progress) {
            Ok(outcome) => outcome,
            Err(SearchError::Unavailable(reason)) => {
                eprintln!("Skipping {} GPU test: {reason}", backend.name());
                continue;
            }
            Err(error) => panic!("{} GPU search failed unexpectedly: {error}", backend.name()),
        };
        assert_eq!(outcome.backend_name, backend.name());
        let hit = outcome
            .hit
            .expect("8-bit predicate should hit in 65536 candidates");
        assert!(hit.index < config.max_index);
        assert!(outcome.processed > 0);
        assert!(outcome.processed <= config.max_index);
        let word = candidate::digest_word(&candidate::digest(hit.index, CONFIG));
        assert!(candidate::matches_leading_zero_bits(
            word,
            config.leading_zero_bits
        ));
    }
}

#[test]
fn gpu_search_returns_minimum_match() {
    let progress: ProgressReporter = Arc::new(|_| {});
    let config = SearchConfig {
        candidate: CONFIG,
        start_index: 0,
        max_index: 256,
        leading_zero_bits: 0,
    };

    let backend = TEST_BACKENDS[0];
    let outcome = match search(&config, backend, &progress) {
        Ok(outcome) => outcome,
        Err(SearchError::Unavailable(reason)) => {
            eprintln!("Skipping GPU test: {reason}");
            return;
        }
        Err(error) => panic!("GPU search failed unexpectedly: {error}"),
    };

    assert_eq!(outcome.hit, Some(SearchHit { index: 0 }));
    assert_eq!(outcome.processed, 1);
    assert_eq!(outcome.evaluated, config.max_index);
}

#[test]
fn gpu_session_reuses_resources() {
    let config = SearchConfig {
        candidate: CONFIG,
        start_index: 0,
        max_index: 256,
        leading_zero_bits: 0,
    };
    let backend = TEST_BACKENDS[0];
    let session = match WgpuSearchSession::new(backend, config) {
        Ok(session) => session,
        Err(SearchError::Unavailable(reason)) => {
            eprintln!("Skipping GPU test: {reason}");
            return;
        }
        Err(error) => panic!("GPU initialization failed unexpectedly: {error}"),
    };
    let progress: ProgressReporter = Arc::new(|_| {});
    let first = session
        .search(&config, &progress)
        .expect("first warm search");
    let second = session
        .search(&config, &progress)
        .expect("second warm search");

    assert_eq!(first.hit, Some(SearchHit { index: 0 }));
    assert_eq!(second.hit, first.hit);
    assert_eq!(second.processed, first.processed);
    assert_eq!(second.evaluated, first.evaluated);
}

#[test]
fn browser_webgpu_shader_matches_scalar_minimums() {
    let backend = TEST_BACKENDS[0];
    let initialization = SearchConfig {
        candidate: CONFIG,
        start_index: 0,
        max_index: 4_096,
        leading_zero_bits: 8,
    };
    let Some(context) = context_or_skip(backend, initialization) else {
        return;
    };
    let shader = context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("xray-qbrute browser SHA-512 WGSL test"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!(concat!(env!("OUT_DIR"), "/sha512_web.wgsl")).into(),
            ),
        });
    let layout = context
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("xray-qbrute browser search test layout"),
            entries: &[storage_binding(0, true), storage_binding(1, false)],
        });
    let pipeline_layout = context
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("xray-qbrute browser search test pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
    let kernel = KernelConfig {
        workgroup_size: 128,
        candidates_per_invocation: 8,
    };
    let buffers = create_search_buffers(&context.device, &layout);

    for candidate_config in [CONFIG, ALTERNATE_CONFIG] {
        for leading_zero_bits in [0, 8, 12] {
            let pipeline = create_search_pipeline(
                &context.device,
                &shader,
                &pipeline_layout,
                candidate_config,
                leading_zero_bits,
                kernel,
                None,
                "xray-qbrute browser search test pipeline",
            );
            for base in [0, 1 << 28, 1 << 30, 1 << 42, (1 << 58) - 4_096] {
                let count = 4_096;
                let expected = (base..base + count).find(|&index| {
                    let word = candidate::digest_word(&candidate::digest(index, candidate_config));
                    candidate::matches_leading_zero_bits(word, leading_zero_bits)
                });
                let result = run_search_pipeline(
                    &context.device,
                    &context.queue,
                    &pipeline,
                    kernel,
                    &buffers,
                    base,
                    count,
                    "browser-webgpu-test",
                )
                .expect("browser-compatible WebGPU shader must dispatch and read back");
                let actual =
                    (result.min_offset != u32::MAX).then(|| base + u64::from(result.min_offset));
                assert_eq!(actual, expected, "base={base}, bits={leading_zero_bits}");
            }
        }
    }
}
