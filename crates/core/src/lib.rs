// wgpu's deeply nested auto-trait evaluation overflows the default limit under clippy -D warnings.
#![recursion_limit = "256"]

#[cfg(not(target_family = "wasm"))]
pub mod backend;
pub mod candidate;
#[cfg(not(target_family = "wasm"))]
pub mod search;
#[cfg(any(not(target_family = "wasm"), feature = "wasm-simd"))]
pub mod sha512_constants;
