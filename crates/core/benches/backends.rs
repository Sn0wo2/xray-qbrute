use std::hint::black_box;
use std::sync::Arc;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
#[cfg(any(feature = "avx2", feature = "wgpu", feature = "cuda"))]
use xray_qbrute_core::backend;
use xray_qbrute_core::backend::{ProgressReporter, SearchConfig};
use xray_qbrute_core::candidate::CandidateConfig;
use xray_qbrute_core::search::{self, BackendKind};

const CANDIDATE_COUNT: u64 = 1 << 20;
const CONFIG: SearchConfig = SearchConfig {
    candidate: CandidateConfig {
        commit: 0xeb366895,
        node_suffix: 0xebac62b9,
    },
    start_index: 0,
    max_index: CANDIDATE_COUNT,
    leading_zero_bits: 64,
};

fn no_progress() -> ProgressReporter {
    Arc::new(|_| {})
}

#[cfg(feature = "avx2")]
fn cpu_backend_available(backend: BackendKind) -> bool {
    if backend == BackendKind::Avx2 {
        return backend::avx2::is_available();
    }
    true
}

fn bench_cpu_backends(c: &mut Criterion) {
    #[cfg(feature = "scalar")]
    bench_one_cpu(c, "scalar", BackendKind::Scalar);
    #[cfg(feature = "avx2")]
    bench_one_cpu(c, "avx2", BackendKind::Avx2);
}

fn bench_one_cpu(c: &mut Criterion, name: &str, backend: BackendKind) {
    let mut group = c.benchmark_group("cpu-backends");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(3));
    group.warm_up_time(Duration::from_secs(1));
    group.throughput(Throughput::Elements(CANDIDATE_COUNT));

    #[cfg(feature = "avx2")]
    if !cpu_backend_available(backend) {
        eprintln!("Skipping AVX2 benchmark: AVX2 is unavailable");
        return;
    }

    let progress = no_progress();
    let prepared =
        search::prepare(&CONFIG, backend).expect("selected CPU backend must be available");
    prepared
        .search(&CONFIG, &progress)
        .expect("CPU warm-up search must succeed");
    group.bench_with_input(
        BenchmarkId::new(name, CANDIDATE_COUNT),
        &CONFIG,
        |b, config| {
            b.iter(|| {
                let outcome = prepared
                    .search(black_box(config), &progress)
                    .expect("selected CPU backend must be available");
                let expected_name = match backend {
                    #[cfg(feature = "scalar")]
                    BackendKind::Scalar => "scalar",
                    #[cfg(feature = "avx2")]
                    BackendKind::Avx2 => "avx2-4x",
                    _ => unreachable!("CPU benchmark only selects CPU backends"),
                };
                debug_assert_eq!(outcome.backend_name, expected_name);
                black_box((outcome.processed, outcome.evaluated, outcome.hit));
            });
        },
    );
    group.finish();
}

fn bench_wgpu(c: &mut Criterion) {
    #[cfg(feature = "wgpu")]
    {
        #[cfg(target_os = "macos")]
        let backend = BackendKind::WgpuMetal;
        #[cfg(not(target_os = "macos"))]
        let backend = BackendKind::WgpuVulkan;

        let prepared = match search::prepare(&CONFIG, backend) {
            Ok(prepared) => prepared,
            Err(error) => {
                eprintln!("Skipping WGPU benchmark: {error}");
                return;
            }
        };
        let progress = no_progress();
        if let Err(error) = prepared.search(&CONFIG, &progress) {
            eprintln!("Skipping WGPU benchmark: {error}");
            return;
        }

        let mut group = c.benchmark_group("gpu-backends");
        group.sample_size(10);
        group.measurement_time(Duration::from_secs(5));
        group.warm_up_time(Duration::from_secs(1));
        group.bench_function("wgpu-session-init", |b| {
            b.iter(|| {
                let selected = match backend {
                    BackendKind::WgpuVulkan => backend::wgpu::WgpuBackend::Vulkan,
                    BackendKind::WgpuMetal => backend::wgpu::WgpuBackend::Metal,
                    _ => unreachable!("GPU benchmark selects a WGPU backend"),
                };
                black_box(
                    backend::wgpu::WgpuSearchSession::new(selected, CONFIG)
                        .expect("WGPU preflight succeeded"),
                );
            });
        });
        group.throughput(Throughput::Elements(CANDIDATE_COUNT));
        group.bench_function(BenchmarkId::new("wgpu-warm-search", CANDIDATE_COUNT), |b| {
            b.iter(|| {
                let outcome = prepared
                    .search(black_box(&CONFIG), &progress)
                    .expect("WGPU preflight succeeded");
                black_box((outcome.processed, outcome.evaluated, outcome.hit));
            });
        });
        group.finish();
    }
}

fn bench_cuda(c: &mut Criterion) {
    #[cfg(feature = "cuda")]
    {
        let prepared = match search::prepare(&CONFIG, BackendKind::Cuda) {
            Ok(prepared) => prepared,
            Err(error) => {
                eprintln!("Skipping CUDA benchmark: {error}");
                return;
            }
        };
        let progress = no_progress();
        if let Err(error) = prepared.search(&CONFIG, &progress) {
            eprintln!("Skipping CUDA benchmark: {error}");
            return;
        }

        let mut group = c.benchmark_group("gpu-backends");
        group.sample_size(10);
        group.measurement_time(Duration::from_secs(5));
        group.warm_up_time(Duration::from_secs(1));
        group.bench_function("cuda-session-init", |b| {
            b.iter(|| {
                black_box(
                    backend::cuda::CudaSearchSession::new(CONFIG)
                        .expect("CUDA preflight succeeded"),
                );
            });
        });
        group.throughput(Throughput::Elements(CANDIDATE_COUNT));
        group.bench_function(BenchmarkId::new("cuda-warm-search", CANDIDATE_COUNT), |b| {
            b.iter(|| {
                let outcome = prepared
                    .search(black_box(&CONFIG), &progress)
                    .expect("CUDA preflight succeeded");
                black_box((outcome.processed, outcome.evaluated, outcome.hit));
            });
        });
        group.finish();
    }
}

criterion_group!(benches, bench_cpu_backends, bench_wgpu, bench_cuda);
criterion_main!(benches);
