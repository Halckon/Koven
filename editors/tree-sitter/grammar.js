const PREC = {
  ASSIGNMENT: 1,
  OR: 2,
  AND: 3,
  EQUALITY: 4,
  COMPARISON: 5,
  MEMBERSHIP: 6,
  ELVIS: 7,
  TO: 8,
  RANGE: 9,
  ADDITIVE: 10,
  MULTIPLICATIVE: 11,
  CAST: 12,
  PREFIX: 13,
  POSTFIX: 14,
};

module.exports = grammar({
  name: "koven",

  word: ($) => $.identifier,

  externals: ($) => [$.identifier, $._destructuring_identifier],

  extras: ($) => [/[ \t\r\f]/, $.line_comment, $.block_comment],

  conflicts: ($) => [
    [$.binary_expression, $.prefix_expression, $.call_expression],
    [$.binary_expression, $.call_expression],
    [$._statement, $._lambda_element],
    [$.block, $.lambda_expression],
    [$.qualified_name, $._primary_expression],
    [$.named_argument_prefix, $._primary_expression],
    [$._file_content],
    [$._imports_and_declarations],
    [$._declarations],
  ],

  rules: {
    source_file: ($) =>
      choice(
        optional($._newline),
        seq(
          optional($._newline),
          $._file_content,
          optional($._file_separator),
        ),
      ),

    _file_content: ($) =>
      choice(
        seq(
          $.package_directive,
          optional(seq($._file_separator, $._imports_and_declarations)),
        ),
        $._imports_and_declarations,
      ),

    _imports_and_declarations: ($) =>
      choice(
        seq(
          $.import_directive,
          repeat(seq($._file_separator, $.import_directive)),
          optional(seq($._file_separator, $._declarations)),
        ),
        $._declarations,
      ),

    _declarations: ($) =>
      seq(
        $._declaration,
        repeat(seq($._file_separator, $._declaration)),
      ),

    package_directive: ($) => seq("package", $.qualified_name),

    import_directive: ($) =>
      seq(
        "import",
        field("target", $.import_target),
        optional(seq("as", field("alias", $.identifier))),
      ),

    import_target: ($) =>
      seq(
        $.identifier,
        repeat(seq(".", $.identifier)),
        optional(".*"),
      ),

    qualified_name: ($) =>
      prec.left(seq($.identifier, repeat(seq(".", $.identifier)))),

    _declaration: ($) =>
      seq(
        optional($.visibility_modifier),
        choice(
          $.variable_declaration,
          $.constant_declaration,
          $.function_declaration,
          $.classifier_declaration,
        ),
      ),

    visibility_modifier: () => choice("public", "internal", "private"),

    variable_declaration: ($) =>
      seq(
        field("kind", choice("val", "var")),
        field("name", $.identifier),
        optional($.type_annotation),
        "=",
        field("value", $.expression),
      ),

    constant_declaration: ($) =>
      seq(
        "const",
        "val",
        field("name", $.identifier),
        optional($.type_annotation),
        "=",
        field("value", $.expression),
      ),

    type_annotation: ($) => seq(":", field("type", $._type)),

    function_declaration: ($) =>
      seq(
        "fun",
        optional($.type_parameter_list),
        field("name", $.identifier),
        $.value_parameter_list,
        choice(
          seq(
            ":",
            field("return_type", $._type),
            optional(choice($.expression_body, $.block)),
          ),
          optional($.block),
        ),
      ),

    expression_body: ($) => seq("=", field("value", $.expression)),

    type_parameter_list: ($) =>
      seq("<", commaSep1($.type_parameter), ">"),

    type_parameter: ($) =>
      seq(
        field("name", $.identifier),
        optional(seq(":", field("bound", $._type))),
      ),

    value_parameter_list: ($) =>
      seq("(", optional(commaSep1($.value_parameter)), ")"),

    value_parameter: ($) =>
      seq(
        optional($.parameter_mode),
        field("name", $.identifier),
        ":",
        field("type", $._type),
      ),

    parameter_mode: () => choice("own", "borrow", "inout"),

    classifier_declaration: ($) =>
      choice(
        $.value_class_declaration,
        $.class_declaration,
        $.interface_declaration,
        $.enum_class_declaration,
        $.object_declaration,
      ),

    value_class_declaration: ($) =>
      seq(
        "value",
        "class",
        field("name", $.identifier),
        optional($.type_parameter_list),
        $.value_primary_constructor,
        optional($.supertype_list),
        optional($.class_body),
      ),

    class_declaration: ($) =>
      seq(
        "class",
        field("name", $.identifier),
        optional($.type_parameter_list),
        optional($.class_primary_constructor),
        optional($.supertype_list),
        optional($.class_body),
      ),

    interface_declaration: ($) =>
      seq(
        "interface",
        field("name", $.identifier),
        optional($.type_parameter_list),
        optional($.supertype_list),
        optional($.interface_body),
      ),

    enum_class_declaration: ($) =>
      seq(
        "enum",
        "class",
        field("name", $.identifier),
        optional($.type_parameter_list),
        optional($.supertype_list),
        $.enum_body,
      ),

    object_declaration: ($) =>
      seq(
        "object",
        field("name", $.identifier),
        optional($.supertype_list),
        optional($.object_body),
      ),

    value_primary_constructor: ($) => seq("(", commaSep1($.class_field), ")"),

    class_primary_constructor: ($) =>
      seq("(", optional(commaSep1($.class_field)), ")"),

    class_field: ($) =>
      seq(
        optional($.visibility_modifier),
        field("kind", choice("val", "var")),
        field("name", $.identifier),
        ":",
        field("type", $._type),
      ),

    supertype_list: ($) => seq(":", commaSep1($.supertype_entry)),

    supertype_entry: ($) =>
      seq(
        field("type", $._type),
        optional(seq(alias("by", $.delegation_keyword), field("delegate", $.identifier))),
      ),

    class_body: ($) =>
      classBody($, choice($.method_declaration, $.companion_object)),

    interface_body: ($) =>
      classBody($, choice($.interface_method_declaration, $.companion_object)),

    object_body: ($) =>
      classBody($, choice($.method_declaration, $._object_constant_declaration)),

    method_declaration: ($) =>
      seq(
        optional($.visibility_modifier),
        optional("override"),
        optional($.parameter_mode),
        $.function_declaration,
      ),

    interface_method_declaration: ($) =>
      seq(optional("public"), optional($.parameter_mode), $.function_declaration),

    _object_constant_declaration: ($) =>
      seq(optional($.visibility_modifier), $.constant_declaration),

    companion_object: ($) =>
      seq(
        optional($.visibility_modifier),
        "companion",
        "object",
        $.companion_body,
      ),

    companion_body: ($) =>
      classBody(
        $,
        seq(
          optional($.visibility_modifier),
          choice($.constant_declaration, $.function_declaration),
        ),
      ),

    enum_body: ($) =>
      seq(
        "{",
        optional($._newline),
        $.enum_variant,
        repeat(
          seq(",", optional($._newline), $.enum_variant),
        ),
        choice(
          seq(optional($._newline), "}"),
          seq(
            optional($._newline),
            ";",
            repeat($._member_separator),
            $.enum_member,
            repeat(choice($._member_separator, $.enum_member)),
            "}",
          ),
        ),
      ),

    enum_variant: ($) =>
      seq(
        field("name", $.identifier),
        optional(seq("(", commaSep1($.enum_variant_parameter), ")")),
      ),

    enum_variant_parameter: ($) =>
      seq(field("name", $.identifier), ":", field("type", $._type)),

    enum_member: ($) =>
      choice($.method_declaration, $.companion_object),

    block: ($) =>
      prec(1, seq("{", repeat(choice($._newline, $._statement)), "}")),

    _statement: ($) =>
      choice(
        $.local_variable_statement,
        $.local_destructuring_statement,
        $.while_statement,
        $.for_statement,
        $.loop_statement,
        $.block,
        $.expression_statement,
      ),

    local_variable_statement: ($) => $.variable_declaration,

    local_destructuring_statement: ($) =>
      seq(
        "val",
        "(",
        commaSep1(field("binding", alias($._destructuring_identifier, $.identifier))),
        ")",
        "=",
        field("value", $.expression),
      ),

    expression_statement: ($) => $.expression,

    while_statement: ($) =>
      seq("while", "(", field("condition", $.expression), ")", $.block),

    for_statement: ($) =>
      seq(
        "for",
        "(",
        field("binding", $.for_binding),
        "in",
        field("source", $.expression),
        ")",
        $.block,
      ),

    for_binding: ($) =>
      choice(
        $.identifier,
        seq("(", commaSep1(field("name", $.identifier)), ")"),
      ),

    loop_statement: ($) => seq("loop", $.block),

    expression: ($) =>
      choice(
        $.assignment_expression,
        $.binary_expression,
        $.cast_expression,
        $.prefix_expression,
        $.call_expression,
        $.index_expression,
        $.member_expression,
        $.postfix_expression,
        $.callable_reference,
        $._primary_expression,
      ),

    assignment_expression: ($) =>
      prec.right(
        PREC.ASSIGNMENT,
        seq(
          field("left", $.expression),
          field("operator", choice("=", "+=", "-=", "*=", "/=", "%=")),
          field("right", $.expression),
        ),
      ),

    binary_expression: ($) => {
      const table = [
        [PREC.OR, "||"],
        [PREC.AND, "&&"],
        [PREC.EQUALITY, choice("==", "!=")],
        [PREC.COMPARISON, choice("<", ">", "<=", ">=" )],
        [PREC.MEMBERSHIP, choice("in", "!in")],
        [PREC.ELVIS, "?:"],
        [PREC.TO, alias("to", $.to_operator)],
        [PREC.RANGE, choice("..", "..<")],
        [PREC.ADDITIVE, choice("+", "-")],
        [PREC.MULTIPLICATIVE, choice("*", "/", "%")],
      ];
      return choice(
        ...table.map(([precedence, operator]) =>
          (operator === "?:" ? prec.right : prec.left)(
            precedence,
            seq(
              field("left", $.expression),
              field("operator", operator),
              field("right", $.expression),
            ),
          ),
        ),
        prec.left(
          PREC.MEMBERSHIP,
          seq(
            field("left", $.expression),
            field("operator", choice("is", "!is")),
            field("right", $._type),
          ),
        ),
      );
    },

    cast_expression: ($) =>
      prec.left(
        PREC.CAST,
        seq(
          field("value", $.expression),
          field("operator", choice("as", "as?")),
          field("type", $._type),
        ),
      ),

    prefix_expression: ($) =>
      prec.right(
        PREC.PREFIX,
        seq(field("operator", choice("!", "+", "-")), field("value", $.expression)),
      ),

    call_expression: ($) =>
      prec.left(
        PREC.POSTFIX,
        seq(
          field("function", $.expression),
          optional($.call_type_arguments),
          $.argument_list,
        ),
      ),

    call_type_arguments: ($) => seq("<", commaSep1($._type), ">"),

    argument_list: ($) => seq("(", optional(commaSep1($.call_argument)), ")"),

    call_argument: ($) =>
      seq(
        optional($.named_argument_prefix),
        optional($.argument_mode),
        field("value", $.expression),
      ),

    named_argument_prefix: ($) => seq(field("name", $.identifier), "="),

    argument_mode: () => "&",

    index_expression: ($) =>
      prec.left(
        PREC.POSTFIX,
        seq(field("value", $.expression), "[", field("index", $.expression), "]"),
      ),

    member_expression: ($) =>
      prec.left(
        PREC.POSTFIX,
        seq(
          field("value", $.expression),
          field("operator", choice(".", "?.")),
          field("name", $.identifier),
        ),
      ),

    postfix_expression: ($) =>
      prec.left(
        PREC.POSTFIX,
        seq(field("value", $.expression), field("operator", choice("!!", "?"))),
      ),

    callable_reference: ($) =>
      prec.left(
        PREC.POSTFIX,
        choice(
          seq("::", field("name", $.identifier)),
          seq(field("value", $.expression), "::", field("name", $.identifier)),
        ),
      ),

    _primary_expression: ($) =>
      choice(
        $.identifier,
        $.integer_literal,
        $.float_literal,
        $.character_literal,
        $.boolean_literal,
        $.null_literal,
        $.this_expression,
        $.grouped_expression,
        $.string_literal,
        $.lambda_expression,
        $.if_expression,
        $.when_expression,
        $.jump_expression,
        $.super_expression,
      ),

    grouped_expression: ($) => seq("(", $.expression, ")"),

    string_literal: ($) =>
      seq(
        '"',
        repeat(
          choice(
            $.string_content,
            $.escape_sequence,
            $.interpolation,
            alias(token.immediate("$"), $.string_content),
          ),
        ),
        '"',
      ),

    string_content: () => token.immediate(/[^"\\$\r\n]+/),
    escape_sequence: () => token.immediate(/\\[\\'"nrt0$]/),
    interpolation: ($) => seq(token.immediate("${"), $.expression, "}"),

    lambda_expression: ($) =>
      seq(
        optional("move"),
        "{",
        optional($.lambda_header),
        repeat(choice($._newline, $._lambda_element)),
        "}",
      ),

    lambda_header: ($) => seq(optional(commaSep1($.identifier)), "->"),

    _lambda_element: ($) =>
      choice(
        $.local_variable_statement,
        $.local_destructuring_statement,
        $.block,
        $.expression_statement,
      ),

    if_expression: ($) =>
      prec.right(seq(
        "if",
        "(",
        field("condition", $.expression),
        ")",
        field("consequence", $.control_body),
        optional(
          seq(
            "else",
            field("alternative", $.control_body),
          ),
        ),
      )),

    when_expression: ($) =>
      seq(
        "when",
        optional(seq("(", field("subject", $.expression), ")")),
        "{",
        repeat(choice($._member_separator, $.when_entry)),
        "}",
      ),

    when_entry: ($) =>
      seq(
        choice(seq(commaSep1($.when_condition)), "else"),
        "->",
        field("body", $.control_body),
      ),

    when_condition: ($) =>
      choice(
        $.expression,
        seq(choice("is", "!is"), $._type),
        seq(choice("in", "!in"), $.expression),
      ),

    control_body: ($) => choice($.expression, $.control_block),

    control_block: ($) =>
      prec(2, seq("{", repeat(choice($._newline, $._statement)), "}")),

    jump_expression: ($) =>
      choice(
        prec.right(seq("return", optional($.expression))),
        "break",
        "continue",
      ),

    super_expression: ($) =>
      seq("super", "<", field("interface", $._type), ">", ".", field("name", $.identifier)),

    _type: ($) => choice($.qualified_type, $.function_type),

    qualified_type: ($) =>
      prec.right(seq(
        field("name", $.qualified_name),
        optional($.type_arguments),
        optional("?"),
      )),

    type_arguments: ($) => seq("<", commaSep1($._type), ">"),

    function_type: ($) =>
      seq(
        optional("move"),
        "(",
        optional(commaSep1($.function_type_parameter)),
        ")",
        "->",
        field("return_type", $._type),
      ),

    function_type_parameter: ($) =>
      seq(optional($.parameter_mode), field("type", $._type)),

    integer_literal: () => token(/[0-9]+(?:L|[uU]L?)?/),
    float_literal: () => token(/[0-9]+(?:\.[0-9]+[fF]?|[fF])/),
    character_literal: () => token(/'(?:\\[\\'"nrt0]|[^'\r\n\\])'/),
    boolean_literal: () => choice("true", "false"),
    null_literal: () => "null",
    this_expression: () => "this",

    line_comment: () => token(seq("//", /[^\r\n]*/)),
    block_comment: () => token(seq("/*", /[^*]*\*+([^/*][^*]*\*+)*/, "/")),

    _newline: () => /\n+/,
    _file_separator: ($) =>
      choice($._newline, prec.right(seq(";", optional($._newline)))),
    _member_separator: ($) =>
      choice($._newline, prec.right(seq(";", optional($._newline)))),
  },
});

function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)));
}

function classBody($, member) {
  return seq(
    "{",
    repeat(choice($._member_separator, member)),
    "}",
  );
}
