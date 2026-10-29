#![allow(clippy::expect_used, clippy::indexing_slicing)]

use super::*;
use crate::backend::SearchHit;
use crate::candidate::CandidateConfig;

const CONFIG: CandidateConfig = CandidateConfig {
    commit: 0xeb36_6895,
    node_suffix: 0xebac_62b9,
};

#[test]
fn rustcrypto_avx2_search_handles_scalar_tail() {
    if !is_available() {
        return;
    }

    let progress: ProgressReporter = std::sync::Arc::new(|_| {});
    let config = SearchConfig {
        candidate: CONFIG,
        start_index: 0,
        max_index: 3,
        leading_zero_bits: 0,
    };
    let outcome = search(&config, &progress).expect("AVX2 must be available");
    assert_eq!(outcome.hit, Some(SearchHit { index: 0 }));
    assert_eq!(outcome.processed, 1);
    assert_eq!(outcome.backend_name, "avx2-4x");
}

#[test]
fn avx2_first_words_match_rustcrypto() {
    if !is_available() {
        return;
    }

    let bases = [
        0,
        1,
        0xffff,
        1 << 16,
        1 << 28,
        1 << 30,
        1 << 42,
        (1 << 58) - 4,
    ];
    for base in bases {
        for offset in 0..128_u64 {
            let indices = [
                base + offset,
                base + offset + 1,
                base + offset + 2,
                base + offset + 3,
            ];
            if indices[3] >= (1 << 58) {
                continue;
            }

            let actual = first_digest_words(indices, CONFIG);
            for (lane, &index) in indices.iter().enumerate() {
                let expected = candidate::digest_word(&candidate::digest(index, CONFIG));
                assert_eq!(actual[lane], expected, "index {index}");
            }
        }
    }
}
