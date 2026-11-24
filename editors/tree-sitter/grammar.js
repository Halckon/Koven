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
  BITWISE_OR: 10,
  BITWISE_XOR: 11,
  BITWISE_AND: 12,
  SHIFT: 13,
  ADDITIVE: 14,
  MULTIPLICATIVE: 15,
  CAST: 16,
  PREFIX: 17,
  POSTFIX: 18,
};

module.exports = grammar({
  name: "koven",

  word: ($) => $.identifier,

  externals: ($) => [
    $.identifier, $._destructuring_identifier,
    $._own, $._borrow, $._inout, $._loop, $._block,
    $._control_lbrace, $._lambda_lbrace, $._same_line_trailing_lbrace, $._crossline_lambda_nl,
    $._move_lambda, $._move_type,
    $._to, $._by, $._in, $._is, $._as, $._not_in, $._not_is,
    $._shl, $._shr, $._ushr, $._bit_and, $._bit_xor, $._bit_or,
  ],

  extras: ($) => [/[ \t]/, $.line_comment, $.block_comment],

  conflicts: ($) => [
    [$._prefix_operand, $.call_expression],
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
        optional(seq(alias($._as, "as"), field("alias", $.identifier))),
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

    parameter_mode: ($) => choice(alias($._own, "own"), alias($._borrow, "borrow"), alias($._inout, "inout")),

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
        optional(seq(alias($._by, $.delegation_keyword), field("delegate", $.identifier))),
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
      prec(1, seq("{", repeat(choice($._newline, ";", $._statement)), "}")),

    _statement: ($) =>
      choice(
        $.local_variable_statement,
        $.local_destructuring_statement,
        $.while_statement,
        $.for_statement,
        $.loop_statement,
        $.nested_block_statement,
        $.expression_statement,
      ),

    nested_block_statement: ($) => seq(alias($._block, "block"), $.block),

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
        alias($._in, "in"),
        field("source", $.expression),
        ")",
        $.block,
      ),

    for_binding: ($) =>
      choice(
        $.identifier,
        seq("(", commaSep1(field("name", $.identifier)), ")"),
      ),

    loop_statement: ($) => seq(alias($._loop, "loop"), $.block),

    expression: ($) =>
      choice(
        $.assignment_expression,
        $.binary_expression,
        alias($._equality_binary, $.binary_expression),
        $._expr_above_equality,
      ),

    _expr_above_equality: ($) =>
      choice(
        alias($._comparison_binary, $.binary_expression),
        $._expr_above_comparison,
      ),

    _expr_above_comparison: ($) =>
      choice(
        alias($._membership_binary, $.binary_expression),
        $._expr_above_membership,
      ),

    _expr_above_membership: ($) =>
      choice(
        alias($._elvis_binary, $.binary_expression),
        $._expr_above_elvis,
      ),

    _expr_above_elvis: ($) =>
      choice(
        alias($._to_binary, $.binary_expression),
        $._expr_above_to,
      ),

    _expr_above_to: ($) =>
      choice(
        alias($._range_binary, $.binary_expression),
        $._expr_above_range,
      ),

    _expr_above_range: ($) =>
      choice(
        alias($._bitwise_or_higher_binary, $.binary_expression),
        $._cast_operand,
      ),

    _cast_operand: ($) =>
      choice(
        $.cast_expression,
        $._prefix_operand,
      ),

    _prefix_operand: ($) =>
      choice(
        $.prefix_expression,
        $._postfix_operand,
      ),

    _postfix_operand: ($) =>
      choice(
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

    binary_expression: ($) =>
      choice(
        prec.left(
          PREC.OR,
          seq(
            field("left", $.expression),
            field("operator", "||"),
            field("right", $.expression),
          ),
        ),
        prec.left(
          PREC.AND,
          seq(
            field("left", $.expression),
            field("operator", "&&"),
            field("right", $.expression),
          ),
        ),
      ),

    _equality_binary: ($) =>
      prec(
        PREC.EQUALITY,
        seq(
          field("left", alias($._expr_above_equality, $.expression)),
          field("operator", choice("==", "!=")),
          field("right", alias($._expr_above_equality, $.expression)),
        ),
      ),

    _comparison_binary: ($) =>
      prec(
        PREC.COMPARISON,
        seq(
          field("left", alias($._expr_above_comparison, $.expression)),
          field("operator", choice("<", ">", "<=", ">=")),
          field("right", alias($._expr_above_comparison, $.expression)),
        ),
      ),

    _membership_binary: ($) =>
      choice(
        prec(
          PREC.MEMBERSHIP,
          seq(
            field("left", alias($._expr_above_membership, $.expression)),
            field("operator", choice(alias($._in, "in"), alias($._not_in, "!in"))),
            field("right", alias($._expr_above_membership, $.expression)),
          ),
        ),
        prec(
          PREC.MEMBERSHIP,
          seq(
            field("left", alias($._expr_above_membership, $.expression)),
            field("operator", choice(alias($._is, "is"), alias($._not_is, "!is"))),
            field("right", $._type),
          ),
        ),
      ),

    _elvis_binary: ($) =>
      prec.right(
        PREC.ELVIS,
        seq(
          field("left", alias($._expr_above_elvis, $.expression)),
          field("operator", "?:"),
          field("right", alias($._expr_above_membership, $.expression)),
        ),
      ),

    _to_binary: ($) =>
      prec.left(
        PREC.TO,
        seq(
          field("left", alias($._expr_above_elvis, $.expression)),
          field("operator", alias($._to, $.to_operator)),
          field("right", alias($._expr_above_to, $.expression)),
        ),
      ),

    _range_binary: ($) =>
      prec(
        PREC.RANGE,
        seq(
          field("left", alias($._expr_above_range, $.expression)),
          field("operator", choice("..", "..<")),
          field("right", alias($._expr_above_range, $.expression)),
        ),
      ),

    _bitwise_or_higher_binary: ($) => {
      const table = [
        [PREC.BITWISE_OR, alias($._bit_or, "or"), true],
        [PREC.BITWISE_XOR, alias($._bit_xor, "xor"), true],
        [PREC.BITWISE_AND, alias($._bit_and, "and"), true],
        [PREC.SHIFT, choice(alias($._shl, "shl"), alias($._shr, "shr"), alias($._ushr, "ushr")), true],
        [PREC.ADDITIVE, choice("+", "-"), false],
        [PREC.MULTIPLICATIVE, choice("*", "/", "%"), false],
      ];
      return choice(
        ...table.map(([precedence, operator, allowLineBreak]) =>
          prec.left(
            precedence,
            allowLineBreak
              ? seq(
                  field("left", alias($._expr_above_range, $.expression)),
                  field("operator", operator),
                  repeat($._newline),
                  field("right", alias($._expr_above_range, $.expression)),
                )
              : seq(
                  field("left", alias($._expr_above_range, $.expression)),
                  field("operator", operator),
                  field("right", alias($._expr_above_range, $.expression)),
                ),
          ),
        ),
      );
    },

    cast_expression: ($) =>
      prec.left(
        PREC.CAST,
        seq(
          field("value", alias($._cast_operand, $.expression)),
          field("operator", choice(alias($._as, "as"), "as?")),
          field("type", $._type),
        ),
      ),

    prefix_expression: ($) =>
      prec.right(
        PREC.PREFIX,
        seq(
          field("operator", choice("!", "+", "-")),
          field("value", alias($._prefix_operand, $.expression)),
        ),
      ),

    call_expression: ($) =>
      choice(
        prec.left(
          PREC.POSTFIX,
          seq(
            field("function", alias($._postfix_operand, $.expression)),
            $.argument_list,
            optional(alias($.trailing_lambda_expression, $.lambda_expression)),
          ),
        ),
        prec.left(
          PREC.POSTFIX,
          seq(
            field("function", alias($._postfix_operand, $.expression)),
            alias($.same_line_trailing_lambda_expression, $.lambda_expression),
          ),
        ),
        prec.left(
          seq(
            field("function", alias($._postfix_operand, $.expression)),
            $.call_type_arguments,
            $.argument_list,
            optional(alias($.trailing_lambda_expression, $.lambda_expression)),
          ),
        ),
        prec.left(
          seq(
            field("function", alias($._postfix_operand, $.expression)),
            $.call_type_arguments,
            alias($.same_line_trailing_lambda_expression, $.lambda_expression),
          ),
        ),
      ),

    call_type_arguments: ($) => prec.dynamic(1, seq("<", commaSep1($._type), ">")),

    argument_list: ($) => seq("(", optional(commaSep1($.call_argument)), ")"),

    call_argument: ($) =>
      seq(
        optional($.named_argument_prefix),
        optional($.argument_mode),
        field("value", $.expression),
      ),

    // An ungrouped name = prefix commits to a named argument (Guide 07).
    named_argument_prefix: ($) => prec(1, seq(field("name", $.identifier), "=")),

    argument_mode: () => "&",

    index_expression: ($) =>
      prec.left(
        PREC.POSTFIX,
        seq(
          field("value", alias($._postfix_operand, $.expression)),
          "[",
          field("index", $.expression),
          "]",
        ),
      ),

    member_expression: ($) =>
      prec.left(
        PREC.POSTFIX,
        seq(
          field("value", alias($._postfix_operand, $.expression)),
          field("operator", choice(".", "?.")),
          field("name", $.identifier),
        ),
      ),

    postfix_expression: ($) =>
      prec.left(
        PREC.POSTFIX,
        seq(
          field("value", alias($._postfix_operand, $.expression)),
          field("operator", choice("!!", "?")),
        ),
      ),

    callable_reference: ($) =>
      prec.left(
        PREC.POSTFIX,
        choice(
          seq("::", field("name", $.identifier)),
          seq(
            field("value", alias($._postfix_operand, $.expression)),
            "::",
            field("name", $.identifier),
          ),
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
      choice(
        seq(
          alias($._move_lambda, "move"),
          repeat($._newline),
          "{",
          optional($.lambda_header),
          repeat(choice($._newline, ";", $._lambda_element)),
          "}",
        ),
        seq(
          alias($._lambda_lbrace, "{"),
          optional($.lambda_header),
          repeat(choice($._newline, ";", $._lambda_element)),
          "}",
        ),
      ),

    trailing_lambda_expression: ($) =>
      seq(
        repeat($._crossline_lambda_nl),
        alias($._same_line_trailing_lbrace, "{"),
        optional($.lambda_header),
        repeat(choice($._newline, ";", $._lambda_element)),
        "}",
      ),

    same_line_trailing_lambda_expression: ($) =>
      seq(
        alias($._same_line_trailing_lbrace, "{"),
        optional($.lambda_header),
        repeat(choice($._newline, ";", $._lambda_element)),
        "}",
      ),

    lambda_header: ($) => seq(optional(commaSep1($.identifier)), "->"),

    _lambda_element: ($) =>
      choice(
        $.local_variable_statement,
        $.local_destructuring_statement,
        $.nested_block_statement,
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
        seq(choice(alias($._is, "is"), alias($._not_is, "!is")), $._type),
        seq(choice(alias($._in, "in"), alias($._not_in, "!in")), $.expression),
      ),

    control_body: ($) => choice($.expression, $.control_block),

    control_block: ($) =>
      prec(2, seq(alias($._control_lbrace, "{"), repeat(choice($._newline, ";", $._statement)), "}")),

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
        optional(seq(alias($._move_type, "move"), repeat($._newline))),
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
    block_comment: () => token(seq("/*", /[^*\n]*\*+([^/*\n][^*\n]*\*+)*/, "/")),
    multiline_block_comment: () =>
      token(seq("/*", /([^*\n]|\*+[^/*\n])*\**\n([^*]|\*+[^/*])*\*+/, "/")),

    _newline: ($) =>
      choice(/(?:\r?\n)+/, alias($.multiline_block_comment, $.block_comment)),
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
