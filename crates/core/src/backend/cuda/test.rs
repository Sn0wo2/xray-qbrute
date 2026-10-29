use super::*;
use crate::backend::SearchHit;
use crate::candidate::{self, CandidateConfig};

const CONFIG: CandidateConfig = CandidateConfig {
    commit: 0xeb366895,
    node_suffix: 0xebac62b9,
};

fn session_or_skip() -> Option<CudaSearchSession> {
    let config = SearchConfig {
        candidate: CONFIG,
        start_index: 0,
        max_index: 1 << 16,
        leading_zero_bits: 33,
    };
    match CudaSearchSession::new(config) {
        Ok(session) => Some(session),
        Err(SearchError::Unavailable(reason)) => {
            eprintln!("Skipping CUDA test: {reason}");
            None
        }
        Err(error) => panic!("CUDA initialization failed unexpectedly: {error}"),
    }
}

#[test]
fn cuda_search_reports_a_valid_hit() {
    let Some(session) = session_or_skip() else {
        return;
    };
    session.warm_up().expect("CUDA warm-up must succeed");

    let progress: ProgressReporter = Arc::new(|_| {});
    let config = SearchConfig {
        candidate: CONFIG,
        start_index: 0,
        max_index: 3,
        leading_zero_bits: 0,
    };
    let outcome = session
        .search(&config, &progress)
        .expect("CUDA search must succeed");
    assert_eq!(outcome.hit, Some(SearchHit { index: 0 }));
    assert_eq!(outcome.processed, 1);
    assert_eq!(outcome.backend_name, "cuda");
}

#[test]
fn cuda_first_words_match_rustcrypto() {
    let Some(session) = session_or_skip() else {
        return;
    };

    let bases = [
        0,
        1,
        0xffff,
        1 << 16,
        1 << 28,
        1 << 30,
        1 << 42,
        (1 << 58) - 256,
    ];
    for base in bases {
        let actual = session
            .hash_first_words(base, 256)
            .expect("CUDA hash readback must succeed");
        assert_eq!(actual.len(), 256);
        for (offset, &actual_word) in actual.iter().enumerate() {
            let index = base + offset as u64;
            if index >= (1 << 58) {
                break;
            }
            let expected = candidate::digest_word(&candidate::digest(index, CONFIG));
            assert_eq!(actual_word, expected, "index {index}");
        }
    }
}
