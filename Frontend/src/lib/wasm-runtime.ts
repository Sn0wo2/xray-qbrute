const SIMD128_PROBE = new Uint8Array([
  0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253, 15, 253, 98, 11,
])

export const WASM_THREADS_ERROR =
  "WASM multithreading requires a cross-origin isolated page. Serve Frontend/ from localhost or HTTPS so coi-serviceworker.js can register, then reload after it activates."

export function supportsSimd128(): boolean {
  return typeof WebAssembly === "object" && WebAssembly.validate(SIMD128_PROBE)
}

export type ArtifactFlavor = "accelerated" | "scalar"

export function artifactFlavor(backend: string, simdSupported: boolean): ArtifactFlavor {
  if (backend === "scalar" || backend === "auto") {
    return simdSupported ? "accelerated" : "scalar"
  }
  return "accelerated"
}

export function wasmThreadsAvailable(scope: typeof globalThis = globalThis): boolean {
  return scope.crossOriginIsolated === true && typeof scope.SharedArrayBuffer === "function"
}
