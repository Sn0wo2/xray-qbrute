import { computed, onUnmounted, ref, shallowRef } from "vue"
import { type ArtifactFlavor, artifactFlavor, supportsSimd128, wasmThreadsAvailable } from "@/lib/wasm-runtime"
import type { DoneKind, Metrics, SearchEvent, SearchRequest, SearchResult, SearchState } from "@/types/worker"

export function useSearchWorker() {
  const state = ref<SearchState>("idle")
  const stateLabel = computed(
    () =>
      ({
        idle: "Idle",
        preparing: "Preparing",
        running: "Running",
        found: "Found",
        stopped: "Stopped",
        exhausted: "Exhausted",
        error: "Error",
      })[state.value],
  )
  const activeBackend = ref("No backend")
  const device = ref("No device")
  const metrics = shallowRef<Metrics>({
    processed: 0n,
    evaluated: 0n,
    maxIndex: 0n,
    elapsedMs: 0,
  })
  const difficulty = ref(0)

  const progressPercent = computed(() => {
    const bits = difficulty.value
    if (bits === 0 || bits > 64) return 0
    const attempts = Number(metrics.value.evaluated)
    if (attempts === 0) return 0
    const probability = 1 - Math.exp(-attempts / 2 ** bits)
    return probability >= 1 ? 100 : probability * 100
  })

  const expectedSeconds = computed(() => {
    const bits = difficulty.value
    if (bits === 0 || bits > 64) return 0
    const attempts = Number(metrics.value.evaluated)
    const elapsedMs = metrics.value.elapsedMs
    if (attempts === 0 || elapsedMs <= 0) return 0
    return 2 ** bits / (attempts / (elapsedMs / 1000))
  })

  const fallbacks = ref<string[]>([])
  const runtimeMessage = ref("Configure and run a search.")
  const result = shallowRef<SearchResult | null>(null)
  const doneKind = ref<DoneKind | null>(null)
  const running = ref(false)

  let worker: Worker | undefined
  let workerKey: string | undefined
  let workerReady = false
  let pendingRequest: SearchRequest | undefined
  let currentRequest: SearchRequest | undefined
  let lastPaint = 0

  const simdSupported = supportsSimd128()
  const detectedThreads = Math.min(256, Math.max(1, navigator.hardwareConcurrency || 1))

  onUnmounted(() => {
    worker?.terminate()
    worker = undefined
  })

  if (wasmThreadsAvailable() && !worker) {
    const flavor = artifactFlavor("auto", simdSupported)
    worker = spawnWorker(
      {
        commit: 0,
        nodeSuffix: 0,
        difficulty: 20,
        maxIndex: 1n << 58n,
        backend: "auto",
        threads: detectedThreads,
        debug: false,
      },
      flavor,
      `${flavor}:${detectedThreads}`,
    )
  }

  function start(request: SearchRequest) {
    currentRequest = request
    difficulty.value = request.difficulty
    lastPaint = 0
    metrics.value = { processed: 0n, evaluated: 0n, maxIndex: 0n, elapsedMs: 0 }
    fallbacks.value = []
    result.value = null
    doneKind.value = null
    activeBackend.value = "Preparing"
    device.value = "No device"
    running.value = true
    state.value = "preparing"
    runtimeMessage.value = "Loading compute modules…"
    startWorker(request)
  }

  function stop() {
    if (!workerReady) {
      worker?.terminate()
      worker = undefined
      workerKey = undefined
      workerReady = false
      pendingRequest = undefined
      running.value = false
      state.value = "stopped"
      doneKind.value = "stopped"
      runtimeMessage.value = "Stopped before preparation completed."
      return
    }
    worker?.postMessage({ type: "stop" })
    runtimeMessage.value = "Stopping after current batch…"
  }

  function startWorker(request: SearchRequest, forcedFlavor?: ArtifactFlavor) {
    const flavor = forcedFlavor ?? artifactFlavor(request.backend, simdSupported)
    const key = `${flavor}:${request.threads}`
    pendingRequest = request

    if (worker && workerKey === key) {
      if (workerReady) worker.postMessage({ type: "start", request })
      return
    }

    worker?.terminate()
    workerReady = false
    workerKey = key

    worker = spawnWorker(request, flavor, key)
  }

  function spawnWorker(request: SearchRequest, flavor: ArtifactFlavor, key: string) {
    const params = new URLSearchParams({ flavor, threads: String(request.threads) })
    if (request.debug) params.set("debug", "1")
    const next = new Worker(`${import.meta.env.BASE_URL}search-worker.js?${params}`, {
      type: "module",
    })
    next.onmessage = (e: MessageEvent<SearchEvent>) => {
      if (worker !== next) return
      handleMessage(e.data)
    }
    next.onerror = (e) => {
      if (worker !== next) return
      handleMessage({
        type: "loaderError",
        flavor,
        message: e.message || "Worker startup failed before a detailed error was reported.",
      })
    }
    workerKey = key
    return next
  }

  function handleMessage(event: SearchEvent) {
    if (
      event.type === "loaderError" &&
      event.flavor === "accelerated" &&
      pendingRequest &&
      ["auto", "scalar"].includes(pendingRequest.backend)
    ) {
      worker?.terminate()
      worker = undefined
      workerKey = undefined
      workerReady = false
      startWorker({ ...pendingRequest, acceleratedLoadFailed: true }, "scalar")
      return
    }

    if (event.type === "loaderError") {
      worker?.terminate()
      worker = undefined
      workerKey = undefined
      workerReady = false
      if (running.value) {
        handleMessage({
          type: "error",
          message: `${event.flavor} WASM failed to load: ${event.message}`,
        })
      }
      return
    }

    // biome-ignore lint/nursery/useExhaustiveSwitchCases: loaderError is handled above; biome does not apply the narrowing
    switch (event.type) {
      case "ready":
        if (!workerReady) {
          workerReady = true
          if (pendingRequest) {
            worker?.postMessage({ type: "start", request: pendingRequest })
          }
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

      case "progress":
        if (performance.now() - lastPaint >= 50) {
          metrics.value = {
            processed: BigInt(event.processed),
            evaluated: BigInt(event.evaluated),
            maxIndex: BigInt(event.maxIndex),
            elapsedMs: event.elapsedMs,
          }
          lastPaint = performance.now()
        }
        break

      case "done":
        running.value = false
        pendingRequest = undefined
        metrics.value = {
          processed: BigInt(event.processed),
          evaluated: BigInt(event.evaluated),
          maxIndex: currentRequest?.maxIndex ?? 0n,
          elapsedMs: event.elapsedMs,
        }
        fallbacks.value = event.fallbacks

        if (event.kind === "found") {
          state.value = "found"
          doneKind.value = "found"
          result.value = {
            uuid: event.uuid ?? "",
            hash: event.hash ?? "",
            index: BigInt(event.index ?? "0"),
          }
          runtimeMessage.value = "Lowest match in verified prefix."
        } else {
          doneKind.value = event.kind
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

  return {
    state,
    stateLabel,
    activeBackend,
    device,
    metrics,
    progressPercent,
    expectedSeconds,
    fallbacks,
    runtimeMessage,
    result,
    doneKind,
    running,
    simdSupported,
    detectedThreads,
    start,
    stop,
  }
}
