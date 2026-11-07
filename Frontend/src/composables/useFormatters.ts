export const integerFormatter = new Intl.NumberFormat()

export function formatRate(count: bigint | number, milliseconds: number): string {
  if (milliseconds <= 0) return "-"
  const n = typeof count === "bigint" ? Number(count) : count
  return `${(n / milliseconds / 1000).toFixed(1)} M/s`
}

export function formatExpectedDuration(seconds: number): string {
  if (seconds <= 0) return "-"
  if (seconds < 1) return `${(seconds * 1000).toFixed(0)} ms`
  if (seconds < 60) return `${seconds.toFixed(1)} s`
  if (seconds < 3600) return `${(seconds / 60).toFixed(1)} min`
  if (seconds < 86_400) return `${(seconds / 3600).toFixed(1)} h`
  return `${(seconds / 86_400).toFixed(1)} d`
}
