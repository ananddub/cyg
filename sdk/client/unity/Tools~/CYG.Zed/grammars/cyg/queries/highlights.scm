; Syntax highlighting for the CYG schema language.

(line_comment) @comment
(block_comment) @comment

[
  "namespace"
  "asset"
  "struct"
  "component"
  "schema"
  "enum"
  "flags"
  "union"
  "input"
  "global"
  "event"
  "synced_event"
  "signal"
  "button"
] @keyword

(primitive_type) @type.builtin
(builtin_type) @type.builtin

(asset_ref_type "asset_ref" @type.builtin)
(string_type "QString" @type.builtin)
(array_type "array" @type.builtin)
(list_type "list" @type.builtin)
(dictionary_type "dictionary" @type.builtin)

(named_type) @type
(qualified_type) @type

; Declared type names.
(namespace_declaration name: (qualified_name) @type)
(asset_declaration name: (identifier) @type)
(struct_declaration name: (identifier) @type)
(component_declaration name: (identifier) @type)
(schema_declaration name: (identifier) @type)
(enum_declaration name: (identifier) @type)
(flags_declaration name: (identifier) @type)
(union_declaration name: (identifier) @type)
(union_variant name: (identifier) @type)

; Enum/flags underlying integer type.
(enum_declaration underlying: (identifier) @type.builtin)
(flags_declaration underlying: (identifier) @type.builtin)

; Enum and flags members are compile-time constants.
(enum_member name: (identifier) @constant)

; Fields and input payload fields.
(field name: (identifier) @property)
(button_field name: (identifier) @property)

; Event, signal, and asset references.
(event_declaration name: (identifier) @type)
(signal_declaration name: (identifier) @function)
(asset_ref_type asset: (identifier) @type)

(parameter name: (identifier) @variable.parameter)

(number) @number

[":" "="] @operator
["."] @punctuation.delimiter
["," ";"] @punctuation.delimiter
["{" "}" "(" ")" "[" "]" "<" ">"] @punctuation.bracket
