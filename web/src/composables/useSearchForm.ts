import { reactive, ref } from 'vue'
import type { Backend, SearchRequest } from '@/types/worker'
import { wasmThreadsAvailable, WASM_THREADS_ERROR } from '@/lib/wasm-runtime'

interface SearchFormOptions {
  detectedThreads: number
  simdSupported: boolean
  start: (request: SearchRequest) => void
}

export function useSearchForm(options: SearchFormOptions) {
  const form = reactive({
    commit: '90243a74',
    node: '',
    difficulty: '33',
    maxIndex: '288230376151711744',
    backend: 'auto' as Backend,
    threads: String(options.detectedThreads),
  })
  const formError = ref('')

  function submit() {
    formError.value = ''
    const formElement = document.getElementById('search-form') as HTMLFormElement | null
    const inputs = formElement?.querySelectorAll<HTMLInputElement>('input')
    for (const input of inputs ?? []) {
      if (!input.checkValidity()) {
        input.setAttribute('aria-invalid', 'true')
        input.focus()
        formError.value = input.validationMessage
        return
      }
      input.removeAttribute('aria-invalid')
    }

    if (!wasmThreadsAvailable()) {
      formError.value = WASM_THREADS_ERROR
      return
    }
    if (form.backend === 'wasm-simd' && !options.simdSupported) {
      formError.value = 'This browser does not support WASM SIMD128.'
      return
    }
    if (form.backend === 'webgpu' && !options.simdSupported) {
      formError.value = 'The current WebGPU artifact also requires WASM SIMD128 support.'
      return
    }

    const threads = Number(form.threads)
    options.start({
      commit: Number.parseInt(form.commit, 16),
      nodeSuffix: Number.parseInt(form.node, 16),
      difficulty: Number(form.difficulty),
      maxIndex: form.maxIndex.trim() === '' ? 1n << 58n : BigInt(form.maxIndex),
      backend: form.backend,
      threads: threads === 0 ? options.detectedThreads : threads,
      debug: new URLSearchParams(location.search).has('debug'),
    })
  }

  return { form, formError, submit }
}
