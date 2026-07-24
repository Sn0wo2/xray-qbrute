# AVX2 + wgpu Vulkan 加速实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

目标：在保持现有 16 字节 UUID 二进制 SHA-512 语义不变的前提下，新增 AVX2 四路并行 CPU 后端和 wgpu + WGSL + Vulkan GPU 后端。

架构：将候选 UUID 编码、固定 SHA-512 block 构造和标量参考实现从 main.rs 提取到库模块。CPU 后端按 Rayon 大块搜索，AVX2 每批处理 4 个独立候选；GPU 后端由 WGSL shader 直接从索引生成消息，只回传命中索引。所有加速命中都由标量 sha2 完整复算确认。

技术栈：Rust 2024、sha2 0.11、Rayon、std::arch AVX2、wgpu 30.0.0 Vulkan backend、WGSL u64、pollster、bytemuck、Clap、Indicatif。

约束：不引入 CUDA、OpenCL 或 DirectX 12；默认搜索空间仍为 2^58；当前命令行参数及默认值保持兼容；没有 AVX2 或 Vulkan 时必须可回退到标量后端。

---

## 当前执行状态（2026-07-24）

本计划已恢复并执行完成。用户已明确要求直接在 `main` 分支修改，不创建 worktree 或新分支。

- 已完成：共享候选编码、标量参考实现、AVX2 四路 SHA-512、wgpu + WGSL + Vulkan 后端、统一后端选择和 CLI 参数。
- 已验证：`cargo fmt -- --check`、`cargo clippy --all-targets -- -D warnings`、release 全量测试（10/10）。
- Vulkan 测试设备：NVIDIA GeForce RTX 4060 Laptop GPU；WGSL SHA-512 与标量摘要逐项一致，GPU 搜索命中通过 CPU 复算。
- `2^24` release 烟测：scalar 67.8 M/s、AVX2-4x 96.0 M/s、wgpu-vulkan 40.9 M/s；三者均处理 16,777,216 个候选且未命中。

执行时保留当前已有修改；不使用破坏性 Git 命令覆盖工作区。

## Task 0：恢复工具链并建立基线

Files:
- No source changes.

- [x] Step 1: 确认本地工具链状态。

使用固定路径执行：
    $env:CARGO_HOME = 'C:\Users\Xiao Qing\AppData\Local\Codex\toolchains\xray-qbrute-rust\cargo'
    $env:RUSTUP_HOME = 'C:\Users\Xiao Qing\AppData\Local\Codex\toolchains\xray-qbrute-rust\rustup'
    & "$env:CARGO_HOME\bin\rustup.exe" toolchain list

期望 stable-x86_64-pc-windows-msvc 可用，并且对应 bin\rustc.exe、bin\cargo.exe 存在。

- [x] Step 2: 若工具链不完整，使用 minimal profile 重新安装。

安装器必须命名为 rustup-init.exe，避免 rustup 将自定义文件名误识别成代理命令；安装参数为 --no-modify-path --profile minimal --default-toolchain stable。安装目录只使用上述 CARGO_HOME 与 RUSTUP_HOME，不修改系统 PATH。

- [x] Step 3: 运行原始基线检查。

执行 cargo test --release --all-targets 和 cargo fmt -- --check。期望原始项目编译并通过；如果基线失败，先记录具体错误再修改源代码。

## Task 1：提取候选编码和标量参考实现

Files:
- Modify: Cargo.toml, src/main.rs.
- Create: src/lib.rs, src/candidate.rs, src/backend/mod.rs, src/backend/scalar.rs.

- [x] Step 1: 添加候选编码回归测试。

测试索引 0、0xffff、1 << 16、1 << 28、1 << 30、1 << 42、(1 << 58) - 1。验证 candidate_bytes 与旧 make_uuid_bytes 的 16 字节结果一致，并验证两个大端 u64 block word 拼接后等于这 16 字节。

索引 0、默认参数的期望字节：
    eb 36 68 95 00 00 40 00 80 00 00 00 eb ac 62 b9

- [x] Step 2: 实现共享候选 API。

candidate.rs 提供 MAX_UUID_INDEX、DEFAULT_DIFFICULTY_BITS、CandidateConfig、candidate_bytes、candidate_words、digest、digest_word、matches_leading_zero_bits 和 bytes_to_uuid_string。

固定 SHA-512 block 必须使用：
    W0 = commit << 32 | time_mid << 16 | 0x4000 | time_high
    W1 = variant << 48 | node_prefix << 32 | node_suffix
    W2 = 0x8000000000000000
    W3..W14 = 0
    W15 = 128

- [x] Step 3: 添加统一搜索类型。

backend/mod.rs 定义 SearchHit { index: u64 }、BackendOutcome { hit, processed, backend_name } 和 SearchError { Unavailable, InvalidConfig, Runtime }。

- [x] Step 4: 实现标量参考搜索。

scalar.rs 按 65,536 个索引分块，用 Rayon 并行处理。每个候选调用 candidate::digest；命中返回索引；计数只在 block 结束时更新一次，不得在每候选执行全局原子 RMW。

- [x] Step 5: 运行共享核心测试。

执行 cargo test --release --lib candidate backend::scalar。期望候选边界、block word、摘要条件和标量搜索测试全部通过。

## Task 2：实现 AVX2 四路 SHA-512

Files:
- Create: src/backend/avx2.rs.
- Modify: src/backend/mod.rs, src/candidate.rs.

- [x] Step 1: 先写 AVX2 与标量等价测试。

生成 1,024 组固定四索引批次，比较 AVX2 返回的四个首摘要 word 与四次 candidate::digest 结果。非 x86/x86_64 或不支持 AVX2 时标记跳过。

- [x] Step 2: 实现固定单 block 的 AVX2 核心。

使用 __m256i 表示四个 u64 lane，每个 lane 对应一个独立索引。状态 a..h 使用 SHA-512 IV 广播初始化；消息 schedule 使用 16 项循环数组；W0/W1 从四个索引直接生成，W2..W15 使用固定 padding。

旋转使用 AVX2 常量移位和 OR，不使用运行时变量移位。完成 80 轮后只累加第一个 IV word，比较 h0 >> 31 == 0，命中 lane 再提取索引；完整摘要由标量路径复算。

- [x] Step 3: 添加运行时特性检查和尾部处理。

提供 is_available 和 search。连续四个索引走 AVX2，最后 1-3 个索引走标量。显式请求 AVX2 但 CPU 不支持时返回 SearchError::Unavailable；auto 才允许回退。

- [x] Step 4: 验证 AVX2。

执行 cargo test --release backend::avx2 -- --nocapture。期望所有四路输出与标量摘要一致，边界索引不会越界。

## Task 3：实现 WGSL SHA-512 内核

Files:
- Create: src/shaders/sha512_search.wgsl, src/backend/wgpu.rs.
- Modify: Cargo.toml.

- [x] Step 1: 添加 GPU 依赖。

Cargo.toml 使用 wgpu = { version = "30.0.0", default-features = false, features = ["std", "vulkan", "wgsl"] }、pollster = "1.0.1" 和 bytemuck = { version = "1.25", features = ["derive"] }。删除入口未使用的 itertools、uuid、rand，保留 sha2 作为参考验证实现。

- [x] Step 2: 实现 WGSL 候选和摘要函数。

shader 启用 wgpu_int64，实现 candidate_words(index, commit, suffix) -> vec2<u64> 和 sha512_first_word(index, commit, suffix) -> u64。使用标准 SHA-512 IV、80 个 round constants、16 项循环 schedule；每个 invocation 使用独立的 64 位状态。

- [x] Step 3: 实现 GPU hash-equivalence 测试入口。

测试入口对连续 256 个索引输出第一个摘要 word。Rust 主机创建 storage、readback 和参数 buffer，执行一个 256-thread workgroup，映射读回结果，并与标量摘要逐项比较。

- [x] Step 4: 验证 shader 编译和摘要结果。

执行 cargo test --release backend::wgpu::tests::gpu_hashes_match_scalar -- --nocapture。期望选择 NVIDIA RTX 4060 Vulkan adapter，SHADER_INT64 可用，256 个摘要 word 全部一致。

## Task 4：实现 wgpu Vulkan 批量搜索

Files:
- Modify: src/backend/wgpu.rs, src/shaders/sha512_search.wgsl.

- [x] Step 1: 创建 Vulkan adapter 和 compute pipeline。

使用 wgpu::Backends::VULKAN，选择 PowerPreference::HighPerformance，过滤 fallback adapter，并要求 Features::SHADER_INT64。记录 adapter 名称；显式 wgpu 初始化失败返回 SearchError::Unavailable。

- [x] Step 2: 实现结果 buffer。

结果结构使用一个 atomic<u32> 状态和两个 u32 索引 word；命中 invocation 用 atomicCompareExchangeWeak 选出唯一 writer，然后写入索引，无需 64 位原子。

- [x] Step 3: 实现批次 dispatch。

每个 invocation 处理 32 个连续候选；单批最多处理 2^28 个索引。每批清零 result buffer、dispatch、copy 到 mapping buffer、等待完成、读取结果。GPU 不传输候选数组或摘要数组。

- [x] Step 4: 添加低难度端到端测试。

内部测试使用 8 个前导零 bit，在前 65,536 个索引内验证 GPU 返回值满足标量摘要条件；不要求 GPU 与 CPU 以相同顺序命中。

- [x] Step 5: 验证 GPU 搜索。

执行 cargo test --release backend::wgpu::tests::gpu_search_finds_valid_hit -- --nocapture。期望返回索引在搜索范围内，并通过 CPU 完整 SHA-512 复算。

## Task 5：统一搜索 API 和 CLI

Files:
- Create: src/search.rs.
- Modify: src/lib.rs.
- Replace: src/main.rs.

- [x] Step 1: 添加后端枚举和配置。

定义 BackendKind { Auto, Scalar, Avx2, Wgpu }、SearchConfig { candidate, max_index, leading_zero_bits }，并提供 search(config, backend, progress) -> Result<BackendOutcome, SearchError>。

- [x] Step 2: 实现自动选择顺序。

auto 按 wgpu -> avx2 -> scalar 尝试；只在初始化阶段失败时回退。显式 scalar、avx2、wgpu 不得静默换后端。

- [x] Step 3: 更新 CLI。

保留 --node-suffix 和 --commit-last8，新增 --backend、--max-index、--no-progress。验证十六进制参数长度和 max_index <= 2^58。命中后用 candidate::digest 完整复算，再保留原有 UUID、/answer、hash[:10]、耗时和速率输出。

- [x] Step 4: 验证 CLI 后端行为。

分别执行：
    cargo run --release -- --backend scalar --max-index 1048576 --no-progress
    cargo run --release -- --backend avx2 --max-index 1048576 --no-progress
    cargo run --release -- --backend wgpu --max-index 1048576 --no-progress

期望三个后端完成相同范围；GPU 输出 adapter 名称；AVX2 不可用时显式模式报错、auto 模式回退。

## Task 6：最终验证和性能烟测

Files:
- Modify only files required by failed verification.

- [x] Step 1: 运行 cargo fmt -- --check 和 cargo clippy --all-targets -- -D warnings。期望退出码均为 0，没有 warning。

- [x] Step 2: 运行 cargo test --release --all-targets。期望标量、AVX2 和可用 Vulkan GPU 测试全部通过。

- [x] Step 3: 三个后端分别运行至少 2^24 个候选，关闭进度条，记录 processed、elapsed 和 M/s。只比较 release 构建，不在没有实测数据时宣称加速倍数。

- [x] Step 4: 运行 git diff --check、git status --short 和 git diff --stat HEAD~1。期望无空白错误、没有 target 或临时工具文件进入 Git，变更只包含源代码、manifest、lockfile 和设计/计划文档。
