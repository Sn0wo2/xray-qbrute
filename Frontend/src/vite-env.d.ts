declare module "*.vue" {
  import type { DefineComponent } from "vue"

  const component: DefineComponent<object, object, unknown>
  export default component
}

interface ViewTransition {
  finished: Promise<void>
  ready: Promise<void>
  updateCallbackDone: Promise<void>
  skipTransition: () => void
}

interface Document {
  startViewTransition?: (callback: () => void | Promise<void>) => ViewTransition
}
