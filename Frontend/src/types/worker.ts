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

export type DoneKind = "found" | "stopped" | "exhausted"

export type SearchEvent =
  | { type: "ready" }
  | { type: "prepared"; backend: string; fallbacks: string[] }
  | { type: "device"; name: string }
  | { type: "progress"; evaluated: string; elapsedMs: number }
  | {
      type: "done"
      kind: DoneKind
      index: string | null
      evaluated: string
      elapsedMs: number
      uuid: string | null
      hash: string | null
      fallbacks: string[]
    }
  | { type: "error"; message: string }
  | { type: "loaderError"; flavor: string; message: string }

export type SearchState = "idle" | "preparing" | "running" | "found" | "stopped" | "exhausted" | "error"
