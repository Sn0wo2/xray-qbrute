export type Backend = "auto" | "webgpu" | "wasm-simd" | "scalar"

export interface SearchRequest {
  commit: number
  nodeSuffix: number
  difficulty: number
  maxIndex: bigint
  backend: Backend
  threads: number
  debug: boolean
  acceleratedLoadFailed?: boolean
}

export type CoordinatorCommand = { type: "start"; request: SearchRequest } | { type: "stop" }

export type DoneKind = "found" | "stopped" | "exhausted"

export type SearchEvent =
  | { type: "ready" }
  | { type: "prepared"; backend: string; fallbacks: string[] }
  | { type: "device"; name: string }
  | {
      type: "progress"
      processed: string
      evaluated: string
      maxIndex: string
      elapsedMs: number
    }
  | {
      type: "done"
      kind: DoneKind
      index: string | null
      processed: string
      evaluated: string
      elapsedMs: number
      uuid: string | null
      hash: string | null
      fallbacks: string[]
    }
  | { type: "error"; message: string }
  | { type: "loaderError"; flavor: string; message: string }

export type SearchState = "idle" | "preparing" | "running" | "found" | "stopped" | "exhausted" | "error"

export interface SearchResult {
  uuid: string
  hash: string
  index: bigint
}

export interface Metrics {
  processed: bigint
  evaluated: bigint
  maxIndex: bigint
  elapsedMs: number
}
