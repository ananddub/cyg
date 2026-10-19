// The grammar behind the console's JSON colouring. It lives apart from the
// components so the view module exports components only.

/** A string that owns a colon is a key; then a value string, a literal, a
 * number, or structure. Capture group order is ROLE order. */
const TOKEN =
  /("(?:\\.|[^"\\])*")(?=\s*:)|("(?:\\.|[^"\\])*")|\b(true|false)\b|\b(null)\b|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|([{}[\],:])/g

const ROLE = [
  "text-json-key",
  "text-json-string",
  "text-json-boolean",
  "text-json-null",
  "text-json-number",
  "text-json-punct",
]

/** Above this the spans cost more than they show: a dump this size is read by
 * grepping it, not by colour. */
const LIMIT = 60_000

export interface JsonToken {
  text: string
  role: string | undefined
}

/** Cut JSON into its roles. Anything the grammar does not recognise falls
 * through unstyled, so a half-typed document still prints instead of throwing. */
export function tokenizeJson(text: string): JsonToken[] {
  if (text.length > LIMIT) return [{ text, role: undefined }]

  // A fresh scanner per run: the shared TOKEN carries a lastIndex.
  const scanner = new RegExp(TOKEN)
  const tokens: JsonToken[] = []
  let cursor = 0
  let match: RegExpExecArray | null

  while ((match = scanner.exec(text)) !== null) {
    if (match.index > cursor) tokens.push({ text: text.slice(cursor, match.index), role: undefined })
    const groups = match
    const role = ROLE.findIndex((_, index) => groups[index + 1] !== undefined)
    tokens.push({ text: match[0], role: role === -1 ? undefined : ROLE[role] })
    cursor = match.index + match[0].length
  }

  if (cursor < text.length) tokens.push({ text: text.slice(cursor), role: undefined })
  return tokens
}
