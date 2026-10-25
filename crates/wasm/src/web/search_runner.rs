use rayon::prelude::*;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::DedicatedWorkerGlobalScope;

use super::controller::Controller;
#[cfg(feature = "wasm-webgpu")]
use super::gpu::GPU_BATCH_SIZE;
use super::protocol::{CpuKernel, SearchEvent, SearchRequest};
use super::result_details;

const CPU_BATCH_SIZE: u64 = 1 << 18;
const CPU_TASK_SIZE: u32 = 1 << 12;

impl Controller {
    pub async fn run_cpu(
        &self,
        request: SearchRequest,
        kernel: CpuKernel,
        started: f64,
        fallbacks: Vec<String>,
    ) -> Result<(), String> {
        self.debug(&format!("CPU search begin kernel={kernel:?}"));
        let mut base = 0u64;
        while base < request.max_index {
            if self.cancelled() {
                self.debug(&format!("CPU search stopped processed={base}"));
                self.finish(request, "stopped", None, base, base, started, fallbacks);
                return Ok(());
            }
            let count = (request.max_index - base).min(CPU_BATCH_SIZE) as u32;
            let task_count = count.div_ceil(CPU_TASK_SIZE);
            let hit = (0..task_count)
                .into_par_iter()
                .filter_map(|task| {
                    let offset = task * CPU_TASK_SIZE;
                    let task_count = CPU_TASK_SIZE.min(count - offset);
                    let start = base + u64::from(offset);
                    let hit = match kernel {
                        CpuKernel::Scalar => super::search_scalar_batch(
                            request.commit,
                            request.node_suffix,
                            start,
                            task_count,
                            request.difficulty,
                        ),
                        #[cfg(all(feature = "wasm-simd", target_family = "wasm"))]
                        CpuKernel::Simd => super::search_simd_batch(
                            request.commit,
                            request.node_suffix,
                            start,
                            task_count,
                            request.difficulty,
                        ),
                        #[cfg(not(all(feature = "wasm-simd", target_family = "wasm")))]
                        CpuKernel::Simd => return None,
                    };
                    (hit != u64::MAX).then_some(hit)
                })
                .min()
                .unwrap_or(u64::MAX);
            let evaluated = base + u64::from(count);
            if hit != u64::MAX {
                self.debug(&format!("CPU hit index={hit} evaluated={evaluated}"));
                self.finish(
                    request,
                    "found",
                    Some(hit),
                    hit + 1,
                    evaluated,
                    started,
                    fallbacks,
                );
                return Ok(());
            }
            base = evaluated;
            self.debug(&format!("CPU batch complete processed={base}"));
            self.progress(base, base, request.max_index, started);
            yield_to_event_loop(self.scope()).await?;
        }
        self.finish(
            request,
            "exhausted",
            None,
            request.max_index,
            request.max_index,
            started,
            fallbacks,
        );
        self.debug("CPU range exhausted");
        Ok(())
    }

    #[cfg(feature = "wasm-webgpu")]
    pub async fn run_gpu(
        &self,
        request: SearchRequest,
        started: f64,
        fallbacks: Vec<String>,
    ) -> Result<(), String> {
        self.debug("WebGPU search begin");
        let session = self.prepare_gpu(request).await?;
        let mut base = 0u64;
        let mut evaluated = 0u64;
        while base < request.max_index {
            if self.cancelled() {
                self.debug(&format!(
                    "WebGPU search stopped processed={base} evaluated={evaluated}"
                ));
                self.finish(
                    request, "stopped", None, base, evaluated, started, fallbacks,
                );
                return Ok(());
            }
            let count = (request.max_index - base).min(GPU_BATCH_SIZE) as u32;
            let offset = session.dispatch(base, count).await?;
            evaluated += u64::from(count);
            self.debug(&format!(
                "WebGPU batch base={base} count={count} result_offset={offset}"
            ));
            if offset != u32::MAX {
                let index = base + u64::from(offset);
                self.finish(
                    request,
                    "found",
                    Some(index),
                    index + 1,
                    evaluated,
                    started,
                    fallbacks,
                );
                return Ok(());
            }
            base += u64::from(count);
            self.progress(base, evaluated, request.max_index, started);
        }
        self.finish(
            request,
            "exhausted",
            None,
            request.max_index,
            evaluated,
            started,
            fallbacks,
        );
        self.debug("WebGPU range exhausted");
        Ok(())
    }

    fn progress(&self, processed: u64, evaluated: u64, max_index: u64, started: f64) {
        self.post(&SearchEvent::Progress {
            processed: processed.to_string(),
            evaluated: evaluated.to_string(),
            max_index: max_index.to_string(),
            elapsed_ms: now(self.scope()) - started,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        &self,
        request: SearchRequest,
        kind: &'static str,
        index: Option<u64>,
        processed: u64,
        evaluated: u64,
        started: f64,
        fallbacks: Vec<String>,
    ) {
        self.debug(&format!(
            "finish kind={kind} index={index:?} processed={processed} evaluated={evaluated}"
        ));
        let (uuid, hash) = index
            .map(|index| result_details(request.commit, request.node_suffix, index))
            .map_or((None, None), |(uuid, hash)| (Some(uuid), Some(hash)));
        self.post(&SearchEvent::Done {
            kind,
            index: index.map(|value| value.to_string()),
            processed: processed.to_string(),
            evaluated: evaluated.to_string(),
            elapsed_ms: now(self.scope()) - started,
            uuid,
            hash,
            fallbacks,
        });
    }
}

async fn yield_to_event_loop(scope: &DedicatedWorkerGlobalScope) -> Result<(), String> {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let callback = Closure::once(move || {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        });
        let _ = scope.set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            0,
        );
        callback.forget();
    });
    JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|error| format!("event-loop yield failed: {error:?}"))
}

pub fn now(scope: &DedicatedWorkerGlobalScope) -> f64 {
    scope
        .performance()
        .map_or_else(js_sys::Date::now, |value| value.now())
}
