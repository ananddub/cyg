// Small display helpers shared by the console views. An unset column arrives as
// the epoch or an empty string, and the console shows an em dash for it — never a
// zero, which would read as a real measurement.

const DASH = "—"

export function formatTime(value: string | null | undefined): string {
  if (value === null || value === undefined || value === "") return DASH
  const date = new Date(value)
  // The epoch is how the schema spells "unset".
  if (Number.isNaN(date.getTime()) || date.getTime() === 0) return DASH
  return date.toLocaleString()
}

export function dash(value: string | null | undefined): string {
  return value === null || value === undefined || value === "" ? DASH : value
}

export function shortId(value: string): string {
  return value.length > 14 ? `${value.slice(0, 8)}…` : value
}

export function num(value: number | null | undefined): string {
  return value === null || value === undefined ? DASH : value.toLocaleString()
}

export { DASH }
