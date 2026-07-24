# AVX2 and wgpu Acceleration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add verified AVX2 four-lane and wgpu Vulkan compute backends for the fixed 16-byte SHA-512 brute-force search.

**Architecture:** Move candidate encoding and scalar hashing into a library reference implementation. CPU search operates on coarse Rayon chunks and dispatches either scalar or AVX2 x4 hashing; GPU search sends index ranges to a WGSL compute pipeline and reads back only a winning index. Every accelerated result is verified by the scalar implementation.

**Tech Stack:** Rust 2024, sha2 0.11, Rayon, std::arch AVX2 intrinsics, wgpu 30 Vulkan backend, WGSL with native `u64`, pollster, bytemuck, clap, indicatif.

---

### Task 1: Shared candidate and scalar reference core

**Files:**
- Modify: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/candidate.rs`
- Create: `src/backend/mod.rs`
- Create: `src/backend/scalar.rs`

- [ ] **Step 1: Add candidate encoding tests**

Add unit tests asserting that `candidate_bytes(0, 0xeb366895, 0xebac62b9)` is:

```rust
[
    0xeb, 0x36, 0x68, 0x95, 0x00, 0x00, 0x40, 0x00,
    0x80, 0x00, 0x00, 0x00, 0xeb, 0xac, 0x62, 0xb9,
]
```

For indices `0`, `0xffff`, `1 << 16`, `1 << 28`, `1 << 30`, `1 << 42`, and `(1 << 58) - 1`, assert that concatenating `candidate_words(index, commit, suffix)` as big-endian bytes equals `candidate_bytes`.

- [ ] **Step 2: Run the tests and confirm the missing module failure**

Run: `cargo test candidate --lib`

Expected: compilation fails because the library and candidate functions do not exist.

- [ ] **Step 3: Implement the candidate API**

Expose the following API from `src/candidate.rs`:

```rust
pub const MAX_UUID_INDEX: u64 = 1u64 << 58;
pub const DEFAULT_DIFFICULTY_BITS: u32 = 33;

#[derive(Clone, Copy, Debug)]
pub struct CandidateConfig {
    pub commit: u32,
    pub node_suffix: u32,
}

pub fn candidate_words(index: u64, config: CandidateConfig) -> [u64; 2];
pub fn candidate_bytes(index: u64, config: CandidateConfig) -> [u8; 16];
pub fn bytes_to_uuid_string(bytes: &[u8; 16]) -> String;
pub fn digest(index: u64, config: CandidateConfig) -> [u8; 64];
pub fn digest_matches(hash: &[u8; 64], leading_zero_bits: u32) -> bool;
pub fn digest_word_matches(first_word: u64, leading_zero_bits: u32) -> bool;
```

`digest_word_matches` accepts `0..=64`, returns true for zero bits, and checks `first_word >> (64 - bits) == 0` otherwise.

- [ ] **Step 4: Implement coarse scalar search**

Create shared result types in `src/backend/mod.rs` and scalar search in `src/backend/scalar.rs`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendHit {
    pub index: u64,
}

#[derive(Clone, Debug)]
pub struct BackendOutcome {
    pub hit: Option<BackendHit>,
    pub processed: u64,
    pub backend_name: String,
}
```

The scalar implementation hashes chunks of 65,536 candidates with Rayon, updates an `AtomicU64` once per completed chunk, and calls a shared progress callback once per chunk.

- [ ] **Step 5: Run reference tests**

Run: `cargo test --lib candidate backend::scalar`

Expected: all candidate and scalar tests pass.

### Task 2: AVX2 four-lane SHA-512 backend

**Files:**
- Create: `src/backend/avx2.rs`
- Modify: `src/backend/mod.rs`
- Test: `src/backend/avx2.rs`

- [ ] **Step 1: Add scalar equivalence tests**

For 1,024 deterministic four-index batches, compare the four values returned by:

```rust
unsafe { first_words_x4_avx2(indices, config) }
```

with the first big-endian `u64` from four `candidate::digest` calls. Skip the test only when `is_x86_feature_detected!("avx2")` is false.

- [ ] **Step 2: Run the test and confirm the missing AVX2 function**

Run: `cargo test --release backend::avx2::tests::first_words_match_scalar`

Expected: compilation fails because `first_words_x4_avx2` is not implemented.

- [ ] **Step 3: Implement fixed-block SHA-512 x4**

Implement `#[target_feature(enable = "avx2")] unsafe fn first_words_x4_avx2` using:

```rust
type V = core::arch::x86_64::__m256i;

macro_rules! rotr {
    ($value:expr, $right:literal, $left:literal) => {
        _mm256_or_si256(
            _mm256_srli_epi64::<$right>($value),
            _mm256_slli_epi64::<$left>($value),
        )
    };
}
```

The state is eight broadcast SHA-512 IV vectors. The 16-entry vector schedule starts with lane-wise `W0` and `W1`, fixed padding words, and is expanded in a circular buffer for rounds 16 through 79. After round 79, add only the first IV word to `a`, store four lanes, and return `[u64; 4]`.

- [ ] **Step 4: Implement AVX2 chunk search and runtime guard**

Expose:

```rust
pub fn is_available() -> bool;
pub fn search(
    config: SearchConfig,
    progress: &ProgressReporter,
) -> Result<BackendOutcome, SearchError>;
```

Each worker hashes four consecutive indices per loop, checks all four first words, and falls back to scalar hashing for one to three tail indices. Calling the explicit backend without AVX2 returns `SearchError::Unavailable`.

- [ ] **Step 5: Verify AVX2**

Run: `cargo test --release backend::avx2`

Expected: all AVX2 equivalence and bounded-search tests pass.

### Task 3: WGSL fixed-message SHA-512 kernel

**Files:**
- Create: `src/shaders/sha512_search.wgsl`
- Create: `src/backend/wgpu.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add GPU hash-equivalence test API**

Add a test that initializes a Vulkan adapter, hashes 256 consecutive indices through a test dispatch, and compares each returned first digest word with the scalar reference. Skip only when Vulkan or `SHADER_INT64` is unavailable.

- [ ] **Step 2: Add pinned GPU dependencies**

Use:

```toml
wgpu = { version = "30.0.0", default-features = false, features = ["std", "vulkan", "wgsl"] }
pollster = "1.0.1"
bytemuck = { version = "1.25", features = ["derive"] }
```

Remove the unused `itertools`, `uuid`, and `rand` dependencies.

- [ ] **Step 3: Implement the WGSL hash function**

The shader declares `enable wgpu_int64;`, uses the 80 standard SHA-512 constants, and exposes:

```wgsl
fn candidate_words(index: u64, commit: u32, suffix: u32) -> vec2<u64>;
fn sha512_first_word(index: u64, commit: u32, suffix: u32) -> u64;
```

`sha512_first_word` uses a 16-word circular schedule, eight `u64` state variables, and returns the first IV word plus final `a`.

- [ ] **Step 4: Implement a hash-test compute entry point**

The test entry point writes one first digest word per global invocation to a storage buffer. The host test creates configuration, output, and readback buffers, dispatches one 256-thread workgroup, copies the output to the readback buffer, waits for mapping, and compares all outputs.

- [ ] **Step 5: Verify the GPU hash implementation**

Run: `cargo test --release backend::wgpu::tests::gpu_hashes_match_scalar -- --nocapture`

Expected: the RTX 4060 Vulkan adapter is selected and all 256 words match.

### Task 4: Batched wgpu search backend

**Files:**
- Modify: `src/shaders/sha512_search.wgsl`
- Modify: `src/backend/wgpu.rs`
- Test: `src/backend/wgpu.rs`

- [ ] **Step 1: Add a reduced-difficulty search test**

Use scalar code to locate a hit at eight leading-zero bits in the first 65,536 indices. Run the GPU backend over the same range and assert its returned index is in range and independently satisfies the eight-bit predicate.

- [ ] **Step 2: Implement result election in WGSL**

Use a storage result with a `u32` atomic state plus low/high index words. Each invocation processes 32 consecutive candidates. On a match, `atomicCompareExchangeWeak` elects one invocation, which writes the two index words. Every invocation completes its assigned range so batch accounting remains exact.

- [ ] **Step 3: Implement host-side batching**

Dispatch at most `2^28` candidates per batch. Clear the result buffer before each dispatch, copy it to a mappable readback buffer after completion, reconstruct the winning index, and update progress by the exact batch length.

- [ ] **Step 4: Verify bounded GPU search**

Run: `cargo test --release backend::wgpu::tests::gpu_search_finds_valid_hit -- --nocapture`

Expected: the test returns a scalar-verified hit.

### Task 5: Backend selection and CLI integration

**Files:**
- Create: `src/search.rs`
- Modify: `src/lib.rs`
- Replace: `src/main.rs`

- [ ] **Step 1: Add backend selection tests**

Test parsing of `auto`, `scalar`, `avx2`, and `wgpu`. Test that explicit unavailable backends return an error and that `auto` tries wgpu, AVX2, then scalar.

- [ ] **Step 2: Implement shared search configuration**

Expose:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendKind { Auto, Scalar, Avx2, Wgpu }

#[derive(Clone, Copy, Debug)]
pub struct SearchConfig {
    pub candidate: CandidateConfig,
    pub max_index: u64,
    pub leading_zero_bits: u32,
}

pub type ProgressReporter = Arc<dyn Fn(u64) + Send + Sync>;
pub fn search(config: SearchConfig, backend: BackendKind, progress: ProgressReporter)
    -> Result<BackendOutcome, SearchError>;
```

- [ ] **Step 3: Replace the CLI entry point**

Keep the existing arguments and add:

```rust
#[arg(long, value_enum, default_value_t = BackendArg::Auto)]
backend: BackendArg,

#[arg(long, default_value_t = MAX_UUID_INDEX)]
max_index: u64,

#[arg(long, default_value_t = false)]
no_progress: bool,
```

Print the selected backend and GPU adapter name. On a hit, recompute the full scalar digest, reject any inconsistent accelerated result, and preserve the existing UUID, answer, hash, time, and rate output.

- [ ] **Step 4: Verify CLI behavior**

Run scalar, AVX2, and wgpu with a bounded range that contains no required 33-bit hit:

```powershell
cargo run --release -- --backend scalar --max-index 1048576 --no-progress
cargo run --release -- --backend avx2 --max-index 1048576 --no-progress
cargo run --release -- --backend wgpu --max-index 1048576 --no-progress
```

Expected: each backend reports the same completed range without an invalid hit.

### Task 6: Final verification and performance smoke tests

**Files:**
- Modify only files required by verification findings.

- [ ] **Step 1: Format and lint**

Run:

```powershell
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
```

Expected: both commands exit successfully with no warnings.

- [ ] **Step 2: Run all release tests**

Run: `cargo test --release --all-targets`

Expected: all CPU and available Vulkan GPU tests pass.

- [ ] **Step 3: Run comparable throughput smoke tests**

Run each backend over at least `2^24` candidates with progress disabled and record processed candidates, elapsed time, and M/s. Do not compare debug builds.

- [ ] **Step 4: Inspect the final diff**

Run:

```powershell
git diff --check
git status --short
git diff --stat HEAD~1
```

Expected: no whitespace errors, no generated build artifacts, and only the planned source, manifest, lockfile, and documentation changes.
