use clap::ValueEnum;

use crate::backend::{self, BackendOutcome, ProgressReporter, SearchConfig, SearchError};
use crate::candidate::MAX_UUID_INDEX;

#[cfg(not(any(
    feature = "scalar",
    feature = "avx2",
    feature = "wgpu",
    feature = "cuda"
)))]
compile_error!(
    "xray-qbrute-core requires at least one backend feature enabled:      scalar, avx2, wgpu, or cuda"
);

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum BackendKind {
    Auto,
    #[cfg(feature = "scalar")]
    Scalar,
    #[cfg(feature = "avx2")]
    Avx2,
    #[cfg(feature = "wgpu")]
    WgpuVulkan,
    #[cfg(feature = "wgpu")]
    WgpuMetal,
    #[cfg(feature = "cuda")]
    Cuda,
}

enum PreparedBackend {
    #[cfg(feature = "scalar")]
    Scalar,
    #[cfg(feature = "avx2")]
    Avx2,
    #[cfg(feature = "wgpu")]
    Wgpu {
        kind: BackendKind,
        session: Box<backend::wgpu::WgpuSearchSession>,
    },
    #[cfg(feature = "cuda")]
    Cuda {
        session: Box<backend::cuda::CudaSearchSession>,
    },
}

pub struct PreparedSearch {
    backends: Vec<PreparedBackend>,
    fallback_reasons: Vec<String>,
}

impl std::fmt::Display for BackendKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Auto => "auto",
            #[cfg(feature = "scalar")]
            Self::Scalar => "scalar",
            #[cfg(feature = "avx2")]
            Self::Avx2 => "avx2",
            #[cfg(feature = "wgpu")]
            Self::WgpuVulkan => "wgpu-vulkan",
            #[cfg(feature = "wgpu")]
            Self::WgpuMetal => "wgpu-metal",
            #[cfg(feature = "cuda")]
            Self::Cuda => "cuda",
        })
    }
}

fn auto_backends() -> Vec<BackendKind> {
    [
        #[cfg(all(any(target_os = "windows", target_os = "linux"), feature = "cuda"))]
        BackendKind::Cuda,
        #[cfg(all(target_os = "macos", feature = "wgpu"))]
        BackendKind::WgpuMetal,
        #[cfg(all(not(target_os = "macos"), feature = "wgpu"))]
        BackendKind::WgpuVulkan,
        #[cfg(feature = "avx2")]
        BackendKind::Avx2,
        #[cfg(feature = "scalar")]
        BackendKind::Scalar,
    ]
    .into_iter()
    .collect()
}

pub fn search(
    config: &SearchConfig,
    backend: BackendKind,
    progress: ProgressReporter,
) -> Result<BackendOutcome, SearchError> {
    prepare(config, backend)?.search(config, &progress)
}

pub fn prepare(
    config: &SearchConfig,
    requested: BackendKind,
) -> Result<PreparedSearch, SearchError> {
    validate_config(config)?;

    if requested == BackendKind::Auto {
        return prepare_auto(config);
    }

    Ok(PreparedSearch {
        backends: vec![prepare_backend(config, requested)?],
        fallback_reasons: Vec::new(),
    })
}

impl PreparedSearch {
    pub fn search(
        &self,
        config: &SearchConfig,
        progress: &ProgressReporter,
    ) -> Result<BackendOutcome, SearchError> {
        validate_config(config)?;
        let mut fallback_reasons = self.fallback_reasons.clone();

        for (position, backend) in self.backends.iter().enumerate() {
            let outcome = match backend {
                #[cfg(feature = "scalar")]
                PreparedBackend::Scalar => Ok(backend::scalar::search(config, progress)),
                #[cfg(feature = "avx2")]
                PreparedBackend::Avx2 => backend::avx2::search(config, progress),
                #[cfg(feature = "wgpu")]
                PreparedBackend::Wgpu { session, .. } => session.search(config, progress),
                #[cfg(feature = "cuda")]
                PreparedBackend::Cuda { session, .. } => session.search(config, progress),
            };

            match outcome {
                Ok(mut outcome) => {
                    outcome.fallback_reasons = fallback_reasons;
                    return Ok(outcome);
                }
                Err(SearchError::Unavailable(reason)) if position + 1 < self.backends.len() => {
                    fallback_reasons.push(format!("{}: {reason}", backend.kind()));
                }
                Err(error) => return Err(error),
            }
        }

        unreachable!("prepared search always contains at least one backend")
    }
}

impl PreparedBackend {
    const fn kind(&self) -> BackendKind {
        match self {
            #[cfg(feature = "scalar")]
            Self::Scalar => BackendKind::Scalar,
            #[cfg(feature = "avx2")]
            Self::Avx2 => BackendKind::Avx2,
            #[cfg(feature = "wgpu")]
            Self::Wgpu { kind, .. } => *kind,
            #[cfg(feature = "cuda")]
            Self::Cuda { .. } => BackendKind::Cuda,
        }
    }
}

fn prepare_auto(config: &SearchConfig) -> Result<PreparedSearch, SearchError> {
    let mut fallback_reasons = Vec::new();
    let mut backends = Vec::new();
    for backend in auto_backends() {
        match prepare_backend(config, backend) {
            Ok(prepared) => backends.push(prepared),
            Err(SearchError::Unavailable(reason)) => {
                fallback_reasons.push(format!("{backend}: {reason}"));
            }
            Err(error) => return Err(error),
        }
    }

    if backends.is_empty() {
        return Err(SearchError::Runtime(
            "auto backend selection exhausted without a usable backend".to_owned(),
        ));
    }

    Ok(PreparedSearch {
        backends,
        fallback_reasons,
    })
}

fn prepare_backend(
    config: &SearchConfig,
    backend: BackendKind,
) -> Result<PreparedBackend, SearchError> {
    match backend {
        BackendKind::Auto => unreachable!("auto is expanded before backend preparation"),
        #[cfg(feature = "scalar")]
        BackendKind::Scalar => {
            warm_up_cpu(config, BackendKind::Scalar)?;
            Ok(PreparedBackend::Scalar)
        }
        #[cfg(feature = "avx2")]
        BackendKind::Avx2 => {
            if !backend::avx2::is_available() {
                return Err(SearchError::Unavailable(
                    "AVX2 is not supported by this CPU".to_owned(),
                ));
            }
            warm_up_cpu(config, BackendKind::Avx2)?;
            Ok(PreparedBackend::Avx2)
        }
        #[cfg(feature = "wgpu")]
        BackendKind::WgpuVulkan | BackendKind::WgpuMetal => {
            let selected = match backend {
                BackendKind::WgpuVulkan => backend::wgpu::WgpuBackend::Vulkan,
                BackendKind::WgpuMetal => backend::wgpu::WgpuBackend::Metal,
                _ => unreachable!("matched WGPU backend"),
            };
            let session = backend::wgpu::WgpuSearchSession::new(selected, *config)?;
            session.warm_up()?;
            Ok(PreparedBackend::Wgpu {
                kind: backend,
                session: Box::new(session),
            })
        }
        #[cfg(feature = "cuda")]
        BackendKind::Cuda => {
            if !backend::cuda::is_available() {
                return Err(SearchError::Unavailable(
                    "CUDA is not available (no NVIDIA GPU or driver)".to_owned(),
                ));
            }
            let session = backend::cuda::CudaSearchSession::new(*config)?;
            session.warm_up()?;
            Ok(PreparedBackend::Cuda {
                session: Box::new(session),
            })
        }
    }
}

#[cfg(any(feature = "scalar", feature = "avx2"))]
fn warm_up_cpu(config: &SearchConfig, backend: BackendKind) -> Result<(), SearchError> {
    let warm_up_config = SearchConfig {
        candidate: config.candidate,
        start_index: 0,
        max_index: backend::SEARCH_CHUNK_SIZE,
        leading_zero_bits: 64,
    };
    let progress: ProgressReporter = std::sync::Arc::new(|_| {});

    match backend {
        #[cfg(feature = "scalar")]
        BackendKind::Scalar => {
            backend::scalar::search(&warm_up_config, &progress);
            Ok(())
        }
        #[cfg(feature = "avx2")]
        BackendKind::Avx2 => backend::avx2::search(&warm_up_config, &progress).map(|_| ()),
        _ => unreachable!("CPU warm-up only accepts CPU backends"),
    }
}

fn validate_config(config: &SearchConfig) -> Result<(), SearchError> {
    if config.start_index > config.max_index {
        return Err(SearchError::InvalidConfig(
            "start_index must not exceed max_index".to_owned(),
        ));
    }
    if config.max_index == 0 {
        return Err(SearchError::InvalidConfig(
            "max_index must be greater than zero".to_owned(),
        ));
    }
    if config.max_index > MAX_UUID_INDEX {
        return Err(SearchError::InvalidConfig(format!(
            "max_index must be at most {MAX_UUID_INDEX}"
        )));
    }
    if config.leading_zero_bits > 64 {
        return Err(SearchError::InvalidConfig(
            "leading_zero_bits must be between 0 and 64".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod test;
