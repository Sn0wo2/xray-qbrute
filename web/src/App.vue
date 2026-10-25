<script setup lang="ts">
import { ref, computed, onUnmounted } from "vue"
import ModeToggle from "@/components/ModeToggle.vue"
import { useSearchForm } from "@/composables/useSearchForm"
import { useSearchWorker } from "@/composables/useSearchWorker"
import { formatInteger, formatElapsed, formatRate, formatExpectedDuration } from "@/composables/useFormatters"
import { GLYPHS } from "@/constants/glyphs"

const {
  state, stateLabel, activeBackend, device,
  metrics, progressPercent, expectedSeconds, fallbacks, runtimeMessage,
  result, doneKind, running,
  simdSupported, detectedThreads,
  start, stop, dispose,
} = useSearchWorker()
onUnmounted(() => dispose())

const { form, formError, submit: onSubmit } = useSearchForm({
  detectedThreads,
  simdSupported,
  start,
})
const copied = ref(false)

const tagType = computed<"info" | "danger" | undefined>(() => {
  switch (state.value) {
    case "running":
    case "found": return undefined
    case "error": return "danger"
    default: return "info"
  }
})

const stateGlyph = computed(() => {
  switch (state.value) {
    case "idle": return GLYPHS.circle
    case "preparing": return GLYPHS.play
    case "running": return GLYPHS.spinner
    case "found": return GLYPHS.flag
    case "stopped": return GLYPHS.square
    case "exhausted": return GLYPHS.ban
    case "error": return GLYPHS.alertTriangle
    default: return GLYPHS.error
  }
})

async function copyAnswer() {
  if (!result.value) return
  try {
    await navigator.clipboard.writeText(`/answer ${result.value.uuid}`)
    copied.value = true
    setTimeout(() => { copied.value = false }, 1500)
  } catch (error) {
    if (!(error instanceof DOMException) && !(error instanceof TypeError)) {
      throw error
    }
  }
}
</script>

<template>
  <div class="app-layout">
    <header class="app-header">
      <div>
        <h1 class="app-title">xray-qbrute</h1>
        <p class="app-subtitle">Brute-force Xray-core Telegram group question 26/7/14</p>
      </div>
      <div class="app-header-actions">
        <ModeToggle />
      </div>
    </header>

    <main class="app-main">
      <form id="search-form" @submit.prevent="onSubmit">
        <el-card shadow="never">
          <template #header>
            <span class="section-overline">01 / input</span>
            <div class="section-title">Configuration</div>
          </template>

          <div class="field-grid">
            <div class="field">
              <span class="field-label">Commit hash</span>
              <el-input v-model="form.commit" maxlength="8" pattern="[0-9a-fA-F]{8}" required spellcheck="false" :disabled="running" />
              <span class="field-hint">Required · 8 hex characters</span>
            </div>
            <div class="field">
              <span class="field-label">Node suffix</span>
              <el-input v-model="form.node" maxlength="8" pattern="[0-9a-fA-F]{8}" required spellcheck="false" :disabled="running" />
              <span class="field-hint">Required · 8 hex characters · example: e4b3a192</span>
            </div>
            <div class="field">
              <span class="field-label">Difficulty</span>
              <el-input v-model="form.difficulty" type="number" :min="0" :max="64" :disabled="running" />
              <span class="field-hint">Required · Leading zero bits (0-64)</span>
            </div>
            <div class="field">
              <span class="field-label">Maximum index</span>
              <el-input v-model="form.maxIndex" inputmode="numeric" pattern="[0-9]+" required :disabled="running" />
              <span class="field-hint">Required · Full range (2<sup>58</sup>)</span>
            </div>
            <div class="field field-full">
              <span class="field-label">Backend</span>
              <el-select v-model="form.backend" :disabled="running">
                <el-option label="Auto · WebGPU -> WASM SIMD128" value="auto" />
                <el-option label="WebGPU" value="webgpu" />
                <el-option label="WASM SIMD128" value="wasm-simd" />
                <el-option label="Scalar WASM" value="scalar" />
              </el-select>
              <span class="field-hint">Auto tries WebGPU, falls back to SIMD</span>
            </div>
            <div class="field field-full">
              <span class="field-label">Threads</span>
              <el-input v-model="form.threads" type="number" :min="0" :max="256" required :disabled="running" />
              <span class="field-hint">Required · Recommended: {{ detectedThreads }} threads</span>
            </div>
          </div>

          <el-alert v-if="formError" class="form-error" type="error" :title="formError" :closable="false" show-icon />

          <template #footer>
            <div class="actions">
              <el-button v-if="!running" type="primary" native-type="submit">
                <span class="glyph" aria-hidden="true">{{ GLYPHS.play }}</span>
                Run
              </el-button>
              <el-button v-else type="danger" native-type="button" @click="stop">
                <span class="glyph" aria-hidden="true">{{ GLYPHS.stop }}</span>
                Stop
              </el-button>
            </div>
          </template>
        </el-card>
      </form>

      <el-card shadow="never">
        <template #header>
          <div class="card-header-row">
            <div>
              <span class="section-overline">02 / execution</span>
              <el-tag class="status" :type="tagType" size="small">
                <span class="glyph" :class="{ spin: state === 'preparing' || state === 'running' }" aria-hidden="true">{{ stateGlyph }}</span>
                {{ stateLabel }}
              </el-tag>
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
        <el-progress :percentage="progressPercent" :show-text="false" :stroke-width="8" />

        <dl class="metrics-grid">
          <div><dt>Evaluated</dt><dd class="mono">{{ formatInteger(metrics.evaluated) }}</dd></div>
          <div><dt>Elapsed</dt><dd class="mono">{{ formatElapsed(metrics.elapsedMs) }}</dd></div>
          <div><dt>Rate</dt><dd class="mono">{{ formatRate(metrics.evaluated, metrics.elapsedMs) }}</dd></div>
          <div><dt>Expected</dt><dd class="mono">{{ formatExpectedDuration(expectedSeconds) }}</dd></div>
        </dl>


        <el-alert v-if="fallbacks.length > 0" class="fallback-alert" type="info" :closable="false" show-icon>
          <template #title>Fallback trace</template>
          <ul class="fallback-list">
            <li v-for="reason in fallbacks" :key="reason">{{ reason }}</li>
          </ul>
        </el-alert>

        <p class="status-text" role="status">{{ runtimeMessage }}</p>
      </el-card>

      <el-card shadow="never">
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
              <el-button text size="small" aria-label="Copy answer" @click="copyAnswer">
                <span class="glyph" aria-hidden="true">{{ copied ? GLYPHS.check : GLYPHS.copy }}</span>
              </el-button>
            </dd>
          </div>
          <div><dt>Hash prefix</dt><dd class="mono">{{ result.hash }}</dd></div>
          <div><dt>Index</dt><dd class="mono">{{ formatInteger(result.index) }}</dd></div>
        </dl>

        <p v-else-if="doneKind === 'stopped'" class="empty-text">Partial results retained above.</p>
        <p v-else-if="doneKind === 'exhausted'" class="empty-text">No match in range.</p>
        <el-skeleton v-else-if="state === 'preparing' || state === 'running'" :rows="4" animated />
        <p v-else class="empty-text">Awaiting search.</p>
      </el-card>
    </main>
    <footer class="app-footer">
      <a href="https://github.com/Sn0wo2/xray-qbrute" target="_blank" rel="noopener noreferrer">
        <span class="glyph" aria-hidden="true">{{ GLYPHS.github }}</span>
        Star xray-qbrute if helpful~
      </a>
    </footer>
  </div>
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
  gap: 1rem;
  padding: 1.5rem 2rem 0.5rem;
}

.app-title {
  font-size: 1.5rem;
  font-weight: 700;
  line-height: 1.25;
  letter-spacing: -0.02em;
}

.app-subtitle {
  margin-top: 0.5rem;
  font-size: 0.875rem;
  line-height: 1.625;
  color: var(--el-text-color-secondary);
}

.app-header-actions {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  gap: 0.75rem;
}

.app-main {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
  padding: 1.5rem 2rem;
}

.app-footer {
  margin-top: auto;
  padding: 1.5rem 2rem 2rem;
  border-top: 1px solid var(--el-border-color);
  text-align: center;
  font-size: 0.875rem;
  color: var(--el-text-color-secondary);

  a {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    color: var(--el-text-color-secondary);
    text-decoration: none;
    transition: color 0.2s ease;

    &:hover {
      color: var(--color-accent);
    }
  }

  .glyph {
    font-size: 1rem;
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
  color: var(--el-text-color-secondary);
  font-size: 0.6875rem;
  font-weight: 500;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.status .glyph {
  margin-right: 0.1rem;
}

.section-title {
  font-size: 1.0625rem;
  font-weight: 600;
  letter-spacing: -0.01em;
}

.field-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1.25rem;

  @media (max-width: 480px) {
    grid-template-columns: 1fr;
  }
}

.field {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;

  :deep(.el-select) {
    width: 100%;
  }
}

.field-full {
  grid-column: 1 / -1;
}

.field-label {
  color: var(--el-text-color-regular);
  font-size: 0.8125rem;
  font-weight: 500;
}

.field-hint {
  color: var(--el-text-color-secondary);
  font-size: 0.75rem;
}

.form-error,
.fallback-alert {
  margin-top: 1rem;
}

.actions {
  display: flex;
  gap: 0.75rem;
}

.progress-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.625rem;
  font-size: 0.875rem;
}

.metrics-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1.25rem;
  margin-top: 1rem;

  dt {
    margin-bottom: 0.125rem;
    color: var(--el-text-color-secondary);
    font-size: 0.75rem;
  }
}

.execution-meta {
  color: var(--el-text-color-secondary);
  font-size: 0.75rem;
  line-height: 1.6;
  text-align: right;
}

.fallback-list {
  margin: 0.5rem 0 0;
  padding-left: 1.25rem;
  color: var(--el-text-color-secondary);
  font-size: 0.8125rem;
}

.status-text {
  margin-top: 0.75rem;
  color: var(--el-text-color-secondary);
  font-size: 0.875rem;
}

.result-list {
  display: grid;
  gap: 1.25rem;

  dt {
    margin-bottom: 0.25rem;
    color: var(--el-text-color-secondary);
    font-size: 0.75rem;
  }
}

.uuid-break {
  word-break: break-all;
}

.answer-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.answer-code {
  color: var(--color-accent);
  font-family: var(--font-mono);
  font-size: 1.375rem;
  font-weight: 700;
  letter-spacing: -0.01em;
}

.mono {
  font-family: var(--font-mono);
  font-size: 0.875rem;
}

.empty-text {
  color: var(--el-text-color-secondary);
  font-size: 0.875rem;
}

.spin {
  display: inline-block;
  animation: spin 1.2s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.el-card {
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

.el-progress.is-animating :deep(.el-progress-bar__inner) {
  transition: width 0.2s ease;
}

.status-text {
  animation: fade-in 0.3s ease both;
}

@keyframes fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

.answer-code {
  animation: pop-in 0.4s cubic-bezier(0.18, 0.89, 0.32, 1.28) both;
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
