// Reading the JSON fields the write dialogs carry. These live apart from the
// fields themselves so a component module exports only components: a file that
// mixes the two opts out of fast refresh for every view that imports it.

/** Parse a JSON field. An empty field is "not sent" rather than an error — the
 * request bodies drop absent keys — while anything else must parse, because a
 * write that quietly sent `null` where an operator meant a document is worse
 * than a refusal. */
export function parseJson(text: string): { value: unknown } | { error: string } {
  if (text.trim() === "") return { value: undefined }
  try {
    return { value: JSON.parse(text) }
  } catch (cause) {
    return { error: cause instanceof Error ? cause.message : String(cause) }
  }
}

/** The way a JSON field is seeded: pretty-printed where the caller has a value,
 * an empty line where it has none. */
export function jsonText(value: unknown): string {
  if (value === null || value === undefined) return ""
  return JSON.stringify(value, null, 2)
}
