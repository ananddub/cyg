// JSON as an operator reads it: structure, coloured by role. Hand-tokenised
// rather than pulling in a highlighter, because the grammar is six token types
// and the console carries no syntax-colouring dependency to grow.

import { useMemo } from "react"

import { tokenizeJson } from "@/components/console/json-highlight"

export function JsonTokens({ text, className }: { text: string; className?: string }) {
  const tokens = useMemo(() => tokenizeJson(text), [text])
  return (
    <span className={className}>
      {tokens.map((token, index) =>
        token.role === undefined ? (
          token.text
        ) : (
          <span key={index} className={token.role}>{token.text}</span>
        ),
      )}
    </span>
  )
}

/** One line of colourised JSON, for a table cell: compact form, clipped short. */
export function JsonInline({ value, max = 60 }: { value: unknown; max?: number }) {
  const text = JSON.stringify(value ?? null)
  if (text === undefined) return <span className="text-muted-foreground">—</span>
  return <JsonTokens text={text.length > max ? `${text.slice(0, max)}…` : text} className="font-mono text-json-punct" />
}
