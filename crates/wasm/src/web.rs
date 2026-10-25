use wasm_bindgen::prelude::*;

use crate::candidate::{self, CandidateConfig, MAX_UUID_INDEX};
#[cfg(all(feature = "wasm-simd", target_family = "wasm"))]
use crate::sha512_constants::{IV, ROUND_CONSTANTS};

#[cfg(feature = "wasm-controller")]
mod controller;
#[cfg(feature = "wasm-webgpu")]
mod gpu;
#[cfg(feature = "wasm-controller")]
mod protocol;
#[cfg(feature = "wasm-controller")]
mod search_runner;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

pub(crate) fn search_scalar_batch(
    commit: u32,
    node_suffix: u32,
    base: u64,
    count: u32,
    leading_zero_bits: u32,
) -> u64 {
    let end = base.saturating_add(u64::from(count)).min(MAX_UUID_INDEX);
    let config = CandidateConfig {
        commit,
        node_suffix,
    };
    (base..end)
        .find(|&index| scalar_matches(index, config, leading_zero_bits))
        .unwrap_or(u64::MAX)
}

#[cfg(all(feature = "wasm-simd", target_family = "wasm"))]
pub(crate) fn search_simd_batch(
    commit: u32,
    node_suffix: u32,
    base: u64,
    count: u32,
    leading_zero_bits: u32,
) -> u64 {
    let end = base.saturating_add(u64::from(count)).min(MAX_UUID_INDEX);
    let config = CandidateConfig {
        commit,
        node_suffix,
    };
    search_simd_range(base, end, config, leading_zero_bits).unwrap_or(u64::MAX)
}

pub(crate) fn result_details(commit: u32, node_suffix: u32, index: u64) -> (String, String) {
    let config = CandidateConfig {
        commit,
        node_suffix,
    };
    let uuid = candidate::bytes_to_uuid_string(&candidate::candidate_bytes(index, config));
    let hash = candidate::digest(index, config);
    let digits = b"0123456789abcdef";
    let mut hash_prefix = String::with_capacity(20);
    for &byte in &hash[..10] {
        hash_prefix.push(char::from(digits[usize::from(byte >> 4)]));
        hash_prefix.push(char::from(digits[usize::from(byte & 0x0f)]));
    }
    (uuid, hash_prefix)
}

#[cfg(all(feature = "wasm-simd", target_family = "wasm"))]
fn search_simd_range(base: u64, end: u64, config: CandidateConfig, bits: u32) -> Option<u64> {
    let vector_end = end - (end - base) % 2;
    let mut index = base;
    while index < vector_end {
        let words = simd_words([index, index + 1], config);
        if candidate::matches_leading_zero_bits(words[0], bits) {
            return Some(index);
        }
        if candidate::matches_leading_zero_bits(words[1], bits) {
            return Some(index + 1);
        }
        index += 2;
    }
    (vector_end..end).find(|&tail| scalar_matches(tail, config, bits))
}

fn scalar_matches(index: u64, config: CandidateConfig, bits: u32) -> bool {
    let word = candidate::digest_word(&candidate::digest(index, config));
    candidate::matches_leading_zero_bits(word, bits)
}

#[cfg(all(feature = "wasm-simd", target_family = "wasm"))]
#[target_feature(enable = "simd128")]
fn simd_words(indices: [u64; 2], config: CandidateConfig) -> [u64; 2] {
    use core::arch::wasm32::*;

    macro_rules! rotr {
        ($value:expr, $right:literal) => {{
            let value = $value;
            v128_or(u64x2_shr(value, $right), i64x2_shl(value, 64 - $right))
        }};
    }

    let candidates = indices.map(|index| candidate::candidate_words(index, config));
    let pair = |left: u64, right: u64| u64x2_replace_lane::<1>(u64x2_splat(left), right);
    let zero = u64x2_splat(0);
    let mut schedule = [zero; 16];
    schedule[0] = pair(candidates[0][0], candidates[1][0]);
    schedule[1] = pair(candidates[0][1], candidates[1][1]);
    schedule[2] = u64x2_splat(0x8000000000000000);
    schedule[15] = u64x2_splat(128);
    let mut state = IV.map(|word| u64x2_splat(word));

    for (round, &constant) in ROUND_CONSTANTS.iter().enumerate() {
        let word = if round < 16 {
            schedule[round]
        } else {
            let x = schedule[(round + 1) & 15];
            let y = schedule[(round + 14) & 15];
            let sigma0 = v128_xor(v128_xor(rotr!(x, 1), rotr!(x, 8)), u64x2_shr(x, 7));
            let sigma1 = v128_xor(v128_xor(rotr!(y, 19), rotr!(y, 61)), u64x2_shr(y, 6));
            let word = i64x2_add(
                i64x2_add(sigma1, schedule[(round + 9) & 15]),
                i64x2_add(sigma0, schedule[round & 15]),
            );
            schedule[round & 15] = word;
            word
        };

        let [a, b, c, d, e, f, g, h] = state;
        let sigma1 = v128_xor(v128_xor(rotr!(e, 14), rotr!(e, 18)), rotr!(e, 41));
        let choice = v128_xor(v128_and(e, f), v128_andnot(g, e));
        let temp1 = i64x2_add(
            i64x2_add(
                i64x2_add(h, u64x2_splat(constant)),
                i64x2_add(sigma1, choice),
            ),
            word,
        );
        let sigma0 = v128_xor(v128_xor(rotr!(a, 28), rotr!(a, 34)), rotr!(a, 39));
        let majority = v128_xor(v128_xor(v128_and(a, b), v128_and(a, c)), v128_and(b, c));
        state = [
            i64x2_add(temp1, i64x2_add(sigma0, majority)),
            a,
            b,
            c,
            i64x2_add(d, temp1),
            e,
            f,
            g,
        ];
    }

    let output = i64x2_add(state[0], u64x2_splat(IV[0]));
    [
        u64x2_extract_lane::<0>(output),
        u64x2_extract_lane::<1>(output),
    ]
}
