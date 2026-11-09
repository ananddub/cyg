// Copyright (c) CYG. Hook SDK for AssemblyScript.
//
// A hook is server-side logic that runs on account events, not on a room tick.
// It is compiled to the same sandbox as a room module but has a different
// contract: one JSON document in, one JSON document out, no host imports.
//
//   import { onPurchase, HookOutput, purchaseGrant } from "./cyg-hooks"
//
//   export function on_purchase(): void {
//     const event = readPurchase()
//     if (event.sku == "vip") {
//       purchaseGrant("coins", 1000, "vip bonus")
//     }
//   }
//
// The host applies what the hook proposes through the same services the REST API
// uses, so a hook can never bypass a quota, a validation or an audit row.

// ---------------------------------------------------------------- input reading

/** The event document the host handed us, as bytes. */
// @ts-ignore: provided by the host at instantiation
@external("env", "hook_input_ptr")
declare function _hookInputPtr(): usize

// @ts-ignore: provided by the host
@external("env", "hook_input_capacity")
declare function _hookInputCapacity(): i32

/** The host-declared input length. Exported so the host can confirm we agree. */
export function hook_input_len(): i32 {
  return _hookInputCapacity()
}

/** Read the whole input document. */
function input(): string {
  const len = _hookInputCapacity()
  return String.UTF8.decodeUnsafe(_hookInputPtr(), len)
}

// --------------------------------------------------------------- output writing

class OutputWriter {
  private buf: ArrayBuffer
  private view: DataView
  private pos: i32 = 0
  private needsComma: bool = false

  constructor(capacity: i32) {
    this.buf = new ArrayBuffer(capacity)
    this.view = new DataView(this.buf)
    this.open()
  }

  private open(): void {
    this.put("{")
  }

  private put(s: string): void {
    const bytes = String.UTF8.encode(s, false)
    const src = Uint8Array.wrap(bytes)
    if (this.pos + src.length > this.buf.byteLength) return
    const dst = Uint8Array.wrap(this.buf)
    memory.copy(changetype<usize>(dst.dataStart) + this.pos, changetype<usize>(src.dataStart), src.length)
    this.pos += src.length
  }

  private key(name: string): void {
    if (this.needsComma) this.put(",")
    this.put('"')
    this.put(name)
    this.put('":')
  }

  private str(s: string): void {
    this.put('"')
    // The escaping the wire needs; a quote or backslash inside a reason would
    // otherwise produce a document the host cannot parse.
    let escaped = ""
    for (let i = 0; i < s.length; i++) {
      const c = s.charCodeAt(i)
      if (c == 34) escaped += '\\"'
      else if (c == 92) escaped += "\\\\"
      else if (c == 10) escaped += "\\n"
      else escaped += String.fromCharCode(c)
    }
    this.put(escaped)
    this.put('"')
  }

  beginArray(name: string): void {
    this.key(name)
    this.put("[")
  }

  arrayComma(): void {
    if (this.needsComma) this.put(",")
  }

  object(body: () => void): void {
    this.put("{")
    this.needsComma = false
    body()
    this.put("}")
    this.needsComma = true
  }

  num(name: string, value: i64): void {
    this.key(name)
    this.put(value.toString())
  }

  text(name: string, value: string): void {
    this.key(name)
    this.str(value)
  }

  /** Finish the document and hand it back to the host. */
  seal(): void {
    this.put("}")
    writeOutput(this.buf, this.pos)
  }
}

let _writer: OutputWriter | null = null
let _outputLen: i32 = 0

/** The host reads this after calling the hook. */
export function hook_output_len(): i32 {
  return _outputLen
}

function writeOutput(buf: ArrayBuffer, len: i32): void {
  _outputLen = len
  // @ts-ignore: provided by the host
  _hookWriteResult(changetype<usize>(buf), len)
}

// @ts-ignore: provided by the host
@external("env", "hook_write_result")
declare function _hookWriteResult(ptr: usize, len: i32): void

/** Start building the answer document. */
export function output(): OutputWriter {
  if (_writer === null) _writer = new OutputWriter(_hookInputCapacity())
  return _writer as OutputWriter
}

// ------------------------------------------------------------ answer vocabulary

/** Grant virtual currency. */
export function purchaseGrant(currency: string, amount: i64, reason: string = ""): void {
  const w = output()
  w.beginArray("grant_currency")
  w.object(() => {
    w.text("currency", currency)
    w.num("amount", amount)
    w.text("reason", reason)
  })
}

/** Grant an inventory item. */
export function grantItem(itemId: string, quantity: i64 = 1): void {
  const w = output()
  w.beginArray("grant_items")
  w.object(() => {
    w.text("item_id", itemId)
    w.num("quantity", quantity)
  })
}

/** Send a notification to the player. */
export function notify(subject: string, body: string = ""): void {
  const w = output()
  w.beginArray("notify")
  w.object(() => {
    w.text("subject", subject)
    w.text("body", body)
  })
}

/** Attach a badge. */
export function grantBadge(badgeId: string): void {
  const w = output()
  w.beginArray("grant_badges")
  w.put('"')
  w.put(badgeId)
  w.put('"')
}

/** Refuse the event; the server rolls the whole transaction back. */
export function reject(reason: string): void {
  output().text("reject", reason)
}

// ------------------------------------------------------------- event payloads

/** Minimal reader for the event document.
 *
 * A hook runs once per event, so a full JSON parser would be code that never
 * gets exercised beyond the handful of fields a rule needs. This finds a
 * top-level field by name and returns its raw value. */
function field(name: string): string | null {
  const doc = input()
  const needle = '"' + name + '":'
  const at = doc.indexOf(needle)
  if (at < 0) return null
  let i = at + needle.length
  while (i < doc.length && doc.charCodeAt(i) == 32) i++
  if (i >= doc.length) return null
  const quote = doc.charCodeAt(i) == 34
  if (quote) i++
  const start = i
  if (quote) {
    while (i < doc.length && doc.charCodeAt(i) != 34) i++
    return doc.substring(start, i)
  }
  while (i < doc.length) {
    const c = doc.charCodeAt(i)
    if (c == 44 || c == 125) break
    i++
  }
  return doc.substring(start, i)
}

/** A string field of the event payload. */
export function payloadString(name: string): string {
  const v = field(name)
  return v === null ? "" : v
}

/** An integer field of the event payload. */
export function payloadInt(name: string): i64 {
  const v = field(name)
  if (v === null) return 0
  return i64.parse(v)
}

/** The player the event concerns. */
export function playerId(): string {
  const v = field("player_id")
  return v === null ? "" : v
}

/** Which event is being delivered, e.g. `purchase`. */
export function eventName(): string {
  const v = field("event")
  return v === null ? "" : v
}
