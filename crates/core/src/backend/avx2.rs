use rayon::prelude::*;

use crate::candidate::{self, CandidateConfig};
use crate::sha512_constants::{IV, ROUND_CONSTANTS};

use super::{
    BackendOutcome, ProgressReporter, SEARCH_CHUNK_SIZE, SearchConfig, SearchError, SearchHit,
};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub fn is_available() -> bool {
    is_x86_feature_detected!("avx2")
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub fn is_available() -> bool {
    false
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub fn first_digest_words(indices: [u64; 4], config: CandidateConfig) -> [u64; 4] {
    assert!(is_available(), "AVX2 is not available on this CPU");
    unsafe { first_digest_words_avx2(indices, config) }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub fn first_digest_words(_: [u64; 4], _: CandidateConfig) -> [u64; 4] {
    unreachable!("AVX2 is not available on this architecture")
}

pub fn search(
    config: &SearchConfig,
    progress: &ProgressReporter,
) -> Result<BackendOutcome, SearchError> {
    if !is_available() {
        return Err(SearchError::Unavailable(
            "AVX2 is not supported by this CPU".to_owned(),
        ));
    }

    let mut start = config.start_index;
    let mut evaluated = 0_u64;

    while start < config.max_index {
        let end = (start + SEARCH_CHUNK_SIZE).min(config.max_index);
        let vector_end = end - (end - start) % 4;
        let min_index = (0..(vector_end - start) / 4)
            .into_par_iter()
            .filter_map(|block| {
                let index = start + block * 4;
                let indices = [index, index + 1, index + 2, index + 3];
                let words = first_digest_words(indices, config.candidate);
                words
                    .iter()
                    .position(|&word| {
                        candidate::matches_leading_zero_bits(word, config.leading_zero_bits)
                    })
                    .map(|lane| indices[lane])
            })
            .min()
            .or_else(|| {
                (vector_end..end).find(|&index| {
                    let word = candidate::digest_word(&candidate::digest(index, config.candidate));
                    candidate::matches_leading_zero_bits(word, config.leading_zero_bits)
                })
            });
        evaluated += vector_end - start;
        if min_index.is_none() {
            evaluated += end - vector_end;
        }

        if let Some(index) = min_index {
            let processed = index + 1;
            progress(processed - start);
            return Ok(BackendOutcome {
                hit: Some(SearchHit { index }),
                processed,
                evaluated,
                backend_name: "avx2-4x".to_owned(),
                device_name: None,
                fallback_reasons: Vec::new(),
                search_elapsed: None,
                kernel_config: None,
                tuning_elapsed: None,
            });
        }

        progress(end - start);
        start = end;
    }

    Ok(BackendOutcome {
        hit: None,
        processed: config.max_index,
        evaluated,
        backend_name: "avx2-4x".to_owned(),
        device_name: None,
        fallback_reasons: Vec::new(),
        search_elapsed: None,
        kernel_config: None,
        tuning_elapsed: None,
    })
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn first_digest_words_avx2(indices: [u64; 4], config: CandidateConfig) -> [u64; 4] {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    macro_rules! rotr {
        ($value:expr, $right:literal) => {{
            let value = $value;
            _mm256_or_si256(
                _mm256_srli_epi64::<$right>(value),
                _mm256_slli_epi64::<{ 64 - $right }>(value),
            )
        }};
    }

    #[inline(always)]
    unsafe fn add3(a: __m256i, b: __m256i, c: __m256i) -> __m256i {
        unsafe { _mm256_add_epi64(_mm256_add_epi64(a, b), c) }
    }

    let candidate_words = indices.map(|index| candidate::candidate_words(index, config));
    let mut schedule = [_mm256_setzero_si256(); 16];
    schedule[0] = _mm256_set_epi64x(
        candidate_words[3][0] as i64,
        candidate_words[2][0] as i64,
        candidate_words[1][0] as i64,
        candidate_words[0][0] as i64,
    );
    schedule[1] = _mm256_set_epi64x(
        candidate_words[3][1] as i64,
        candidate_words[2][1] as i64,
        candidate_words[1][1] as i64,
        candidate_words[0][1] as i64,
    );
    schedule[2] = _mm256_set1_epi64x(0x8000000000000000_u64 as i64);
    schedule[15] = _mm256_set1_epi64x(128);

    let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) = (
        _mm256_set1_epi64x(IV[0] as i64),
        _mm256_set1_epi64x(IV[1] as i64),
        _mm256_set1_epi64x(IV[2] as i64),
        _mm256_set1_epi64x(IV[3] as i64),
        _mm256_set1_epi64x(IV[4] as i64),
        _mm256_set1_epi64x(IV[5] as i64),
        _mm256_set1_epi64x(IV[6] as i64),
        _mm256_set1_epi64x(IV[7] as i64),
    );

    for (round, &constant) in ROUND_CONSTANTS.iter().enumerate() {
        let word = if round < 16 {
            schedule[round]
        } else {
            let x = schedule[(round + 1) & 15];
            let y = schedule[(round + 14) & 15];
            let (sigma0, sigma1) = (
                _mm256_xor_si256(
                    _mm256_xor_si256(rotr!(x, 1), rotr!(x, 8)),
                    _mm256_srli_epi64::<7>(x),
                ),
                _mm256_xor_si256(
                    _mm256_xor_si256(rotr!(y, 19), rotr!(y, 61)),
                    _mm256_srli_epi64::<6>(y),
                ),
            );
            let word = _mm256_add_epi64(
                _mm256_add_epi64(sigma1, schedule[(round + 9) & 15]),
                _mm256_add_epi64(sigma0, schedule[round & 15]),
            );
            schedule[round & 15] = word;
            word
        };

        let (sigma1, ch, constant) = (
            _mm256_xor_si256(_mm256_xor_si256(rotr!(e, 14), rotr!(e, 18)), rotr!(e, 41)),
            _mm256_xor_si256(_mm256_and_si256(e, f), _mm256_andnot_si256(e, g)),
            _mm256_set1_epi64x(constant as i64),
        );
        let temp1 = _mm256_add_epi64(
            unsafe { add3(h, constant, _mm256_add_epi64(sigma1, ch)) },
            word,
        );
        let (sigma0, majority) = (
            _mm256_xor_si256(_mm256_xor_si256(rotr!(a, 28), rotr!(a, 34)), rotr!(a, 39)),
            _mm256_xor_si256(
                _mm256_xor_si256(_mm256_and_si256(a, b), _mm256_and_si256(a, c)),
                _mm256_and_si256(b, c),
            ),
        );
        let temp2 = _mm256_add_epi64(sigma0, majority);

        h = g;
        g = f;
        f = e;
        e = _mm256_add_epi64(d, temp1);
        d = c;
        c = b;
        b = a;
        a = _mm256_add_epi64(temp1, temp2);
    }

    let digest_word = _mm256_add_epi64(a, _mm256_set1_epi64x(IV[0] as i64));
    let mut output = [0_u64; 4];
    unsafe { _mm256_storeu_si256(output.as_mut_ptr().cast(), digest_word) };
    output
}

#[cfg(test)]
mod test;
