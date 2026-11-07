import { fileURLToPath, URL } from "node:url"
import vue from "@vitejs/plugin-vue"
import { defineConfig } from "vite"

export default defineConfig({
  base: "/",
  plugins: [
    vue(),
    {
      name: "coi-serviceworker",
      transformIndexHtml: {
        order: "pre",
        handler(html: string) {
          return html.replace("</head>", '    <script src="/coi-serviceworker.js"></script>\n  </head>')
        },
      },
      async closeBundle() {
        const { copyFileSync, mkdirSync } = await import("node:fs")
        const dest = fileURLToPath(new URL("./dist", import.meta.url))
        mkdirSync(dest, { recursive: true })
        copyFileSync(
          fileURLToPath(new URL("./node_modules/coi-serviceworker/coi-serviceworker.js", import.meta.url)),
          fileURLToPath(new URL("./dist/coi-serviceworker.js", import.meta.url)),
        )
      },
    },
  ],
  build: {
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            { name: "vue", test: /node_modules\/vue/ },
            {
              name: "teal-ink",
              test: /node_modules\/(naive-ui|@teal-ink|vueuc|vdirs|vooks|treemate|seemly|css-render|@css-render|date-fns)/,
            },
            { name: "vendor", test: /node_modules/ },
          ],
        },
      },
    },
  },
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  server: {
    headers: {
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    },
  },
})
