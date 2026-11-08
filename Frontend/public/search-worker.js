import { WASM_THREADS_ERROR, wasmThreadsAvailable } from "./wasm-runtime.js"

const parameters = new URL(import.meta.url).searchParams
const flavor = parameters.get("flavor") || "accelerated"
const threads = Number(parameters.get("threads"))
const debug = parameters.get("debug") === "1"
const log = (...values) => {
  if (debug) console.debug("[search-worker]", ...values)
}

try {
  log("startup", {
    flavor,
    threads,
    crossOriginIsolated,
    sharedArrayBuffer: typeof SharedArrayBuffer === "function",
  })
  if (!wasmThreadsAvailable()) throw new Error(WASM_THREADS_ERROR)
  const importStarted = performance.now()
  const module = await import(`./pkg/${flavor}/xray_qbrute.js`)
  log("glue imported", { elapsedMs: performance.now() - importStarted })
  const wasmStarted = performance.now()
  await module.default()
  log("WASM initialized", { elapsedMs: performance.now() - wasmStarted })
  const rayonStarted = performance.now()
  await module.initThreadPool(threads)
  log("Rayon initialized", { threads, elapsedMs: performance.now() - rayonStarted })
  module.install_coordinator()
  module.prewarm_gpu?.()
  log("coordinator installed")
} catch (error) {
  console.error("[search-worker] startup failed", error)
  const missingArtifact = error instanceof TypeError && /import|module|fetch/i.test(error.message)
  self.postMessage({
    type: "loaderError",
    flavor,
    message: missingArtifact
      ? `${flavor} browser artifact is missing. Run the frontend build before serving Frontend/.`
      : error instanceof Error
        ? error.message
        : String(error),
  })
}
