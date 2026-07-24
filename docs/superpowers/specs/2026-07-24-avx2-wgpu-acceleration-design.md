# AVX2 and Vulkan GPU Acceleration Design

## Goal

Add two accelerated search backends while preserving the current search semantics: hash the raw 16 UUID bytes with SHA-512 and accept a digest whose first 33 bits are zero.

The supported backends are:

- `scalar`: the existing `sha2` implementation, parallelized in coarse chunks.
- `avx2`: four independent fixed-size SHA-512 computations in the four `u64` lanes of AVX2 vectors.
- `wgpu`: a WGSL compute shader dispatched through wgpu's Vulkan backend.
- `auto`: prefer `wgpu`, then `avx2`, then `scalar`.

CUDA, DirectX 12, OpenCL, and browser WebGPU are outside this change.

## Compatibility

The existing `--commit-last8` and `--node-suffix` arguments remain supported with the same defaults. The UUID bit layout and the 58-bit index range are unchanged. A new `--backend` option selects the implementation, and `--max-index` permits bounded correctness and performance runs without changing the default `2^58` range.

An explicitly requested unavailable backend returns an error. `auto` records the rejected backend reason and falls back. All accelerated hits are recomputed with `sha2::Sha512` before being reported.

## Architecture

The binary is split into a small CLI and a reusable library:

- `src/candidate.rs`: index-to-UUID mapping, fixed SHA-512 block words, formatting, and digest predicate.
- `src/backend/scalar.rs`: scalar reference search over coarse Rayon chunks.
- `src/backend/avx2.rs`: runtime detection plus four-lane SHA-512 compression.
- `src/backend/wgpu.rs`: Vulkan adapter setup, buffers, dispatch batching, and result readback.
- `src/shaders/sha512_search.wgsl`: fixed-message SHA-512 GPU kernel.
- `src/search.rs`: backend selection, shared result types, cancellation, and progress accounting.
- `src/lib.rs`: public module boundary used by the binary and tests.
- `src/main.rs`: argument parsing, progress display, and result output.

The hot loops return a `SearchHit` containing the candidate index and UUID bytes. Formatting and full digest computation stay outside the hot path.

## Candidate Encoding

Each 16-byte candidate occupies the first two big-endian words of one padded SHA-512 block:

```text
W0 = commit << 32 | time_mid << 16 | 0x4000 | time_high
W1 = variant << 48 | node_prefix << 32 | node_suffix
W2 = 0x8000000000000000
W3..W14 = 0
W15 = 128
```

The fields are derived from the 58-bit index exactly as in the current `make_uuid_bytes` function. Tests compare both encodings for fixed boundary values.

## AVX2 Backend

The AVX2 kernel uses a structure-of-arrays layout. SHA-512 state words `a` through `h` and the 16-word circular message schedule are `__m256i` values; each of the four lanes represents one independent candidate. Constant rotates are expressed as paired AVX2 shifts and ORs.

Only the final first digest word is accumulated and tested. The predicate is equivalent to `(h0 >> 31) == 0`. A matching lane is extracted and returned; the remaining range tail is handled by the scalar reference path. Runtime dispatch uses `is_x86_feature_detected!("avx2")`, so unsupported processors never execute AVX2 instructions.

Rayon parallelizes coarse index chunks. Each worker loops locally and updates the global processed count once when its chunk exits, replacing the current per-candidate `fetch_add`.

## wgpu Vulkan Backend

wgpu is initialized with the Vulkan backend and a high-performance adapter. The selected adapter must expose `SHADER_INT64`; the RTX 4060 in the target environment advertises Vulkan `shaderInt64` support.

The shader uses one workgroup of 256 invocations. Each invocation processes a fixed sequence of candidates from a batch, constructs `W0` and `W1` directly, performs the 80 SHA-512 rounds with a 16-word schedule, and checks the first digest word. A `u32` atomic flag elects one winning invocation, which writes the index as low/high `u32` words. A 64-bit atomic is not required.

The host dispatches bounded batches and reads back only the result structure after each batch. Candidate and digest arrays are never transferred. A returned index is reconstructed on the CPU and verified with the scalar implementation before reporting.

## Error Handling

Configuration parsing rejects non-eight-digit hexadecimal values and ranges above `2^58`. GPU initialization errors include the missing adapter or feature. Device-loss and map failures abort an explicit `wgpu` search; in `auto`, only initialization failure permits fallback, while a failure after dispatch is reported instead of silently repeating work on another backend.

## Testing and Verification

Tests cover:

- UUID byte layout and fixed block words at zero, field boundaries, and `2^58 - 1`.
- The scalar predicate and full SHA-512 digest.
- AVX2 first-word output against scalar SHA-512 for many four-index batches.
- WGSL output against scalar SHA-512 through a test-only hash dispatch.
- Bounded scalar, AVX2, and GPU searches using a reduced test difficulty.
- Backend availability and fallback behavior.

Required verification is `cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --release`, and bounded release-mode runs for each available backend. Throughput is reported from processed candidates divided by elapsed wall time; no claim about speedup is made without those measurements.

