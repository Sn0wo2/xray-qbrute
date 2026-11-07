import type { TiColorMode } from "@teal-ink/vue"

function svgUrl(inner: string) {
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" ` +
    `stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${inner}</svg>`
  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

export const THEME_SWITCH_LABELS: Record<TiColorMode, string> = {
  light: "Light",
  dark: "Dark",
  system: "System",
}

export const THEME_SWITCH_ICONS: Record<TiColorMode, string> = {
  light: svgUrl(
    '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32 1.41 1.41M2 12h2m16 0h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41"/>',
  ),
  dark: svgUrl('<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>'),
  system: svgUrl('<rect width="20" height="14" x="2" y="3" rx="2"/><path d="M8 21h8m-4-4v4"/>'),
}
