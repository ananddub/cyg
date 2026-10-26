(namespace_declaration
  "namespace" @context
  name: (qualified_name) @name) @item

(asset_declaration
  "asset" @context
  name: (identifier) @name) @item

(struct_declaration
  "struct" @context
  name: (identifier) @name) @item

(component_declaration
  "component" @context
  name: (identifier) @name) @item

(enum_declaration
  "enum" @context
  name: (identifier) @name) @item

(flags_declaration
  "flags" @context
  name: (identifier) @name) @item

(union_declaration
  "union" @context
  name: (identifier) @name) @item

(union_variant
  "struct" @context
  name: (identifier) @name) @item

(schema_declaration
  "schema" @context
  name: (identifier) @name) @item

(event_declaration
  ["event" "synced_event"] @context
  name: (identifier) @name) @item

(signal_declaration
  "signal" @context
  name: (identifier) @name) @item
