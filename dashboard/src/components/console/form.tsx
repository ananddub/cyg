// The fields the console's write dialogs are built from. One place for the input
// look, so a dialog, the login gate and the security page cannot drift apart.

import type { ReactNode } from "react"
import { useEffect, useRef, useState } from "react"
import { Search } from "lucide-react"
import CodeMirror from "@uiw/react-codemirror"
import { json } from "@codemirror/lang-json"
import { useTheme } from "@/lib/theme"
import {
  Select as SelectPrimitive,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

/** The console's one input style. */
const INPUT =
  "w-full rounded-md border border-border bg-background px-3 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"

export function Labelled({
  label,
  hint,
  children,
}: {
  label: string
  hint?: string
  children: ReactNode
}) {
  return (
    <label className="block space-y-1.5">
      <span className="block text-xs font-medium text-muted-foreground">{label}</span>
      {children}
      {hint !== undefined && <span className="block text-[11px] text-muted-foreground">{hint}</span>}
    </label>
  )
}

export function TextInput({
  label,
  hint,
  value,
  onChange,
  placeholder,
  type = "text",
  mono = false,
  autoFocus = false,
  disabled = false,
}: {
  label: string
  hint?: string
  value: string
  onChange: (value: string) => void
  placeholder?: string
  type?: "text" | "password" | "number"
  mono?: boolean
  autoFocus?: boolean
  disabled?: boolean
}) {
  return (
    <Labelled label={label} hint={hint}>
      <input
        autoFocus={autoFocus}
        disabled={disabled}
        type={type}
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
        autoComplete={type === "password" ? "new-password" : "off"}
        className={`h-9 ${mono ? "font-mono text-xs" : ""} ${INPUT}`}
      />
    </Labelled>
  )
}

export function TextArea({
  label,
  hint,
  value,
  onChange,
  rows = 4,
  placeholder,
}: {
  label: string
  hint?: string
  value: string
  onChange: (value: string) => void
  rows?: number
  placeholder?: string
}) {
  return (
    <Labelled label={label} hint={hint}>
      <textarea
        value={value}
        rows={rows}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
        className={`resize-y py-2 font-mono text-xs leading-relaxed ${INPUT}`}
      />
    </Labelled>
  )
}

/** A JSON editor field powered by CodeMirror with syntax highlighting, bracket
 * matching, code folding, and auto-formatting. */
export function JsonTextArea({
  label,
  hint,
  value,
  onChange,
  rows = 6,
  placeholder,
}: {
  label: string
  hint?: string
  value: string
  onChange: (value: string) => void
  rows?: number
  placeholder?: string
}) {
  const mode = useTheme()
  const height = `${Math.max(rows * 22, 110)}px`

  return (
    <Labelled label={label} hint={hint}>
      <div className="overflow-hidden rounded-md border border-border bg-background focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/50">
        <CodeMirror
          value={value}
          height={height}
          theme={mode === "dark" ? "dark" : "light"}
          extensions={[json()]}
          onChange={(val) => onChange(val)}
          placeholder={placeholder}
          basicSetup={{
            lineNumbers: true,
            foldGutter: true,
            dropCursor: true,
            indentOnInput: true,
            bracketMatching: true,
            closeBrackets: true,
            autocompletion: true,
            highlightActiveLine: true,
          }}
          className="text-xs font-mono"
        />
      </div>
    </Labelled>
  )
}

/** A picker. `searchable` is for the lists read from the server — an operator
 * hunting one username out of three hundred is filtering, not scrolling.
 * `hideLabel` drops the stacked label so the trigger sits level with a toolbar's
 * other controls; the value line already names the choice. */
export function Select({
  label,
  hint,
  value,
  onChange,
  options,
  searchable = false,
  hideLabel = false,
}: {
  label: string
  hint?: string
  value: string
  onChange: (value: string) => void
  options: readonly { value: string; label: string }[]
  searchable?: boolean
  hideLabel?: boolean
}) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")
  const search = useRef<HTMLInputElement>(null)

  const needle = query.trim().toLowerCase()
  const shown =
    needle === ""
      ? options
      : options.filter((option) => option.label.toLowerCase().includes(needle))

  useEffect(() => {
    if (open && searchable) search.current?.focus()
  }, [open, searchable])

  const field = (
    <SelectPrimitive
      value={value}
      items={options}
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (!next) setQuery("")
      }}
      onValueChange={(next) => onChange(next ?? "")}
    >
      {/* The trigger carries the console's input metrics — same height, radius
          and padding as a text field — so the two sit level in a grid. */}
      <SelectTrigger
        aria-label={hideLabel ? label : undefined}
        className="w-full rounded-md border-border bg-background px-3 data-[size=default]:h-9"
      >
        <SelectValue />
      </SelectTrigger>
      {/* Anchored below the trigger's left edge rather than Base UI's default
          of sliding the selected item over the trigger, which reads as the
          popup being misaligned with the field it belongs to. */}
      <SelectContent
        align="start"
        alignItemWithTrigger={false}
        header={
          searchable ? (
            /* Flush and borderless: the popup's own edge is the field's box,
               so the search reads as the top of the dropdown rather than a
               separate control dropped into it. */
            <div className="relative">
              <Search className="text-muted-foreground pointer-events-none absolute top-1/2 left-3 size-3.5 -translate-y-1/2" />
              <input
                ref={search}
                value={query}
                placeholder={`Search ${options.length}`}
                onChange={(event) => setQuery(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "ArrowDown") {
                    const first = event.currentTarget
                      .closest('[data-slot="select-content"]')
                      ?.querySelector<HTMLElement>('[data-slot="select-item"]')
                    if (first) {
                      event.preventDefault()
                      first.focus()
                    }
                  } else if (event.key === "Enter" && shown.length > 0) {
                    event.preventDefault()
                    onChange(shown[0].value)
                    setOpen(false)
                  }
                }}
                className="placeholder:text-muted-foreground h-9 w-full bg-transparent pr-3 pl-9 text-sm outline-none"
              />
            </div>
          ) : undefined
        }
      >
        {shown.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
        {searchable && shown.length === 0 && (
          <p className="text-muted-foreground px-3 py-3 text-center text-xs">
            Nothing matches “{query}”.
          </p>
        )}
      </SelectContent>
    </SelectPrimitive>
  )

  if (hideLabel) return field

  return (
    <Labelled label={label} hint={hint}>
      {field}
    </Labelled>
  )
}

export function Checkbox({
  label,
  checked,
  onChange,
}: {
  label: string
  checked: boolean
  onChange: (checked: boolean) => void
}) {
  return (
    <label className="flex items-center gap-2 text-xs text-foreground">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="size-3.5 rounded border-border accent-primary"
      />
      {label}
    </label>
  )
}

