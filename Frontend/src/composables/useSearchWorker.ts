import { computed, onUnmounted, ref, shallowRef } from "vue"
import { type ArtifactFlavor, artifactFlavor, supportsSimd128, wasmThreadsAvailable } from "@/lib/wasm-runtime"
import type { SearchEvent, SearchRequest, SearchState } from "@/types/worker"

export function useSearchWorker() {
  const state = ref<SearchState>("idle")
  const activeBackend = ref("No backend")
  const device = ref("No device")
  const metrics = shallowRef({ evaluated: 0n, elapsedMs: 0 })
  const difficulty = ref(0)
  const fallbacks = ref<string[]>([])
  const runtimeMessage = ref("Configure and run a search.")
  const result = shallowRef<{ uuid: string; hash: string; index: bigint } | null>(null)
  const running = ref(false)

  const progressPercent = computed(() => {
    const attempts = Number(metrics.value.evaluated)
    const probability = difficulty.value > 0 && attempts > 0 ? 1 - Math.exp(-attempts / 2 ** difficulty.value) : 0
    return probability >= 1 ? 100 : probability * 100
  })

  const expectedSeconds = computed(() => {
    const attempts = Number(metrics.value.evaluated)
    const elapsedMs = metrics.value.elapsedMs
    return attempts > 0 && elapsedMs > 0 && difficulty.value > 0
      ? 2 ** difficulty.value / (attempts / (elapsedMs / 1000))
      : 0
  })

  let worker: Worker | undefined
  let workerKey: string | undefined
  let workerReady = false
  let pendingRequest: SearchRequest | undefined
  let lastPaint = 0

  const simdSupported = supportsSimd128()
  const detectedThreads = Math.min(256, Math.max(1, navigator.hardwareConcurrency || 1))

  function terminateWorker() {
    worker?.terminate()
    worker = undefined
    workerKey = undefined
    workerReady = false
  }

  function createWorker(flavor: ArtifactFlavor, threads: number, debug: boolean) {
    const params = new URLSearchParams({ flavor, threads: String(threads) })
    if (debug) params.set("debug", "1")
    const next = new Worker(`${import.meta.env.BASE_URL}search-worker.js?${params}`, { type: "module" })
    next.onmessage = (e: MessageEvent<SearchEvent>) => {
      if (worker === next) handleMessage(e.data)
    }
    next.onerror = (e) => {
      if (worker !== next) return
      handleMessage({
        type: "loaderError",
        flavor,
        message: e.message || "Worker startup failed before a detailed error was reported.",
      })
    }
    return next
  }

  function startWorker(request: SearchRequest, forcedFlavor?: ArtifactFlavor) {
    const flavor = forcedFlavor ?? artifactFlavor(request.backend, simdSupported)
    const key = `${flavor}:${request.threads}`
    pendingRequest = request
    if (worker && workerKey === key) {
      if (workerReady) worker.postMessage({ type: "start", request })
      return
    }
    terminateWorker()
    workerKey = key
    worker = createWorker(flavor, request.threads, request.debug)
  }

  function start(request: SearchRequest) {
    difficulty.value = request.difficulty
    lastPaint = 0
    metrics.value = { evaluated: 0n, elapsedMs: 0 }
    fallbacks.value = []
    result.value = null
    activeBackend.value = "Preparing"
    device.value = "No device"
    running.value = true
    state.value = "preparing"
    runtimeMessage.value = "Loading compute modules…"
    startWorker(request)
  }

  function stop() {
    if (workerReady) {
      worker?.postMessage({ type: "stop" })
      runtimeMessage.value = "Stopping after current batch…"
      return
    }
    terminateWorker()
    pendingRequest = undefined
    running.value = false
    state.value = "stopped"
    runtimeMessage.value = "Stopped before preparation completed."
  }

  function handleMessage(event: SearchEvent) {
    switch (event.type) {
      case "loaderError": {
        const wasReady = workerReady
        terminateWorker()
        if (
          !wasReady &&
          event.flavor === "accelerated" &&
          pendingRequest &&
          (pendingRequest.backend === "auto" || pendingRequest.backend === "scalar")
        ) {
          startWorker({ ...pendingRequest, acceleratedLoadFailed: true }, "scalar")
        } else if (running.value) {
          pendingRequest = undefined
          handleMessage({
            type: "error",
            message: wasReady ? event.message : `${event.flavor} WASM failed to load: ${event.message}`,
          })
        }
        break
      }

      case "ready":
        if (!workerReady) {
          workerReady = true
          if (pendingRequest) worker?.postMessage({ type: "start", request: pendingRequest })
        }
        break

      case "prepared":
        activeBackend.value = event.backend
        fallbacks.value = event.fallbacks
        state.value = "running"
        runtimeMessage.value = "Searching…"
        break

      case "device":
        device.value = event.name
        break

      case "progress": {
        const now = performance.now()
        if (now - lastPaint >= 50) {
          metrics.value = { evaluated: BigInt(event.evaluated), elapsedMs: event.elapsedMs }
          lastPaint = now
        }
        break
      }

      case "done":
        running.value = false
        pendingRequest = undefined
        metrics.value = { evaluated: BigInt(event.evaluated), elapsedMs: event.elapsedMs }
        fallbacks.value = event.fallbacks
        if (event.kind === "found") {
          state.value = "found"
          result.value = { uuid: event.uuid ?? "", hash: event.hash ?? "", index: BigInt(event.index ?? "0") }
          runtimeMessage.value = "Lowest match in verified prefix."
        } else {
          state.value = event.kind
          result.value = null
          runtimeMessage.value = event.kind === "stopped" ? "Search stopped." : "Range fully evaluated."
        }
        break

      case "error":
        running.value = false
        state.value = "error"
        runtimeMessage.value = event.message
        break
    }
  }

  onUnmounted(terminateWorker)

  if (wasmThreadsAvailable()) {
    const flavor = artifactFlavor("auto", simdSupported)
    worker = createWorker(flavor, detectedThreads, false)
    workerKey = `${flavor}:${detectedThreads}`
  }

  return {
    state,
    activeBackend,
    device,
    metrics,
    progressPercent,
    expectedSeconds,
    fallbacks,
    runtimeMessage,
    result,
    running,
    simdSupported,
    detectedThreads,
    start,
    stop,
  }
}
