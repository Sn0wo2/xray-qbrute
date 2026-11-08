<script setup lang="ts">
import { Ban, Check, Circle, Copy, Flag, LoaderCircle, Play, Square, TriangleAlert } from "@lucide/vue"
import {
  dateEnUS,
  enUS,
  NAlert,
  NButton,
  NFlex,
  NGi,
  NGrid,
  NIcon,
  NProgress,
  NSelect,
  NSkeleton,
  NSpin,
  NStatistic,
  NTag,
  NText,
  TiButton,
  TiCard,
  type TiColorMode,
  TiInput,
  TiProvider,
  TiTag,
  TiThemeSwitch,
} from "@teal-ink/vue"
import { useClipboard } from "@vueuse/core"
import { type Component, computed, reactive, ref, useTemplateRef, watch } from "vue"
import { useSearchWorker } from "@/composables/useSearchWorker"
import { WASM_THREADS_ERROR, wasmThreadsAvailable } from "@/lib/wasm-runtime"
import type { Backend, SearchState } from "@/types/worker"
import { svgDataUrl } from "@/utils/svg"

const storedMode = localStorage.getItem("ti-color-mode")
const mode = ref<TiColorMode>(
  storedMode === "light" || storedMode === "dark" || storedMode === "system" ? storedMode : "system",
)
watch(mode, (value) => {
  localStorage.setItem("ti-color-mode", value)
})

const {
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
} = useSearchWorker()
const { copy, copied } = useClipboard({ copiedDuring: 1.5 })

const form = reactive({
  commit: "90243a74",
  node: "",
  difficulty: "33",
  maxIndex: (1n << 58n).toString(),
  backend: "auto" as Backend,
  threads: String(detectedThreads),
})
const formError = ref("")
const formElement = useTemplateRef<HTMLFormElement>("formElement")

const themeIcons = {
  light: svgDataUrl(
    '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32 1.41 1.41M2 12h2m16 0h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41"/>',
  ),
  dark: svgDataUrl('<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>'),
  system: svgDataUrl('<rect width="20" height="14" x="2" y="3" rx="2"/><path d="M8 21h8m-4-4v4"/>'),
}

const STATE_META: Record<SearchState, { label: string; type: "default" | "primary" | "error"; icon: Component }> = {
  idle: { label: "Idle", type: "default", icon: Circle },
  preparing: { label: "Preparing", type: "primary", icon: Play },
  running: { label: "Running", type: "primary", icon: LoaderCircle },
  found: { label: "Found", type: "primary", icon: Flag },
  stopped: { label: "Stopped", type: "default", icon: Square },
  exhausted: { label: "Exhausted", type: "default", icon: Ban },
  error: { label: "Error", type: "error", icon: TriangleAlert },
}

const rate = computed(() => {
  const elapsedMs = metrics.value.elapsedMs
  return elapsedMs > 0 ? `${(Number(metrics.value.evaluated) / elapsedMs / 1000).toFixed(1)} M/s` : "-"
})

const expectedDuration = computed(() => {
  const seconds = expectedSeconds.value
  if (seconds <= 0) return "-"
  for (const [unit, size] of [
    ["d", 86_400],
    ["h", 3600],
    ["min", 60],
    ["s", 1],
  ] as const) {
    if (seconds >= size) return `${(seconds / size).toFixed(1)} ${unit}`
  }
  return `${(seconds * 1000).toFixed(0)} ms`
})

const backendChain = computed(() => {
  const acceleratedFailure = fallbacks.value.find((reason) => reason.startsWith("accelerated WASM"))
  const keys: Backend[] = form.backend === "auto" ? ["webgpu", "wasm-simd", "scalar"] : [form.backend]
  return keys.map((key) => {
    const reason =
      (key === "webgpu" || key === "wasm-simd" ? acceleratedFailure : undefined) ??
      fallbacks.value.find((fallback) => fallback.startsWith(key))
    return {
      key,
      label: { auto: "Auto", webgpu: "WebGPU", "wasm-simd": "WASM SIMD128", scalar: "Scalar WASM" }[key],
      tone: reason ? ("error" as const) : activeBackend.value === key ? ("accent" as const) : ("neutral" as const),
      reason,
    }
  })
})

watch(
  () => form.backend,
  () => {
    fallbacks.value = []
    activeBackend.value = "No backend"
  },
)

function onSubmit() {
  formError.value = ""
  for (const input of formElement.value?.querySelectorAll<HTMLInputElement>("input") ?? []) {
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
  if ((form.backend === "wasm-simd" || form.backend === "webgpu") && !simdSupported) {
    formError.value =
      form.backend === "wasm-simd"
        ? "This browser does not support WASM SIMD128."
        : "The current WebGPU artifact also requires WASM SIMD128 support."
    return
  }

  const maxIndex = BigInt(form.maxIndex)
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
          <NText depth="3" tag="p" class="app-subtitle">
            Brute-force Xray-core Telegram group question 26/7/14
          </NText>
        </div>
        <TiThemeSwitch
          label="Color mode"
          :labels="{ light: 'Light', dark: 'Dark', system: 'System' }"
          :icons="themeIcons"
        />
      </header>

      <main class="app-main">
        <form ref="formElement" @submit.prevent="onSubmit">
          <TiCard class="reveal">
            <template #header>
              <span class="section-overline">01 / input</span>
              <div class="section-title">Configuration</div>
            </template>

            <NGrid cols="1 s:2" :x-gap="20" :y-gap="20" responsive="screen" item-responsive>
              <NGi>
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
              </NGi>
              <NGi>
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
              </NGi>
              <NGi>
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
              </NGi>
              <NGi>
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
              </NGi>
              <NGi span="1 s:2">
                <NFlex vertical :size="8">
                  <span class="field-label">Backend</span>
                  <NSelect
                    :value="form.backend"
                    :options="[
                      { label: 'Auto · WebGPU -> WASM SIMD128', value: 'auto' },
                      { label: 'WebGPU', value: 'webgpu' },
                      { label: 'WASM SIMD128', value: 'wasm-simd' },
                      { label: 'Scalar WASM', value: 'scalar' },
                    ]"
                    :disabled="running"
                    @update:value="form.backend = $event as Backend"
                  />
                  <NText depth="3" size="small">Auto tries WebGPU, falls back to SIMD</NText>
                </NFlex>
              </NGi>
              <NGi span="1 s:2">
                <TiInput
                  v-model="form.threads"
                  label="Threads"
                  :hint="`Required · Recommended: ${detectedThreads} threads`"
                  :maxlength="3"
                  inputmode="numeric"
                  pattern="([0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-6])"
                  required
                  :disabled="running"
                />
              </NGi>
            </NGrid>

            <NAlert v-if="formError" class="form-error" type="error" :title="formError" />

            <template #footer>
              <NFlex :size="12">
                <TiButton v-if="!running" variant="primary" type="submit">
                  <template #icon>
                    <NIcon aria-hidden="true"><Play /></NIcon>
                  </template>
                  Run
                </TiButton>
                <NButton v-else secondary type="error" @click="stop">
                  <template #icon>
                    <NIcon aria-hidden="true"><Square /></NIcon>
                  </template>
                  Stop
                </NButton>
              </NFlex>
            </template>
          </TiCard>
        </form>

        <TiCard class="reveal">
          <template #header>
            <span class="section-overline">02 / execution</span>
            <NTag :type="STATE_META[state].type" size="small" round>
              <template #icon>
                <NSpin v-if="state === 'preparing' || state === 'running'" :size="12" />
                <NIcon v-else aria-hidden="true">
                  <component :is="STATE_META[state].icon" />
                </NIcon>
              </template>
              {{ STATE_META[state].label }}
            </NTag>
          </template>
          <template #header-extra>
            <NFlex vertical :size="2" align="flex-end">
              <NText depth="3" size="small">{{ activeBackend }}</NText>
              <NText depth="3" size="small">{{ device }}</NText>
            </NFlex>
          </template>

          <NFlex vertical :size="16">
            <NFlex align="center" justify="space-between">
              <span>P (hit in range)</span>
              <span class="mono">{{ progressPercent.toFixed(2) }}%</span>
            </NFlex>
            <NProgress type="line" :percentage="progressPercent" :show-indicator="false" :height="8" :border-radius="4" />

            <NGrid cols="1 s:2" :x-gap="20" :y-gap="16" responsive="screen">
              <NStatistic label="Evaluated" tabular-nums>
                <span class="mono">{{ metrics.evaluated.toLocaleString() }}</span>
              </NStatistic>
              <NStatistic label="Elapsed" tabular-nums>
                <span class="mono">{{ (metrics.elapsedMs / 1000).toFixed(2) }} s</span>
              </NStatistic>
              <NStatistic label="Rate" tabular-nums>
                <span class="mono">{{ rate }}</span>
              </NStatistic>
              <NStatistic label="Expected" tabular-nums>
                <span class="mono">{{ expectedDuration }}</span>
              </NStatistic>
            </NGrid>

            <div>
              <span class="section-overline">Backend chain</span>
              <NFlex vertical :size="8">
                <NFlex v-for="item in backendChain" :key="item.key" align="center" :size="12">
                  <TiTag :tone="item.tone" size="small">{{ item.label }}</TiTag>
                  <NText v-if="item.reason" depth="3" size="small">{{ item.reason }}</NText>
                </NFlex>
              </NFlex>
            </div>

            <NText depth="3" role="status">{{ runtimeMessage }}</NText>
          </NFlex>
        </TiCard>

        <TiCard class="reveal">
          <template #header>
            <span class="section-overline">03 / result</span>
            <div class="section-title">
              <template v-if="result">Match found</template>
              <template v-else-if="state === 'stopped'">Search stopped</template>
              <template v-else-if="state === 'exhausted'">Range exhausted</template>
              <template v-else>No result yet</template>
            </div>
          </template>

          <NFlex v-if="result" vertical :size="16">
            <div>
              <NText depth="3" size="small" tag="div">UUID</NText>
              <div class="mono uuid-break">{{ result.uuid }}</div>
            </div>
            <div>
              <NText depth="3" size="small" tag="div">Answer</NText>
              <NFlex align="center" :size="8">
                <code class="answer-code">/answer {{ result.uuid }}</code>
                <TiButton
                  variant="text"
                  size="small"
                  aria-label="Copy answer"
                  @click="result && copy(`/answer ${result.uuid}`)"
                >
                  <template #icon>
                    <NIcon aria-hidden="true">
                      <Check v-if="copied" />
                      <Copy v-else />
                    </NIcon>
                  </template>
                </TiButton>
              </NFlex>
            </div>
            <div>
              <NText depth="3" size="small" tag="div">Hash prefix</NText>
              <div class="mono">{{ result.hash }}</div>
            </div>
            <div>
              <NText depth="3" size="small" tag="div">Index</NText>
              <div class="mono">{{ result.index.toLocaleString() }}</div>
            </div>
          </NFlex>
          <NText v-else-if="state === 'stopped'" depth="3" tag="p">Partial results retained above.</NText>
          <NText v-else-if="state === 'exhausted'" depth="3" tag="p">No match in range.</NText>
          <NSkeleton v-else-if="state === 'preparing' || state === 'running'" text :repeat="4" animated />
          <NText v-else depth="3" tag="p">Awaiting search.</NText>
        </TiCard>
      </main>

      <footer class="app-footer">
        <a href="https://github.com/Sn0wo2/xray-qbrute" target="_blank" rel="noopener noreferrer">
          <NIcon aria-hidden="true">
            <svg viewBox="0 0 16 16" fill="currentColor">
              <path
                d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z"
              />
            </svg>
          </NIcon>
          <strong>Star xray-qbrute if helpful~</strong>
        </a>
        <span aria-hidden="true">|</span>
        <a href="https://github.com/Sn0wo2/teal-ink-design" target="_blank" rel="noopener noreferrer">
          <em>Inspired</em> by
          <img
            class="brand-logo"
            src="https://raw.githubusercontent.com/Sn0wo2/teal-ink-design/refs/heads/main/examples/showcase/public/brand/logo-horizontal-en.svg"
            alt="Teal Ink"
          />
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
}

.app-main {
  display: flex;
  flex-direction: column;
  gap: var(--ti-space-6);
  padding: var(--ti-space-6) var(--ti-space-8);
}

.app-footer {
  margin-top: auto;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: center;
  gap: var(--ti-space-2);
  padding: var(--ti-space-6) var(--ti-space-8) var(--ti-space-8);
  border-top: 1px solid var(--ti-color-border);
  color: var(--ti-color-text-secondary);

  a {
    display: inline-flex;
    align-items: center;
    gap: var(--ti-space-2);
    color: inherit;
    text-decoration: none;
    transition: color var(--ti-motion-duration) var(--ti-motion-easing);

    &:hover {
      color: var(--ti-color-accent-text);
    }
  }

  .brand-logo {
    height: 1.2em;
    width: auto;
  }
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

.field-label {
  color: var(--ti-color-text);
  font-size: var(--ti-font-size-label);
  font-weight: 500;
}

.form-error {
  margin-top: var(--ti-space-4);
}

.mono {
  font-family: var(--ti-font-mono);
  font-size: var(--ti-font-size-body);
}

.uuid-break {
  word-break: break-all;
}

.answer-code {
  color: var(--ti-color-accent-text);
  font-family: var(--ti-font-mono);
  font-size: 1.375rem;
  font-weight: 700;
  letter-spacing: -0.01em;
  animation: pop-in 0.4s cubic-bezier(0.18, 0.89, 0.32, 1.28) both;
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
</style>
