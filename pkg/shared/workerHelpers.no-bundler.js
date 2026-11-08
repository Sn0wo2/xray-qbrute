/*
 * Derived from wasm-bindgen-rayon 1.3.0 workerHelpers.no-bundler.js.
 * Copyright 2022 Google Inc. Licensed under Apache-2.0.
 */

function waitForReady(worker) {
  return new Promise((resolve, reject) => {
    const onMessage = ({ data }) => {
      if (data?.type !== "wasm_bindgen_worker_ready") return;
      worker.removeEventListener("message", onMessage);
      worker.removeEventListener("error", onError);
      resolve();
    };
    const onError = (event) => {
      worker.removeEventListener("message", onMessage);
      worker.removeEventListener("error", onError);
      reject(new Error(event.message || "Rayon worker failed to initialize"));
    };
    worker.addEventListener("message", onMessage);
    worker.addEventListener("error", onError);
  });
}

let workers;

export async function startWorkers(module, memory, builder) {
  const threadCount = builder.numThreads();
  if (threadCount === 0) throw new Error("num_threads must be > 0");

  const mainUrl = new URL(builder.mainJS());
  const workerGlueResponse = await fetch(new URL("xray_qbrute.worker.js", mainUrl));
  if (!workerGlueResponse.ok) {
    throw new Error(`Rayon worker glue request failed: ${workerGlueResponse.status}`);
  }
  const workerModuleUrl = URL.createObjectURL(new Blob(
    [await workerGlueResponse.text()],
    { type: "text/javascript" },
  ));
  const bootstrapUrl = URL.createObjectURL(new Blob([
    `self.addEventListener("message", async function initialize({ data }) {
      if (data?.type !== "wasm_bindgen_worker_init") return;
      self.removeEventListener("message", initialize);
      const pkg = await import(data.workerModuleUrl);
      await pkg.default({ module_or_path: data.module, memory: data.memory });
      self.postMessage({ type: "wasm_bindgen_worker_ready" });
      pkg.wbg_rayon_start_worker(data.receiver);
    });`,
  ], { type: "text/javascript" }));

  const workerInit = {
    type: "wasm_bindgen_worker_init",
    module,
    memory,
    receiver: builder.receiver(),
    workerModuleUrl,
  };

  try {
    workers = Array.from({ length: threadCount }, () => new Worker(bootstrapUrl, { type: "module" }));
    await Promise.all(workers.map(async (worker) => {
      const ready = waitForReady(worker);
      worker.postMessage(workerInit);
      await ready;
    }));
    builder.build();
  } finally {
    URL.revokeObjectURL(bootstrapUrl);
    URL.revokeObjectURL(workerModuleUrl);
  }
}
