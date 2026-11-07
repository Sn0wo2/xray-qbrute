#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { artifactFlavor, wasmThreadsAvailable } from '../Frontend/public/wasm-runtime.js'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const frontendDir = path.join(root, 'Frontend')
const pkgDir = path.join(frontendDir, 'public', 'pkg')
const wasmTarget = 'wasm32-unknown-unknown'
const wasmOut = path.join(root, 'target', wasmTarget, 'release', 'xray_qbrute.wasm')

const flavors = [
  { name: 'scalar', features: 'wasm-controller' },
  { name: 'accelerated', features: 'wasm-webgpu,wasm-simd' },
]

const RAYON_HELPER = `/*
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
    throw new Error(\`Rayon worker glue request failed: \${workerGlueResponse.status}\`);
  }
  const workerModuleUrl = URL.createObjectURL(new Blob(
    [await workerGlueResponse.text()],
    { type: "text/javascript" },
  ));
  const bootstrapUrl = URL.createObjectURL(new Blob([
    \`self.addEventListener("message", async function initialize({ data }) {
      if (data?.type !== "wasm_bindgen_worker_init") return;
      self.removeEventListener("message", initialize);
      const pkg = await import(data.workerModuleUrl);
      await pkg.default({ module_or_path: data.module, memory: data.memory });
      self.postMessage({ type: "wasm_bindgen_worker_ready" });
      pkg.wbg_rayon_start_worker(data.receiver);
    });\`,
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
`

function run(command, args, opts = {}) {
  console.log(`\n$ ${[command, ...args].join(' ')}`)
  const result = spawnSync(command, args, {
    cwd: opts.cwd ?? root,
    stdio: 'inherit',
    shell: false,
  })
  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}

function buildCli() {
  run('cargo', ['build', '--release'])
}

function buildWasm() {
  fs.rmSync(pkgDir, { recursive: true, force: true })
  for (const flavor of flavors) {
    run('cargo', [
      'build',
      '--release',
      '-Zbuild-std=panic_abort,std',
      '-p',
      'xray-qbrute-wasm',
      '--lib',
      '--target',
      wasmTarget,
      '--no-default-features',
      '--features',
      flavor.features,
    ])
    run('wasm-bindgen', [
      '--target',
      'web',
      '--no-typescript',
      '--out-dir',
      path.join(pkgDir, flavor.name),
      '--out-name',
      'xray_qbrute',
      wasmOut,
    ])

    const directory = path.join(pkgDir, flavor.name)
    const gluePath = path.join(directory, 'xray_qbrute.js')
    const glue = fs.readFileSync(gluePath, 'utf8')
    const workerGlue = glue.replace(
      /^import \{ startWorkers \} from '.+workerHelpers\.no-bundler\.js';$/m,
      "const startWorkers = () => { throw new Error('nested Rayon pool initialization is unavailable'); };",
    )
    if (workerGlue === glue) throw new Error(`Rayon helper import not found in ${gluePath}`)
    fs.writeFileSync(path.join(directory, 'xray_qbrute.worker.js'), workerGlue)

    const sharedHelperPath = path.join(pkgDir, 'shared', 'workerHelpers.no-bundler.js')
    if (!fs.existsSync(sharedHelperPath)) {
      fs.mkdirSync(path.dirname(sharedHelperPath), { recursive: true })
      fs.writeFileSync(sharedHelperPath, RAYON_HELPER)
    }

    const rayonDirectory = fs
      .readdirSync(path.join(directory, 'snippets'), { withFileTypes: true })
      .find((entry) => entry.isDirectory() && entry.name.startsWith('wasm-bindgen-rayon-'))
    if (!rayonDirectory) throw new Error(`Rayon snippet directory not found in ${directory}`)

    const glueImportPath = `./snippets/${rayonDirectory.name}/src/workerHelpers.no-bundler.js`
    const reGlue = glue.replace(
      new RegExp(`from '${glueImportPath.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}'`),
      `from '../shared/workerHelpers.no-bundler.js'`,
    )
    if (reGlue === glue) throw new Error(`Glue import path not rewritten in ${gluePath}`)
    fs.writeFileSync(gluePath, reGlue)

    fs.rmSync(path.join(directory, 'snippets'), { recursive: true, force: true })
  }
}

function verify() {
  const webPackage = JSON.parse(fs.readFileSync(path.join(frontendDir, 'package.json'), 'utf8'))
  if (!webPackage.dependencies?.['coi-serviceworker']) {
    throw new Error('coi-serviceworker is not in Frontend/package.json dependencies')
  }
  for (const flavor of flavors) {
    const directory = path.join(pkgDir, flavor.name)
    const glue = fs.readFileSync(path.join(directory, 'xray_qbrute.js'), 'utf8')
    const workerGlue = fs.readFileSync(path.join(directory, 'xray_qbrute.worker.js'), 'utf8')
    const wasm = fs.readFileSync(path.join(directory, 'xray_qbrute_bg.wasm'))
    if (!WebAssembly.validate(wasm)) throw new Error(`${flavor.name} WASM is invalid`)
    if (!glue.includes('initThreadPool')) throw new Error(`${flavor.name} thread export missing`)
    if (!glue.includes('../shared/workerHelpers.no-bundler.js')) {
      throw new Error(`${flavor.name} glue does not reference the shared Rayon helper`)
    }
    if (
      workerGlue.includes('import { startWorkers }') ||
      !workerGlue.includes('nested Rayon pool initialization')
    ) {
      throw new Error(`${flavor.name} worker glue still imports the Rayon pool helper`)
    }
  }
  const helper = fs.readFileSync(path.join(pkgDir, 'shared', 'workerHelpers.no-bundler.js'), 'utf8')
  if (helper.includes('fetch(import.meta.url)') || !helper.includes('workerModuleUrl')) {
    throw new Error('Rayon helper still downloads one bootstrap per thread')
  }
  if (
    artifactFlavor('scalar', true) !== 'accelerated' ||
    artifactFlavor('wasm-simd', true) !== 'accelerated'
  ) {
    throw new Error('SIMD-capable browsers do not reuse one artifact for Scalar and SIMD')
  }
  if (artifactFlavor('scalar', false) !== 'scalar' || artifactFlavor('auto', false) !== 'scalar') {
    throw new Error('baseline browsers do not select the scalar artifact')
  }
  if (!wasmThreadsAvailable({ crossOriginIsolated: true, SharedArrayBuffer })) {
    throw new Error('cross-origin isolated runtimes were rejected')
  }
  if (wasmThreadsAvailable({ crossOriginIsolated: false, SharedArrayBuffer })) {
    throw new Error('non-isolated runtimes were accepted for WASM threads')
  }
  console.log(
    'Browser artifacts validated; Scalar/SIMD share the accelerated package when SIMD128 is available',
  )
}

async function buildWeb() {
  const dist = path.join(frontendDir, 'dist')
  fs.rmSync(dist, { recursive: true, force: true })
  run('node', [path.join(frontendDir, 'node_modules/vue-tsc/bin/vue-tsc.js'), '--noEmit'], {
    cwd: frontendDir,
  })
  run('node', [path.join(frontendDir, 'node_modules/vite/bin/vite.js'), 'build'], { cwd: frontendDir })

  const { transform } = await import(
    pathToFileURL(path.join(frontendDir, 'node_modules/esbuild/lib/main.js')).href,
  )
  const files = [
    ...flavors.flatMap((flavor) => [
      path.join(dist, 'pkg', flavor.name, 'xray_qbrute.js'),
      path.join(dist, 'pkg', flavor.name, 'xray_qbrute.worker.js'),
    ]),
    path.join(dist, 'coi-serviceworker.js'),
    path.join(dist, 'search-worker.js'),
    path.join(dist, 'wasm-runtime.js'),
  ]
  for (const file of files) {
    if (!fs.existsSync(file)) {
      console.warn(`skip missing ${path.relative(frontendDir, file)}`)
      continue
    }
    const format = file.endsWith('coi-serviceworker.js') ? 'iife' : 'esm'
    const before = fs.statSync(file).size
    const { code } = await transform(fs.readFileSync(file, 'utf8'), { minify: true, format })
    fs.writeFileSync(file, code)
    const after = Buffer.byteLength(code)
    console.log(
      `minified ${path.relative(frontendDir, file)}: ${before} -> ${after} bytes (-${Math.round((1 - after / before) * 100)}%)`,
    )
  }
}

async function buildWebAll() {
  buildWasm()
  verify()
  await buildWeb()
}

const tasks = {
  cli: 'build the CLI binary (crates/cli)',
  wasm: 'build the WebAssembly packages (Frontend/public/pkg/{scalar,accelerated})',
  frontend: 'build the web frontend (vue-tsc + vite)',
  'frontend-all': 'build the full web deliverable (wasm packages, validate, frontend)',
  verify: 'validate the built wasm packages',
  all: 'build everything (cli + frontend-all)',
}

function printHelp() {
  console.log('xray-qbrute build script\n')
  console.log('usage: node scripts/build.ts [task]\n')
  console.log('tasks:')
  for (const [name, description] of Object.entries(tasks)) {
    console.log(`  ${name.padEnd(9)} ${description}`)
  }
  console.log('\n(default: all)')
}

const task = process.argv[2] ?? 'all'
if (task === '--help' || task === '-h') {
  printHelp()
  process.exit(0)
}

async function main() {
  switch (task) {
    case 'cli':
      buildCli()
      break
    case 'wasm':
      buildWasm()
      break
    case 'verify':
      verify()
      break
    case 'frontend':
      await buildWeb()
      break
    case 'frontend-all':
      await buildWebAll()
      break
    case 'all':
      buildCli()
      await buildWebAll()
      break
    default:
      console.error(`unknown task: ${task}`)
      printHelp()
      process.exit(1)
  }
  console.log('\nOK')
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
