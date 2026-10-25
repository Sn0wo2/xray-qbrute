use std::cell::Cell;
#[cfg(feature = "wasm-webgpu")]
use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::{JsCast, closure::Closure, prelude::*};
use wasm_bindgen_futures::spawn_local;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

#[cfg(feature = "wasm-webgpu")]
use super::gpu::GpuSession;
use super::protocol::{BackendRequest, CoordinatorCommand, CpuKernel, SearchEvent, SearchRequest};
use super::search_runner::now;

pub(super) struct Controller {
    scope: DedicatedWorkerGlobalScope,
    cancelled: Rc<Cell<bool>>,
    debug_enabled: Rc<Cell<bool>>,
    #[cfg(feature = "wasm-webgpu")]
    gpu_session: Rc<RefCell<Option<GpuCacheEntry>>>,
}

#[cfg(feature = "wasm-webgpu")]
#[derive(Clone, Copy, Eq, PartialEq)]
struct GpuKey {
    commit: u32,
    node_suffix: u32,
    difficulty: u32,
}

#[cfg(feature = "wasm-webgpu")]
type GpuCacheEntry = (GpuKey, Rc<GpuSession>);

#[wasm_bindgen]
pub fn install_coordinator() -> Result<(), JsValue> {
    let scope: DedicatedWorkerGlobalScope = js_sys::global().dyn_into()?;
    let controller = Rc::new(Controller {
        scope: scope.clone(),
        cancelled: Rc::new(Cell::new(false)),
        debug_enabled: Rc::new(Cell::new(false)),
        #[cfg(feature = "wasm-webgpu")]
        gpu_session: Rc::new(RefCell::new(None)),
    });
    let on_message = Closure::wrap(Box::new(move |event: MessageEvent| {
        let command = serde_wasm_bindgen::from_value::<CoordinatorCommand>(event.data());
        match command {
            Ok(CoordinatorCommand::Stop) => {
                controller.debug("stop requested");
                controller.cancelled.set(true);
            }
            Ok(CoordinatorCommand::Start { request }) => {
                controller.debug_enabled.set(request.debug);
                controller.debug(&format!(
                    "start backend={:?} threads={} max_index={} difficulty={}",
                    request.backend, request.threads, request.max_index, request.difficulty
                ));
                controller.cancelled.set(false);
                let controller = Rc::clone(&controller);
                spawn_local(async move {
                    if let Err(message) = controller.run(request).await {
                        controller.debug(&format!("search failed: {message}"));
                        controller.post(&SearchEvent::Error { message });
                    }
                });
            }
            Err(error) => controller.post(&SearchEvent::Error {
                message: format!("invalid coordinator command: {error}"),
            }),
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    scope.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
    let ready = serde_wasm_bindgen::to_value(&SearchEvent::Ready)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    scope.post_message(&ready)?;
    Ok(())
}

impl Controller {
    async fn run(&self, request: SearchRequest) -> Result<(), String> {
        if request.max_index == 0 || request.max_index > crate::candidate::MAX_UUID_INDEX {
            return Err("maximum index must be between 1 and 2^58".to_owned());
        }
        if request.difficulty > 64 {
            return Err("difficulty must be between 0 and 64".to_owned());
        }
        if request.threads == 0 || request.threads > 256 {
            return Err("threads must be between 1 and 256".to_owned());
        }
        let mut fallbacks = Vec::new();
        let backend = match request.backend {
            #[cfg(feature = "wasm-webgpu")]
            BackendRequest::Auto => match self.prepare_gpu(request).await {
                Ok(_) => BackendRequest::Webgpu,
                Err(error) => {
                    self.debug(&format!("WebGPU prepare fallback: {error}"));
                    fallbacks.push(format!("webgpu: {error}"));
                    BackendRequest::WasmSimd
                }
            },
            #[cfg(not(feature = "wasm-webgpu"))]
            BackendRequest::Auto => {
                fallbacks.push(format!(
                    "accelerated WASM: {}",
                    if request.accelerated_load_failed {
                        "module loading or SIMD validation failed"
                    } else {
                        "unavailable in this browser"
                    }
                ));
                BackendRequest::Scalar
            }
            #[cfg(feature = "wasm-webgpu")]
            BackendRequest::Webgpu => {
                self.prepare_gpu(request).await?;
                BackendRequest::Webgpu
            }
            #[cfg(not(feature = "wasm-webgpu"))]
            BackendRequest::Webgpu | BackendRequest::WasmSimd => {
                return Err("accelerated WASM is unavailable in this browser".to_owned());
            }
            #[cfg(feature = "wasm-webgpu")]
            BackendRequest::WasmSimd => BackendRequest::WasmSimd,
            BackendRequest::Scalar => BackendRequest::Scalar,
        };

        match backend {
            #[cfg(feature = "wasm-webgpu")]
            BackendRequest::Webgpu => {
                let session = self.prepare_gpu(request).await?;
                self.post(&SearchEvent::Device {
                    name: session.adapter_name().to_owned(),
                });
            }
            BackendRequest::WasmSimd | BackendRequest::Scalar => {
                self.post(&SearchEvent::Device {
                    name: format!("WASM Rayon · {} threads", request.threads),
                });
            }
            BackendRequest::Auto => unreachable!("auto is resolved before execution"),
            #[cfg(not(feature = "wasm-webgpu"))]
            BackendRequest::Webgpu => unreachable!("WebGPU is rejected before execution"),
        }
        self.post(&SearchEvent::Prepared {
            backend: backend_name(backend),
            fallbacks: fallbacks.clone(),
        });
        self.debug(&format!("prepared backend={}", backend_name(backend)));

        let started = now(&self.scope);
        match backend {
            #[cfg(feature = "wasm-webgpu")]
            BackendRequest::Webgpu if matches!(request.backend, BackendRequest::Auto) => {
                match self.run_gpu(request, started, fallbacks.clone()).await {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        self.debug(&format!("WebGPU runtime fallback: {error}"));
                        fallbacks.push(format!("webgpu runtime: {error}"));
                        self.post(&SearchEvent::Device {
                            name: format!("WASM Rayon · {} threads", request.threads),
                        });
                        self.post(&SearchEvent::Prepared {
                            backend: "wasm-simd",
                            fallbacks: fallbacks.clone(),
                        });
                        self.run_cpu(request, CpuKernel::Simd, now(&self.scope), fallbacks)
                            .await
                    }
                }
            }
            #[cfg(feature = "wasm-webgpu")]
            BackendRequest::Webgpu => self.run_gpu(request, started, fallbacks).await,
            #[cfg(feature = "wasm-simd")]
            BackendRequest::WasmSimd => {
                self.run_cpu(request, CpuKernel::Simd, started, fallbacks)
                    .await
            }
            #[cfg(not(feature = "wasm-simd"))]
            BackendRequest::WasmSimd => {
                Err("WASM SIMD is unavailable in the scalar artifact".to_owned())
            }
            BackendRequest::Scalar => {
                self.run_cpu(request, CpuKernel::Scalar, started, fallbacks)
                    .await
            }
            BackendRequest::Auto => unreachable!("auto is resolved before execution"),
            #[cfg(not(feature = "wasm-webgpu"))]
            BackendRequest::Webgpu => unreachable!("WebGPU is rejected before execution"),
        }
    }

    #[cfg(feature = "wasm-webgpu")]
    pub(super) async fn prepare_gpu(
        &self,
        request: SearchRequest,
    ) -> Result<Rc<GpuSession>, String> {
        let key = GpuKey {
            commit: request.commit,
            node_suffix: request.node_suffix,
            difficulty: request.difficulty,
        };
        if let Some((cached_key, session)) = self.gpu_session.borrow().as_ref()
            && *cached_key == key
        {
            self.debug("reusing cached WebGPU session");
            return Ok(Rc::clone(session));
        }
        self.debug("creating WebGPU session");
        let session = Rc::new(GpuSession::new(request).await?);
        self.gpu_session.replace(Some((key, Rc::clone(&session))));
        Ok(session)
    }

    pub(super) fn post(&self, event: &SearchEvent) {
        if let Ok(value) = serde_wasm_bindgen::to_value(event) {
            let _ = self.scope.post_message(&value);
        }
    }

    pub(super) fn cancelled(&self) -> bool {
        self.cancelled.get()
    }

    pub(super) fn scope(&self) -> &DedicatedWorkerGlobalScope {
        &self.scope
    }

    pub(super) fn debug(&self, message: &str) {
        if self.debug_enabled.get() {
            web_sys::console::debug_1(&format!("[wasm] {message}").into());
        }
    }
}

fn backend_name(backend: BackendRequest) -> &'static str {
    match backend {
        BackendRequest::Auto => "auto",
        BackendRequest::Webgpu => "webgpu",
        BackendRequest::WasmSimd => "wasm-simd",
        BackendRequest::Scalar => "scalar",
    }
}
