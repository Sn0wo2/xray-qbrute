export const WASM_THREADS_ERROR =
  "WASM multithreading requires a cross-origin isolated page. Serve Frontend/ from localhost or HTTPS so coi-serviceworker.js can register, then reload after it activates."

export function artifactFlavor(backend, simdSupported) {
  return simdSupported || backend === "webgpu" || backend === "wasm-simd" ? "accelerated" : "scalar"
}

export function wasmThreadsAvailable(scope = globalThis) {
  return scope.crossOriginIsolated === true && typeof scope.SharedArrayBuffer === "function"
}
