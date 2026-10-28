/**
 * Tree-sitter grammar for the CYG deterministic simulation schema language.
 *
 * The grammar mirrors Tools~/CYG.Dsl.Core (lexer + recursive-descent parser).
 * It intentionally accepts a few editor-friendly extras (a trailing comma in
 * enum bodies) so incomplete files still produce a useful syntax tree.
 */
module.exports = grammar({
  name: 'cyg',

  extras: $ => [
    /\s/,
    $.line_comment,
    $.block_comment,
  ],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($._declaration),

    _declaration: $ => choice(
      $.namespace_declaration,
      $.asset_declaration,
      $.struct_declaration,
      $.component_declaration,
      $.schema_declaration,
      $.enum_declaration,
      $.flags_declaration,
      $.union_declaration,
      $.input_declaration,
      $.global_declaration,
      $.event_declaration,
      $.signal_declaration,
    ),

    namespace_declaration: $ => seq(
      'namespace',
      field('name', $.qualified_name),
      ';',
    ),

    qualified_name: $ => prec.right(seq(
      $.identifier,
      repeat1(seq('.', $.identifier)),
    )),

    asset_declaration: $ => seq(
      'asset',
      field('name', $.identifier),
      ';',
    ),

    struct_declaration: $ => seq(
      'struct',
      field('name', $.identifier),
      field('body', $.field_block),
    ),

    component_declaration: $ => seq(
      'component',
      field('name', $.identifier),
      optional(seq(':', field('id', $.number))),
      field('body', $.field_block),
    ),

    schema_declaration: $ => seq(
      'schema',
      field('name', $.identifier),
      optional(seq(':', field('version', $.number))),
      field('body', $.field_block),
    ),

    enum_declaration: $ => seq(
      'enum',
      field('name', $.identifier),
      optional(seq(':', field('underlying', $.identifier))),
      field('body', $.enum_body),
    ),

    flags_declaration: $ => seq(
      'flags',
      field('name', $.identifier),
      optional(seq(':', field('underlying', $.identifier))),
      field('body', $.enum_body),
    ),

    union_declaration: $ => seq(
      'union',
      field('name', $.identifier),
      field('body', $.union_body),
    ),

    union_body: $ => seq(
      '{',
      repeat($.union_variant),
      '}',
    ),

    union_variant: $ => seq(
      'struct',
      field('name', $.identifier),
      field('body', $.field_block),
    ),

    input_declaration: $ => seq(
      'input',
      field('body', $.input_body),
    ),

    input_body: $ => seq(
      '{',
      repeat(choice($.button_field, $.field)),
      '}',
    ),

    button_field: $ => seq(
      'button',
      field('name', $.identifier),
      ';',
    ),

    global_declaration: $ => seq(
      'global',
      field('body', $.field_block),
    ),

    event_declaration: $ => seq(
      choice('event', 'synced_event'),
      field('name', $.identifier),
      optional(seq(':', field('id', $.number))),
      field('body', $.field_block),
    ),

    signal_declaration: $ => seq(
      'signal',
      field('name', $.identifier),
      '(',
      optional($.parameter_list),
      ')',
      ';',
    ),

    parameter_list: $ => seq(
      $.parameter,
      repeat(seq(',', $.parameter)),
      optional(','),
    ),

    parameter: $ => seq(
      field('type', $._type),
      field('name', $.identifier),
    ),

    field_block: $ => seq(
      '{',
      repeat($.field),
      '}',
    ),

    field: $ => seq(
      field('type', $._type),
      field('name', $.identifier),
      ';',
    ),

    enum_body: $ => seq(
      '{',
      optional(seq(
        $.enum_member,
        repeat(seq(',', $.enum_member)),
        optional(','),
      )),
      '}',
    ),

    enum_member: $ => seq(
      field('name', $.identifier),
      optional(seq('=', field('value', $.number))),
    ),

    _type: $ => choice(
      $.primitive_type,
      $.builtin_type,
      $.asset_ref_type,
      $.string_type,
      $.array_type,
      $.list_type,
      $.dictionary_type,
      $.qualified_type,
      $.named_type,
    ),

    primitive_type: _ => choice(
      'Byte',
      'SByte',
      'Int16',
      'UInt16',
      'Int32',
      'UInt32',
      'Int64',
      'UInt64',
      'Boolean',
      'FP',
    ),

    builtin_type: _ => choice(
      'FPVector2',
      'FPVector3',
      'FPQuaternion',
      'FPMatrix4x4',
      'FPAABB',
      'FPBounds',
      'entity_ref',
      'player_ref',
    ),

    asset_ref_type: $ => seq(
      'asset_ref',
      '<',
      field('asset', $.identifier),
      '>',
    ),

    string_type: $ => seq(
      'QString',
      '<',
      field('capacity', $.number),
      '>',
    ),

    array_type: $ => seq(
      'array',
      '<',
      field('element', $._type),
      '>',
      '[',
      field('capacity', $.number),
      ']',
    ),

    list_type: $ => seq(
      'list',
      '<',
      field('element', $._type),
      '>',
      '[',
      field('capacity', $.number),
      ']',
    ),

    dictionary_type: $ => seq(
      'dictionary',
      '<',
      field('key', $._type),
      ',',
      field('value', $._type),
      '>',
      '[',
      field('capacity', $.number),
      ']',
    ),

    named_type: $ => $.identifier,

    qualified_type: $ => prec.right(seq(
      $.identifier,
      repeat1(seq('.', $.identifier)),
    )),

    number: _ => token(/-?(0[xX][0-9a-fA-F]+|[0-9]+)/),

    identifier: _ => /[A-Za-z_][A-Za-z0-9_]*/,

    line_comment: _ => token(seq('//', /[^\n]*/)),

    block_comment: _ => token(seq(
      '/*',
      /[^*]*\*+([^/*][^*]*\*+)*/,
      '/',
    )),
  },
});
