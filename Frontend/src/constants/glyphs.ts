import { icons } from "@m234/nerd-fonts/icons"

const iconValues = icons as Record<string, { value: string }>

function glyph(name: string): string {
  const entry = iconValues[name]
  if (entry) return entry.value
  if (import.meta.env.DEV) {
    console.warn(`[glyphs] NF icon "${name}" not found in @m234/nerd-fonts`)
  }
  return iconValues["nf-cod-error"]?.value ?? ""
}

export const GLYPHS = {
  play: glyph("nf-fa-play"),
  stop: glyph("nf-fa-stop"),
  sun: glyph("nf-fa-sun"),
  moon: glyph("nf-fa-moon"),
  copy: glyph("nf-fa-copy"),
  check: glyph("nf-fa-check"),
  circle: glyph("nf-fa-circle_o"),
  spinner: glyph("nf-fa-spinner"),
  flag: glyph("nf-fa-flag_checkered"),
  square: glyph("nf-fa-stop"),
  ban: glyph("nf-fa-ban"),
  alertTriangle: glyph("nf-fa-exclamation_triangle"),
  github: glyph("nf-fa-github"),
  error: glyph("nf-cod-error"),
} as const
