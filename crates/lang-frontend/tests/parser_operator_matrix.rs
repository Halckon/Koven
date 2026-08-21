//! SPEC-0072 / SPEC-0104 的 Pratt 运算符优先级、结合性与不结合组矩阵契约。

use lang_frontend::{
    ast::ExpressionId,
    parser::{
        AssignmentOperator, BinaryOperator, CastOperator, Expression, ParsedExpression,
        PrefixOperator, parse_expression,
    },
};

#[path = "support/frontend_output_assertions.rs"]
mod frontend_output_assertions;
#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;

use frontend_output_assertions::{validate_ast, validate_diagnostics, validate_lexed};
use lexer_matrix_assertions::lex_source_twice;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperatorKind {
    Member,
    Prefix(PrefixOperator),
    Binary(BinaryOperator),
    Assignment(AssignmentOperator),
    Cast(CastOperator),
    TypeTest(bool),
}

#[derive(Clone, Copy)]
struct OperatorCase {
    spelling: &'static str,
    kind: OperatorKind,
}

fn parse_case(text: &str, expect_clean: bool) -> ParsedExpression {
    let (sources, source_id, lexed) =
        lex_source_twice("operator-matrix.ko", text, text, validate_lexed);
    assert!(
        lexed.diagnostics().is_empty(),
        "{text:?}: {:?}",
        lexed.diagnostics()
    );
    let first = parse_expression(&sources, &lexed)
        .unwrap_or_else(|error| panic!("first parse failed for {text:?}: {error}"));
    let repeated = parse_expression(&sources, &lexed)
        .unwrap_or_else(|error| panic!("repeated parse failed for {text:?}: {error}"));
    for parsed in [&first, &repeated] {
        assert_eq!(parsed.source_id(), source_id);
        validate_ast(source_id, text.len(), parsed.ast());
        validate_diagnostics(source_id, text.len(), parsed.diagnostics());
        parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .unwrap_or_else(|error| panic!("invalid expression root for {text:?}: {error}"));
        if expect_clean {
            assert!(
                parsed.diagnostics().is_empty(),
                "{text:?}: {:?}",
                parsed.diagnostics()
            );
        }
    }
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic parse for {text:?}"
    );
    first
}

fn parse(text: &str) -> ParsedExpression {
    parse_case(text, true)
}

fn expression(parsed: &ParsedExpression, id: ExpressionId) -> &Expression {
    parsed
        .ast()
        .expressions()
        .get(id)
        .expect("matrix expression ID must resolve")
        .payload()
}

fn assert_kind(parsed: &ParsedExpression, id: ExpressionId, expected: OperatorKind, text: &str) {
    let actual = expression(parsed, id);
    let matches = match (expected, actual) {
        (OperatorKind::Member, Expression::Member { .. }) => true,
        (
            OperatorKind::Prefix(expected),
            Expression::Prefix {
                operator: actual, ..
            },
        ) => expected == *actual,
        (
            OperatorKind::Binary(expected),
            Expression::Binary {
                operator: actual, ..
            },
        ) => expected == *actual,
        (
            OperatorKind::Assignment(expected),
            Expression::Assignment {
                operator: actual, ..
            },
        ) => expected == *actual,
        (
            OperatorKind::Cast(expected),
            Expression::Cast {
                operator: actual, ..
            },
        ) => expected == *actual,
        (
            OperatorKind::TypeTest(expected),
            Expression::TypeTest {
                negated: actual, ..
            },
        ) => expected == *actual,
        _ => false,
    };
    assert!(matches, "{text:?}: expected {expected:?}, got {actual:?}");
}

fn expression_infix_levels() -> [OperatorCase; 11] {
    [
        OperatorCase {
            spelling: "*",
            kind: OperatorKind::Binary(BinaryOperator::Multiply),
        },
        OperatorCase {
            spelling: "+",
            kind: OperatorKind::Binary(BinaryOperator::Add),
        },
        OperatorCase {
            spelling: "..",
            kind: OperatorKind::Binary(BinaryOperator::InclusiveRange),
        },
        OperatorCase {
            spelling: "to",
            kind: OperatorKind::Binary(BinaryOperator::To),
        },
        OperatorCase {
            spelling: "?:",
            kind: OperatorKind::Binary(BinaryOperator::Elvis),
        },
        OperatorCase {
            spelling: "in",
            kind: OperatorKind::Binary(BinaryOperator::In),
        },
        OperatorCase {
            spelling: "<",
            kind: OperatorKind::Binary(BinaryOperator::Less),
        },
        OperatorCase {
            spelling: "==",
            kind: OperatorKind::Binary(BinaryOperator::Equal),
        },
        OperatorCase {
            spelling: "&&",
            kind: OperatorKind::Binary(BinaryOperator::LogicalAnd),
        },
        OperatorCase {
            spelling: "||",
            kind: OperatorKind::Binary(BinaryOperator::LogicalOr),
        },
        OperatorCase {
            spelling: "=",
            kind: OperatorKind::Assignment(AssignmentOperator::Assign),
        },
    ]
}

fn expression_children(
    parsed: &ParsedExpression,
    id: ExpressionId,
) -> (ExpressionId, ExpressionId) {
    match expression(parsed, id) {
        Expression::Binary { left, right, .. } => (*left, *right),
        Expression::Assignment { target, value, .. } => (*target, *value),
        actual => panic!("expected expression-RHS infix node, got {actual:?}"),
    }
}

#[test]
fn every_expression_infix_precedence_pair_groups_in_both_source_orders() {
    // 顺序来自现行 guide（高到低）；不复制实现中的 binding-power 数值。
    let levels = expression_infix_levels();

    let mut executed = 0;
    for (higher_index, higher) in levels.iter().enumerate() {
        for lower in &levels[higher_index + 1..] {
            let higher_first = format!("a {} b {} c", higher.spelling, lower.spelling);
            let parsed = parse(&higher_first);
            assert_kind(&parsed, parsed.root(), lower.kind, &higher_first);
            let (left, _) = expression_children(&parsed, parsed.root());
            assert_kind(&parsed, left, higher.kind, &higher_first);
            executed += 1;

            let lower_first = format!("a {} b {} c", lower.spelling, higher.spelling);
            let parsed = parse(&lower_first);
            assert_kind(&parsed, parsed.root(), lower.kind, &lower_first);
            let (_, right) = expression_children(&parsed, parsed.root());
            assert_kind(&parsed, right, higher.kind, &lower_first);
            executed += 1;
        }
    }

    assert_eq!(executed, 11 * 10);
}

#[test]
fn postfix_prefix_and_cast_bind_before_every_lower_level() {
    let levels = expression_infix_levels();
    let high_layers = [
        ("a.member", OperatorKind::Member),
        ("-a", OperatorKind::Prefix(PrefixOperator::Minus)),
        ("a as T", OperatorKind::Cast(CastOperator::As)),
    ];

    let mut executed = 0;
    for (source, expected) in high_layers {
        for original_lower in &levels {
            // `T < ...` 在 type_ref 内按现行 EBNF 开始泛型实参；用同层的 `>` 消除该歧义。
            let lower =
                if matches!(expected, OperatorKind::Cast(_)) && original_lower.spelling == "<" {
                    OperatorCase {
                        spelling: ">",
                        kind: OperatorKind::Binary(BinaryOperator::Greater),
                    }
                } else {
                    *original_lower
                };
            let text = format!("{source} {} c", lower.spelling);
            let parsed = parse(&text);
            assert_kind(&parsed, parsed.root(), lower.kind, &text);
            let (left, _) = expression_children(&parsed, parsed.root());
            assert_kind(&parsed, left, expected, &text);
            executed += 1;
        }
    }

    for (text, outer, inner) in [
        (
            "a.member as T",
            OperatorKind::Cast(CastOperator::As),
            OperatorKind::Member,
        ),
        (
            "-a as T",
            OperatorKind::Cast(CastOperator::As),
            OperatorKind::Prefix(PrefixOperator::Minus),
        ),
    ] {
        let parsed = parse(text);
        assert_kind(&parsed, parsed.root(), outer, text);
        let Expression::Cast {
            expression: operand,
            ..
        } = expression(&parsed, parsed.root())
        else {
            panic!("{text:?}: expected outer cast")
        };
        assert_kind(&parsed, *operand, inner, text);
        executed += 1;
    }

    let text = "-a.member";
    let parsed = parse(text);
    assert_kind(
        &parsed,
        parsed.root(),
        OperatorKind::Prefix(PrefixOperator::Minus),
        text,
    );
    let Expression::Prefix { operand, .. } = expression(&parsed, parsed.root()) else {
        panic!("{text:?}: expected outer prefix")
    };
    assert_kind(&parsed, *operand, OperatorKind::Member, text);
    executed += 1;

    assert_eq!(executed, 36);
}

#[test]
fn every_left_and_right_associative_variant_uses_the_expected_child_direction() {
    let left_groups: &[&[OperatorCase]] = &[
        &[
            OperatorCase {
                spelling: "*",
                kind: OperatorKind::Binary(BinaryOperator::Multiply),
            },
            OperatorCase {
                spelling: "/",
                kind: OperatorKind::Binary(BinaryOperator::Divide),
            },
            OperatorCase {
                spelling: "%",
                kind: OperatorKind::Binary(BinaryOperator::Remainder),
            },
        ],
        &[
            OperatorCase {
                spelling: "+",
                kind: OperatorKind::Binary(BinaryOperator::Add),
            },
            OperatorCase {
                spelling: "-",
                kind: OperatorKind::Binary(BinaryOperator::Subtract),
            },
        ],
        &[OperatorCase {
            spelling: "to",
            kind: OperatorKind::Binary(BinaryOperator::To),
        }],
        &[OperatorCase {
            spelling: "&&",
            kind: OperatorKind::Binary(BinaryOperator::LogicalAnd),
        }],
        &[OperatorCase {
            spelling: "||",
            kind: OperatorKind::Binary(BinaryOperator::LogicalOr),
        }],
    ];
    let assignments = [
        OperatorCase {
            spelling: "=",
            kind: OperatorKind::Assignment(AssignmentOperator::Assign),
        },
        OperatorCase {
            spelling: "+=",
            kind: OperatorKind::Assignment(AssignmentOperator::AddAssign),
        },
        OperatorCase {
            spelling: "-=",
            kind: OperatorKind::Assignment(AssignmentOperator::SubtractAssign),
        },
        OperatorCase {
            spelling: "*=",
            kind: OperatorKind::Assignment(AssignmentOperator::MultiplyAssign),
        },
        OperatorCase {
            spelling: "/=",
            kind: OperatorKind::Assignment(AssignmentOperator::DivideAssign),
        },
        OperatorCase {
            spelling: "%=",
            kind: OperatorKind::Assignment(AssignmentOperator::RemainderAssign),
        },
    ];

    let mut left_executed = 0;
    for group in left_groups {
        for first in *group {
            for second in *group {
                let text = format!("a {} b {} c", first.spelling, second.spelling);
                let parsed = parse(&text);
                assert_kind(&parsed, parsed.root(), second.kind, &text);
                let (left, _) = expression_children(&parsed, parsed.root());
                assert_kind(&parsed, left, first.kind, &text);
                left_executed += 1;
            }
        }
    }
    assert_eq!(left_executed, 9 + 4 + 1 + 1 + 1);

    let mut right_executed = 0;
    for first in assignments {
        for second in assignments {
            let text = format!("a {} b {} c", first.spelling, second.spelling);
            let parsed = parse(&text);
            assert_kind(&parsed, parsed.root(), first.kind, &text);
            let (_, right) = expression_children(&parsed, parsed.root());
            assert_kind(&parsed, right, second.kind, &text);
            right_executed += 1;
        }
    }
    assert_eq!(right_executed, 36);

    let text = "a ?: b ?: c";
    let parsed = parse(text);
    assert_kind(
        &parsed,
        parsed.root(),
        OperatorKind::Binary(BinaryOperator::Elvis),
        text,
    );
    let (_, right) = expression_children(&parsed, parsed.root());
    assert_kind(
        &parsed,
        right,
        OperatorKind::Binary(BinaryOperator::Elvis),
        text,
    );

    let text = "a as T as? U";
    let parsed = parse(text);
    assert_kind(
        &parsed,
        parsed.root(),
        OperatorKind::Cast(CastOperator::SafeAs),
        text,
    );
    let Expression::Cast {
        expression: left, ..
    } = expression(&parsed, parsed.root())
    else {
        panic!("{text:?}: expected outer cast")
    };
    assert_kind(&parsed, *left, OperatorKind::Cast(CastOperator::As), text);
}

#[test]
fn every_non_associative_group_pair_reports_the_second_operator_span() {
    let range = [
        OperatorCase {
            spelling: "..",
            kind: OperatorKind::Binary(BinaryOperator::InclusiveRange),
        },
        OperatorCase {
            spelling: "..<",
            kind: OperatorKind::Binary(BinaryOperator::ExclusiveRange),
        },
    ];
    let membership = [
        OperatorCase {
            spelling: "in",
            kind: OperatorKind::Binary(BinaryOperator::In),
        },
        OperatorCase {
            spelling: "!in",
            kind: OperatorKind::Binary(BinaryOperator::NotIn),
        },
        OperatorCase {
            spelling: "is",
            kind: OperatorKind::TypeTest(false),
        },
        OperatorCase {
            spelling: "!is",
            kind: OperatorKind::TypeTest(true),
        },
    ];
    let comparison = [
        OperatorCase {
            spelling: "<",
            kind: OperatorKind::Binary(BinaryOperator::Less),
        },
        OperatorCase {
            spelling: ">",
            kind: OperatorKind::Binary(BinaryOperator::Greater),
        },
        OperatorCase {
            spelling: "<=",
            kind: OperatorKind::Binary(BinaryOperator::LessEqual),
        },
        OperatorCase {
            spelling: ">=",
            kind: OperatorKind::Binary(BinaryOperator::GreaterEqual),
        },
    ];
    let equality = [
        OperatorCase {
            spelling: "==",
            kind: OperatorKind::Binary(BinaryOperator::Equal),
        },
        OperatorCase {
            spelling: "!=",
            kind: OperatorKind::Binary(BinaryOperator::NotEqual),
        },
    ];

    let mut executed = 0;
    for group in [&range[..], &membership[..], &comparison[..], &equality[..]] {
        for first in group {
            for second in group {
                let first_rhs = if matches!(first.kind, OperatorKind::TypeTest(_)) {
                    "T"
                } else {
                    "b"
                };
                let second_rhs = if matches!(second.kind, OperatorKind::TypeTest(_)) {
                    "U"
                } else {
                    "c"
                };
                let prefix = format!("a {} {first_rhs} ", first.spelling);
                let second_start = prefix.len();
                let text = format!("{prefix}{} {second_rhs}", second.spelling);

                let parsed = parse_case(&text, false);
                let diagnostics = parsed.diagnostics();
                assert_eq!(diagnostics.len(), 1, "{text:?}: {diagnostics:?}");
                assert_eq!(diagnostics[0].code().to_string(), "L0012", "{text:?}");
                assert_eq!(
                    (
                        diagnostics[0].primary_span().start(),
                        diagnostics[0].primary_span().end(),
                    ),
                    (second_start, second_start + second.spelling.len()),
                    "{text:?}",
                );
                executed += 1;
            }
        }
    }

    assert_eq!(executed, 4 + 16 + 16 + 4);
}
