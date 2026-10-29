use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum BackendRequest {
    Auto,
    Webgpu,
    WasmSimd,
    Scalar,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchRequest {
    pub commit: u32,
    pub node_suffix: u32,
    pub difficulty: u32,
    pub max_index: u64,
    pub backend: BackendRequest,
    pub threads: u32,
    #[serde(default)]
    pub debug: bool,
    #[serde(default)]
    pub accelerated_load_failed: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(super) enum CoordinatorCommand {
    Start { request: SearchRequest },
    Stop,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum CpuKernel {
    Scalar,
    Simd,
}

#[derive(Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum SearchEvent {
    Ready,
    Prepared {
        backend: &'static str,
        fallbacks: Vec<String>,
    },
    Device {
        name: String,
    },
    Progress {
        processed: String,
        evaluated: String,
        max_index: String,
        elapsed_ms: f64,
    },
    Done {
        kind: &'static str,
        index: Option<String>,
        processed: String,
        evaluated: String,
        elapsed_ms: f64,
        uuid: Option<String>,
        hash: Option<String>,
        fallbacks: Vec<String>,
    },
    Error {
        message: String,
    },
}
