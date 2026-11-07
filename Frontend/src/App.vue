<script setup lang="ts">
import {
  dateEnUS,
  enUS,
  NAlert,
  NButton,
  NProgress,
  NSelect,
  NSkeleton,
  TiButton,
  TiCard,
  type TiColorMode,
  TiInput,
  TiProvider,
  TiTag,
  TiThemeSwitch,
} from "@teal-ink/vue"
import { computed, reactive, ref, watch } from "vue"
import { formatExpectedDuration, formatRate, integerFormatter } from "@/composables/useFormatters"
import { useSearchWorker } from "@/composables/useSearchWorker"
import { GLYPHS } from "@/constants/glyphs"
import { THEME_SWITCH_ICONS, THEME_SWITCH_LABELS } from "@/constants/theme"
import { WASM_THREADS_ERROR, wasmThreadsAvailable } from "@/lib/wasm-runtime"
import type { Backend } from "@/types/worker"

const storedMode = localStorage.getItem("ti-color-mode")
const mode = ref<TiColorMode>(
  storedMode === "light" || storedMode === "dark" || storedMode === "system" ? storedMode : "system",
)
watch(mode, (value) => {
  localStorage.setItem("ti-color-mode", value)
})

const {
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
} = useSearchWorker()

const form = reactive({
  commit: "90243a74",
  node: "",
  difficulty: "33",
  maxIndex: (1n << 58n).toString(),
  backend: "auto" as Backend,
  threads: String(detectedThreads),
})
const formError = ref("")
const copied = ref(false)

const backendOptions = [
  { label: "Auto · WebGPU -> WASM SIMD128", value: "auto" },
  { label: "WebGPU", value: "webgpu" },
  { label: "WASM SIMD128", value: "wasm-simd" },
  { label: "Scalar WASM", value: "scalar" },
]

const statusTone = computed<"accent" | "error" | "neutral">(() => {
  // biome-ignore lint/nursery/useExhaustiveSwitchCases: every SearchState variant is handled; biome cannot narrow Ref.value
  switch (state.value) {
    case "preparing":
    case "running":
    case "found":
      return "accent"
    case "error":
      return "error"
    case "idle":
    case "stopped":
    case "exhausted":
      return "neutral"
  }
})

const stateGlyph = computed(() => {
  // biome-ignore lint/nursery/useExhaustiveSwitchCases: every SearchState variant is handled; biome cannot narrow Ref.value
  switch (state.value) {
    case "idle":
      return GLYPHS.circle
    case "preparing":
      return GLYPHS.play
    case "running":
      return GLYPHS.spinner
    case "found":
      return GLYPHS.flag
    case "stopped":
      return GLYPHS.square
    case "exhausted":
      return GLYPHS.ban
    case "error":
      return GLYPHS.alertTriangle
  }
})

async function copyAnswer() {
  if (!result.value) return
  try {
    await navigator.clipboard.writeText(`/answer ${result.value.uuid}`)
    copied.value = true
    setTimeout(() => {
      copied.value = false
    }, 1500)
  } catch (error) {
    if (!(error instanceof DOMException || error instanceof TypeError)) {
      throw error
    }
  }
}

function onSubmit() {
  formError.value = ""
  const formElement = document.getElementById("search-form") as HTMLFormElement | null
  const inputs = formElement?.querySelectorAll<HTMLInputElement>("input")
  for (const input of inputs ?? []) {
    if (!input.checkValidity()) {
      input.setAttribute("aria-invalid", "true")
      input.focus()
      formError.value = input.validationMessage
      return
    }
    input.removeAttribute("aria-invalid")
  }

  if (!wasmThreadsAvailable()) {
    formError.value = WASM_THREADS_ERROR
    return
  }
  if (form.backend === "wasm-simd" && !simdSupported) {
    formError.value = "This browser does not support WASM SIMD128."
    return
  }
  if (form.backend === "webgpu" && !simdSupported) {
    formError.value = "The current WebGPU artifact also requires WASM SIMD128 support."
    return
  }

  const maxIndex = form.maxIndex.trim() === "" ? 1n << 58n : BigInt(form.maxIndex)
  if (maxIndex > 1n << 58n) {
    formError.value = "Maximum index cannot exceed 2^58."
    return
  }

  const threads = Number(form.threads)
  start({
    commit: Number.parseInt(form.commit, 16),
    nodeSuffix: Number.parseInt(form.node, 16),
    difficulty: Number(form.difficulty),
    maxIndex,
    backend: form.backend,
    threads: threads === 0 ? detectedThreads : threads,
    debug: new URLSearchParams(location.search).has("debug"),
  })
}
</script>

<template>
  <TiProvider v-model:mode="mode" global :locale="enUS" :date-locale="dateEnUS">
    <div class="app-layout">
      <header class="app-header">
        <div>
          <h1 class="app-title">xray-qbrute</h1>
          <p class="app-subtitle">Brute-force Xray-core Telegram group question 26/7/14</p>
        </div>
        <div class="app-header-actions">
          <TiThemeSwitch label="Color mode" :labels="THEME_SWITCH_LABELS" :icons="THEME_SWITCH_ICONS" />
        </div>
      </header>

      <main class="app-main">
        <form id="search-form" @submit.prevent="onSubmit">
          <TiCard class="reveal">
            <template #header>
              <span class="section-overline">01 / input</span>
              <div class="section-title">Configuration</div>
            </template>

            <div class="field-grid">
              <TiInput
                v-model="form.commit"
                label="Commit hash"
                hint="Required · 8 hex characters"
                :maxlength="8"
                pattern="[0-9a-fA-F]{8}"
                required
                spellcheck="false"
                :disabled="running"
              />
              <TiInput
                v-model="form.node"
                label="Node suffix"
                hint="Required · 8 hex characters · example: e4b3a192"
                :maxlength="8"
                pattern="[0-9a-fA-F]{8}"
                required
                spellcheck="false"
                :disabled="running"
              />
              <TiInput
                v-model="form.difficulty"
                label="Difficulty"
                hint="Required · Leading zero bits (0-64)"
                :maxlength="2"
                inputmode="numeric"
                pattern="([0-9]|[1-5][0-9]|6[0-4])"
                required
                :disabled="running"
              />
              <TiInput
                v-model="form.maxIndex"
                label="Maximum index"
                hint="Required · Full range (2⁵⁸)"
                :maxlength="19"
                inputmode="numeric"
                pattern="[0-9]+"
                required
                :disabled="running"
              />
              <div class="select-field field-full">
                <span class="select-label">Backend</span>
                <NSelect
                  class="select-input"
                  :value="form.backend"
                  :options="backendOptions"
                  :disabled="running"
                   @update:value="form.backend = $event as typeof form.backend"
                />
                <span class="select-hint">Auto tries WebGPU, falls back to SIMD</span>
              </div>
              <TiInput
                v-model="form.threads"
                class="field-full"
                label="Threads"
                :hint="`Required · Recommended: ${detectedThreads} threads`"
                :maxlength="3"
                inputmode="numeric"
                pattern="([0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-6])"
                required
                :disabled="running"
              />
            </div>

            <NAlert v-if="formError" class="form-error" type="error" :show-icon="true" :title="formError" />

            <template #footer>
              <div class="actions">
                <TiButton v-if="!running" class="action-button" variant="primary" type="submit">
                  <span class="glyph" aria-hidden="true">{{ GLYPHS.play }}</span>
                  Run
                </TiButton>
                <NButton v-else secondary type="error" class="action-button" @click="stop">
                  <span class="glyph" aria-hidden="true">{{ GLYPHS.stop }}</span>
                  Stop
                </NButton>
              </div>
            </template>
          </TiCard>
        </form>

        <TiCard class="reveal">
          <template #header>
            <div class="card-header-row">
              <div>
                <span class="section-overline">02 / execution</span>
                <TiTag class="status" :tone="statusTone" size="small">
                  <span class="glyph" :class="{ spin: state === 'preparing' || state === 'running' }" aria-hidden="true">{{ stateGlyph }}</span>
                  {{ stateLabel }}
                </TiTag>
              </div>
              <div class="execution-meta">
                <div>{{ activeBackend }}</div>
                <div>{{ device }}</div>
              </div>
            </div>
          </template>

          <div class="progress-row">
            <span>P (hit in range)</span>
            <span class="mono">{{ progressPercent.toFixed(2) }}%</span>
          </div>
          <NProgress type="line" :percentage="progressPercent" :show-indicator="false" :height="8" :border-radius="4" />

          <dl class="metrics-grid">
            <div><dt>Evaluated</dt><dd class="mono">{{ integerFormatter.format(metrics.evaluated) }}</dd></div>
            <div><dt>Elapsed</dt><dd class="mono">{{ (metrics.elapsedMs / 1000).toFixed(2) }} s</dd></div>
            <div><dt>Rate</dt><dd class="mono">{{ formatRate(metrics.evaluated, metrics.elapsedMs) }}</dd></div>
            <div><dt>Expected</dt><dd class="mono">{{ formatExpectedDuration(expectedSeconds) }}</dd></div>
          </dl>

          <NAlert v-if="fallbacks.length > 0" class="fallback-alert" type="info" :show-icon="true" title="Fallback trace">
            <ul class="fallback-list">
              <li v-for="reason in fallbacks" :key="reason">{{ reason }}</li>
            </ul>
          </NAlert>

          <p class="status-text" role="status">{{ runtimeMessage }}</p>
        </TiCard>

        <TiCard class="reveal">
          <template #header>
            <span class="section-overline">03 / result</span>
            <div class="section-title">
              <template v-if="result">Match found</template>
              <template v-else-if="doneKind === 'stopped'">Search stopped</template>
              <template v-else-if="doneKind === 'exhausted'">Range exhausted</template>
              <template v-else>No result yet</template>
            </div>
          </template>

          <dl v-if="result" class="result-list">
            <div><dt>UUID</dt><dd class="mono uuid-break">{{ result.uuid }}</dd></div>
            <div>
              <dt>Answer</dt>
              <dd class="answer-row">
                <code class="answer-code">/answer {{ result.uuid }}</code>
                <TiButton variant="text" size="small" aria-label="Copy answer" @click="copyAnswer">
                  <span class="glyph" aria-hidden="true">{{ copied ? GLYPHS.check : GLYPHS.copy }}</span>
                </TiButton>
              </dd>
            </div>
            <div><dt>Hash prefix</dt><dd class="mono">{{ result.hash }}</dd></div>
            <div><dt>Index</dt><dd class="mono">{{ integerFormatter.format(result.index) }}</dd></div>
          </dl>

          <p v-else-if="doneKind === 'stopped'" class="empty-text">Partial results retained above.</p>
          <p v-else-if="doneKind === 'exhausted'" class="empty-text">No match in range.</p>
          <NSkeleton v-else-if="state === 'preparing' || state === 'running'" text :repeat="4" animated />
          <p v-else class="empty-text">Awaiting search.</p>
        </TiCard>
      </main>

      <footer class="app-footer">
        <a href="https://github.com/Sn0wo2/xray-qbrute" target="_blank" rel="noopener noreferrer">
          <span class="glyph" aria-hidden="true">{{ GLYPHS.github }}</span>
          Star xray-qbrute if helpful~
        </a>
      </footer>
    </div>
  </TiProvider>
</template>

<style scoped lang="scss">
.app-layout {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

.app-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--ti-space-4);
  padding: var(--ti-space-6) var(--ti-space-8) var(--ti-space-2);
}

.app-title {
  font-size: var(--ti-font-size-heading);
  font-weight: 700;
  line-height: var(--ti-line-height-heading);
  letter-spacing: -0.02em;
}

.app-subtitle {
  margin-top: var(--ti-space-2);
  font-size: var(--ti-font-size-body);
  line-height: var(--ti-line-height-body);
  color: var(--ti-color-text-secondary);
}

.app-header-actions {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  gap: var(--ti-space-3);
}

.app-main {
  display: flex;
  flex-direction: column;
  gap: var(--ti-space-6);
  padding: var(--ti-space-6) var(--ti-space-8);
}

.app-footer {
  margin-top: auto;
  padding: var(--ti-space-6) var(--ti-space-8) var(--ti-space-8);
  border-top: 1px solid var(--ti-color-border);
  text-align: center;
  font-size: var(--ti-font-size-body);
  color: var(--ti-color-text-secondary);

  a {
    display: inline-flex;
    align-items: center;
    gap: var(--ti-space-2);
    color: var(--ti-color-text-secondary);
    text-decoration: none;
    transition: color var(--ti-motion-duration) var(--ti-motion-easing);

    &:hover {
      color: var(--ti-color-accent-text);
    }
  }

  .glyph {
    font-size: var(--ti-font-size-body);
  }
}

.card-header-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.section-overline {
  display: block;
  margin-bottom: 0.375rem;
  color: var(--ti-color-text-secondary);
  font-size: var(--ti-font-size-caption);
  font-weight: 500;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.section-title {
  font-size: 1.0625rem;
  font-weight: 600;
  letter-spacing: -0.01em;
}

.status .glyph {
  margin-right: 0.25rem;
}

.field-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--ti-space-5);

  @media (max-width: 480px) {
    grid-template-columns: 1fr;
  }
}

.field-full {
  grid-column: 1 / -1;
}

.select-field {
  display: grid;
  gap: var(--ti-space-2);
}

.select-label {
  color: var(--ti-color-text);
  font-size: var(--ti-font-size-label);
  font-weight: 500;
}

.select-hint {
  color: var(--ti-color-text-secondary);
  font-size: var(--ti-font-size-label);
}

.select-input {
  width: 100%;
}

.form-error,
.fallback-alert {
  margin-top: var(--ti-space-4);
}

.actions {
  display: flex;
  gap: var(--ti-space-3);
}

.action-button .glyph {
  margin-right: var(--ti-space-1);
}

.progress-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.625rem;
  font-size: var(--ti-font-size-body);
}

.metrics-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--ti-space-5);
  margin: var(--ti-space-4) 0 0;

  dt {
    margin-bottom: 0.125rem;
    color: var(--ti-color-text-secondary);
    font-size: var(--ti-font-size-label);
  }

  dd {
    margin: 0;
  }
}

.execution-meta {
  color: var(--ti-color-text-secondary);
  font-size: var(--ti-font-size-label);
  line-height: 1.6;
  text-align: right;
}

.fallback-list {
  margin: var(--ti-space-2) 0 0;
  padding-left: 1.25rem;
  color: var(--ti-color-text-secondary);
  font-size: var(--ti-font-size-label);
}

.status-text {
  margin-top: var(--ti-space-3);
  color: var(--ti-color-text-secondary);
  font-size: var(--ti-font-size-body);
  animation: fade-in 0.3s ease both;
}

.result-list {
  display: grid;
  gap: var(--ti-space-5);
  margin: 0;

  dt {
    margin-bottom: var(--ti-space-1);
    color: var(--ti-color-text-secondary);
    font-size: var(--ti-font-size-label);
  }

  dd {
    margin: 0;
  }
}

.uuid-break {
  word-break: break-all;
}

.answer-row {
  display: flex;
  align-items: center;
  gap: var(--ti-space-2);
}

.answer-code {
  color: var(--ti-color-accent-text);
  font-family: var(--ti-font-mono);
  font-size: 1.375rem;
  font-weight: 700;
  letter-spacing: -0.01em;
  animation: pop-in 0.4s cubic-bezier(0.18, 0.89, 0.32, 1.28) both;
}

.mono {
  font-family: var(--ti-font-mono);
  font-size: var(--ti-font-size-body);
}

.empty-text {
  color: var(--ti-color-text-secondary);
  font-size: var(--ti-font-size-body);
}

.spin {
  display: inline-block;
  animation: spin 1.2s linear infinite;
}

.reveal {
  animation: card-in 0.35s ease both;
}

@keyframes card-in {
  from {
    opacity: 0;
    transform: translateY(10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

@keyframes pop-in {
  from {
    opacity: 0;
    transform: scale(0.85);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
