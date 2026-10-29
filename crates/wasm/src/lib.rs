#![allow(clippy::pub_use)]

#[cfg(feature = "wasm-controller")]
pub use wasm_bindgen_rayon::init_thread_pool;

pub use xray_qbrute_core::candidate;
#[cfg(feature = "wasm-simd")]
pub use xray_qbrute_core::sha512_constants;

mod web;
