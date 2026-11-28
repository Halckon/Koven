//! SPEC-0485 (F3c -> F4a): Independent 18-level expression precedence, associativity,
//! non-associative group, and adjacency-forbidden-operator differential tests against
//! `lang_frontend::parser::parse_expression`.

#[path = "f3c_expr_diff/reference_parser.rs"]
mod reference_parser;

use lang_frontend::{
    ast::{ExpressionId, TypeRefId},
    lexer::lex,
    parser::{
        AssignmentOperator, BinaryOperator, CastOperator, Expression, LiteralKind,
        ParameterModeMarker, PrefixOperator, SyntaxAst, TypeRef, parse_expression,
    },
    source::{SourceMap, Span},
};
use reference_parser::{Outcome, run_expression_reference};

fn within(parent: Span, child: Span) {
    assert_eq!(parent.source_id(), child.source_id());
    assert!(
        parent.start() <= child.start()
            && child.start() <= child.end()
            && child.end() <= parent.end(),
        "child span {child:?} escapes parent {parent:?}"
    );
}

fn prod_type(ast: &SyntaxAst, sources: &SourceMap, id: TypeRefId, parent: Span) -> String {
    let node = ast.type_refs().get(id).unwrap();
    let span = node.span();
    within(parent, span);
    match node.payload() {
        TypeRef::Qualified {
            segments,
            nullable_span,
        } => {
            assert_eq!(segments.len(), 1);
            let seg = &segments[0];
            within(span, seg.name_span);
            assert!(seg.arguments.is_empty());
            let mut out = sources.slice(seg.name_span).unwrap().to_owned();
            if let Some(q) = nullable_span {
                within(span, *q);
                out.push('?');
            }
            out
        }
        other => panic!("unexpected TypeRef in F3c slice: {other:?}"),
    }
}

fn prod_expr(ast: &SyntaxAst, sources: &SourceMap, id: ExpressionId, parent: Span) -> String {
    let node = ast.expressions().get(id).unwrap();
    let span = node.span();
    within(parent, span);
    match node.payload() {
        Expression::Name => sources.slice(span).unwrap().to_owned(),
        Expression::This => "this".to_owned(),
        Expression::Literal(LiteralKind::Integer(_)) => sources.slice(span).unwrap().to_owned(),
        Expression::Literal(LiteralKind::Boolean(b)) => b.to_string(),
        Expression::Literal(LiteralKind::Null) => "null".to_owned(),
        Expression::Group { expression } => {
            format!("(group {})", prod_expr(ast, sources, *expression, span))
        }
        Expression::Prefix {
            operator,
            operator_span,
            operand,
        } => {
            within(span, *operator_span);
            let operand_span = ast.expressions().get(*operand).unwrap().span();
            assert!(operator_span.end() <= operand_span.start());
            let op = match operator {
                PrefixOperator::Not => "pre!",
                PrefixOperator::Plus => "pre+",
                PrefixOperator::Minus => "pre-",
            };
            format!("({op} {})", prod_expr(ast, sources, *operand, span))
        }
        Expression::Cast {
            expression,
            operator,
            operator_span,
            type_ref,
        } => {
            within(span, *operator_span);
            let lhs_span = ast.expressions().get(*expression).unwrap().span();
            let rhs_span = ast.type_refs().get(*type_ref).unwrap().span();
            assert!(lhs_span.end() <= operator_span.start());
            assert!(operator_span.end() <= rhs_span.start());
            let op = match operator {
                CastOperator::As => "as",
                CastOperator::SafeAs => "as?",
            };
            format!(
                "({op} {} {})",
                prod_expr(ast, sources, *expression, span),
                prod_type(ast, sources, *type_ref, span)
            )
        }
        Expression::TypeTest {
            expression,
            negated,
            operator_span,
            type_ref,
        } => {
            within(span, *operator_span);
            let lhs_span = ast.expressions().get(*expression).unwrap().span();
            let rhs_span = ast.type_refs().get(*type_ref).unwrap().span();
            assert!(lhs_span.end() <= operator_span.start());
            assert!(operator_span.end() <= rhs_span.start());
            let op = if *negated { "!is" } else { "is" };
            format!(
                "({op} {} {})",
                prod_expr(ast, sources, *expression, span),
                prod_type(ast, sources, *type_ref, span)
            )
        }
        Expression::Binary {
            left,
            operator,
            operator_span,
            right,
        } => {
            within(span, *operator_span);
            let lhs_span = ast.expressions().get(*left).unwrap().span();
            let rhs_span = ast.expressions().get(*right).unwrap().span();
            assert!(lhs_span.end() <= operator_span.start());
            assert!(operator_span.end() <= rhs_span.start());
            let op = match operator {
                BinaryOperator::Multiply => "*",
                BinaryOperator::Divide => "/",
                BinaryOperator::Remainder => "%",
                BinaryOperator::Add => "+",
                BinaryOperator::Subtract => "-",
                BinaryOperator::Shl => "shl",
                BinaryOperator::Shr => "shr",
                BinaryOperator::Ushr => "ushr",
                BinaryOperator::BitwiseAnd => "and",
                BinaryOperator::BitwiseXor => "xor",
                BinaryOperator::BitwiseOr => "or",
                BinaryOperator::InclusiveRange => "..",
                BinaryOperator::ExclusiveRange => "..<",
                BinaryOperator::To => "to",
                BinaryOperator::Elvis => "?:",
                BinaryOperator::In => "in",
                BinaryOperator::NotIn => "!in",
                BinaryOperator::Less => "<",
                BinaryOperator::Greater => ">",
                BinaryOperator::LessEqual => "<=",
                BinaryOperator::GreaterEqual => ">=",
                BinaryOperator::Equal => "==",
                BinaryOperator::NotEqual => "!=",
                BinaryOperator::LogicalAnd => "&&",
                BinaryOperator::LogicalOr => "||",
            };
            format!(
                "({op} {} {})",
                prod_expr(ast, sources, *left, span),
                prod_expr(ast, sources, *right, span)
            )
        }
        Expression::Assignment {
            target,
            operator,
            operator_span,
            value,
        } => {
            within(span, *operator_span);
            let lhs_span = ast.expressions().get(*target).unwrap().span();
            let rhs_span = ast.expressions().get(*value).unwrap().span();
            assert!(lhs_span.end() <= operator_span.start());
            assert!(operator_span.end() <= rhs_span.start());
            let op = match operator {
                AssignmentOperator::Assign => "=",
                AssignmentOperator::AddAssign => "+=",
                AssignmentOperator::SubtractAssign => "-=",
                AssignmentOperator::MultiplyAssign => "*=",
                AssignmentOperator::DivideAssign => "/=",
                AssignmentOperator::RemainderAssign => "%=",
            };
            format!(
                "({op} {} {})",
                prod_expr(ast, sources, *target, span),
                prod_expr(ast, sources, *value, span)
            )
        }
        Expression::Member {
            receiver,
            operator_span,
            name_span,
            safe,
        } => {
            within(span, *operator_span);
            within(span, *name_span);
            let recv_span = ast.expressions().get(*receiver).unwrap().span();
            assert!(recv_span.end() <= operator_span.start());
            assert!(operator_span.end() <= name_span.start());
            let op = if *safe { "?." } else { "." };
            format!(
                "({op} {} {})",
                prod_expr(ast, sources, *receiver, span),
                sources.slice(*name_span).unwrap()
            )
        }
        Expression::Call {
            callee,
            type_arguments,
            arguments,
            ..
        } => {
            assert!(type_arguments.is_empty());
            let c = prod_expr(ast, sources, *callee, span);
            if arguments.is_empty() {
                format!("(call {c})")
            } else {
                let args: Vec<_> = arguments
                    .iter()
                    .map(|a| {
                        within(span, a.span);
                        let val = prod_expr(ast, sources, a.value, a.span);
                        let rendered = match a.mode_marker {
                            Some(ParameterModeMarker::Inout(ms)) => {
                                within(a.span, ms);
                                format!("&{val}")
                            }
                            None => val,
                            other => panic!("unexpected call mode: {other:?}"),
                        };
                        if let Some(np) = a.named_prefix {
                            within(a.span, np.name_span);
                            within(a.span, np.equals_span);
                            format!(
                                "(named {} {rendered})",
                                sources.slice(np.name_span).unwrap()
                            )
                        } else {
                            rendered
                        }
                    })
                    .collect();
                format!("(call {c} {})", args.join(" "))
            }
        }
        Expression::Index { receiver, index } => format!(
            "(idx {} {})",
            prod_expr(ast, sources, *receiver, span),
            prod_expr(ast, sources, *index, span)
        ),
        Expression::NonNullAssert {
            operand,
            operator_span,
        } => {
            within(span, *operator_span);
            let op_span = ast.expressions().get(*operand).unwrap().span();
            assert!(op_span.end() <= operator_span.start());
            format!("(!! {})", prod_expr(ast, sources, *operand, span))
        }
        Expression::Propagate {
            value,
            question_span,
        } => {
            within(span, *question_span);
            let op_span = ast.expressions().get(*value).unwrap().span();
            assert!(op_span.end() <= question_span.start());
            format!("(? {})", prod_expr(ast, sources, *value, span))
        }
        Expression::CallableReference {
            receiver,
            operator_span,
            name_span,
        } => {
            within(span, *operator_span);
            within(span, *name_span);
            assert!(operator_span.end() <= name_span.start());
            match receiver {
                Some(r) => {
                    let recv_span = ast.expressions().get(*r).unwrap().span();
                    assert!(recv_span.end() <= operator_span.start());
                    format!(
                        "(:: {} {})",
                        prod_expr(ast, sources, *r, span),
                        sources.slice(*name_span).unwrap()
                    )
                }
                None => format!("(:: {})", sources.slice(*name_span).unwrap()),
            }
        }
        other => panic!("unexpected Expression in F3c slice: {other:?}"),
    }
}

fn run_expression_production(source: &str) -> (Outcome, Vec<String>) {
    let mut sources = SourceMap::new();
    let id = sources.add_source("prod-expr.ko", source).unwrap();
    let lexed = lex(&sources, id).expect("lex must not fail internally");
    if !lexed.diagnostics().is_empty() {
        return (
            Outcome::Rejected,
            lexed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect(),
        );
    }
    let parsed =
        parse_expression(&sources, &lexed).expect("parse_expression must not fail internally");
    if !parsed.diagnostics().is_empty() {
        return (
            Outcome::Rejected,
            parsed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect(),
        );
    }
    let root_span = parsed
        .ast()
        .expressions()
        .get(parsed.root())
        .unwrap()
        .span();
    assert_eq!(root_span.source_id(), id);
    assert!(root_span.start() <= root_span.end() && root_span.end() <= source.len());
    let sexpr = prod_expr(parsed.ast(), &sources, parsed.root(), root_span);
    (Outcome::Accepted(sexpr), Vec::new())
}

fn compare(prod: &Outcome, reference: &Outcome) -> Result<(), String> {
    if prod == reference {
        Ok(())
    } else {
        Err(format!(
            "F3c differential mismatch:\n  production: {prod:?}\n  reference:  {reference:?}"
        ))
    }
}

fn assert_expr_accepted(label: &str, source: &str) {
    let (prod, codes) = run_expression_production(source);
    assert!(
        matches!(prod, Outcome::Accepted(_)),
        "{label} ({source:?}): production rejected with {codes:?}"
    );
    let reference = run_expression_reference(source);
    compare(&prod, &reference).unwrap_or_else(|err| panic!("{label} ({source:?}):\n{err}"));
}

fn assert_expr_rejected(label: &str, source: &str) {
    let (prod, _) = run_expression_production(source);
    assert_eq!(
        prod,
        Outcome::Rejected,
        "{label} ({source:?}): production must reject"
    );
    let reference = run_expression_reference(source);
    assert_eq!(
        reference,
        Outcome::Rejected,
        "{label} ({source:?}): reference must reject"
    );
}

#[test]
fn fault_controls_detect_precedence_associativity_and_acceptance_drift() {
    let sample = "a..b to c ?: d ?: e";
    let (prod, _) = run_expression_production(sample);
    let reference = run_expression_reference(sample);
    assert_eq!(compare(&prod, &reference), Ok(()));
    assert_eq!(
        reference,
        Outcome::Accepted("(?: (to (.. a b) c) (?: d e))".to_owned())
    );

    // 1. Acceptance vs rejection drift.
    assert!(compare(&prod, &Outcome::Rejected).is_err());
    // 2. Precedence swap (`..` vs `to`).
    assert!(
        compare(
            &prod,
            &Outcome::Accepted("(?: (.. a (to b c)) (?: d e))".to_owned())
        )
        .is_err()
    );
    // 3. Right-associativity drift on `?:`.
    assert!(
        compare(
            &prod,
            &Outcome::Accepted("(?: (?: (to (.. a b) c) d) e)".to_owned())
        )
        .is_err()
    );
    // 4. Right-associativity drift on `=` / `+=`.
    let (assign_prod, _) = run_expression_production("a = b += c");
    assert!(
        compare(
            &assign_prod,
            &Outcome::Accepted("(+= (= a b) c)".to_owned())
        )
        .is_err()
    );
}

#[test]
fn f3c_sixteen_positive_and_ten_negative_witnesses_match_reference() {
    let positive = [
        ("postfix-chain", "a!!.b(c)[i]::ref!!"),
        ("safe-member-propagate", "a?.b?.c?"),
        ("prefix-cast-mul", "-a as Int * b"),
        ("double-prefix", "!-a"),
        ("cast-nullable-chain", "a as? Int? as Long"),
        ("bitwise-ladder", "a + b shl c and d xor e or f"),
        ("shift-and-mul", "a * b + c shr d ushr e"),
        ("range-to-elvis", "a or b..c to d ?: e ?: f"),
        ("full-logic-ladder", "a ?: b in c < d == e && f || g"),
        ("is-and-comparison", "a !is Int? == false"),
        ("assignment-right-assoc", "a = b += c -= d *= e /= f %= g"),
        ("grouped-comparison-chain", "(a < b) < c"),
        ("grouped-range-chain", "(a..b)..<c"),
        ("unbound-callable-ref-call", "::top()"),
        ("named-and-inout-call-args", "f(name = a, &b, other = &c)"),
        ("inout-grouped-assign", "f(&(x = y))"),
        ("atoms", "this.x ?: null ?: true"),
    ];
    for (label, src) in positive {
        assert_expr_accepted(label, src);
    }

    let negative = [
        ("range-chain-mixed", "a..b..<c"),
        ("range-chain-same", "a..b..c"),
        ("in-then-is", "a in b is Int"),
        ("is-then-not-in", "a is Int !in b"),
        ("lt-then-le", "a < b <= c"),
        ("gt-then-ge", "a > b >= c"),
        ("eq-then-ne", "a == b != c"),
        ("not-is-then-is", "a !is Int is Long"),
        ("dangling-plus", "a +"),
        ("missing-operator", "a b"),
        ("trailing-comma-in-call", "f(a,)"),
        ("inout-ungrouped-assign", "f(&x = y)"),
    ];
    for (label, src) in negative {
        assert_expr_rejected(label, src);
    }
}

#[test]
fn adjacency_forbidden_operators_vs_spaced_prefix_match_reference() {
    for (label, src) in [
        ("adj-prefix-inc", "++a"),
        ("adj-prefix-dec", "--a"),
        ("adj-postfix-inc", "a++"),
        ("adj-postfix-dec", "a--"),
        ("adj-shl", "a << b"),
        ("adj-shr", "a >> b"),
        ("adj-ellipsis", "a...b"),
    ] {
        assert_expr_rejected(label, src);
    }

    for (label, src) in [
        ("spaced-plus-plus", "+ +a"),
        ("spaced-minus-minus", "- -a"),
        ("spaced-add-pos", "a + +b"),
        ("spaced-sub-neg", "a - -b"),
    ] {
        assert_expr_accepted(label, src);
    }
}

#[test]
fn exhaustive_adjacent_precedence_pairs_and_non_assoc_groups_match_reference() {
    // Representative operator for each binary level from P04 (highest) down to P18 (lowest).
    let levels = [
        ("p04", "*"),
        ("p05", "+"),
        ("p06", "shl"),
        ("p07", "and"),
        ("p08", "xor"),
        ("p09", "or"),
        ("p10", ".."),
        ("p11", "to"),
        ("p12", "?:"),
        ("p13", "in"),
        ("p14", "<"),
        ("p15", "=="),
        ("p16", "&&"),
        ("p17", "||"),
        ("p18", "="),
    ];

    let mut total = 0usize;
    for w in levels.windows(2) {
        let (hi_name, hi_op) = w[0];
        let (lo_name, lo_op) = w[1];
        // Both orders: `a hi b lo c` and `a lo b hi c`
        let s1 = format!("a {hi_op} b {lo_op} c");
        let s2 = format!("a {lo_op} b {hi_op} c");
        assert_expr_accepted(&format!("prec-{hi_name}-{lo_name}"), &s1);
        assert_expr_accepted(&format!("prec-{lo_name}-{hi_name}"), &s2);
        total += 2;
    }
    assert_eq!(total, 28);

    // Every pair within each of the 4 non-associative groups must be rejected by both sides.
    let non_assoc_groups: &[(&str, &[(&str, bool)])] = &[
        ("p10", &[("..", false), ("..<", false)]),
        (
            "p13",
            &[("in", false), ("!in", false), ("is", true), ("!is", true)],
        ),
        (
            "p14",
            &[("<", false), (">", false), ("<=", false), (">=", false)],
        ),
        ("p15", &[("==", false), ("!=", false)]),
    ];
    let mut non_assoc_total = 0usize;
    for (gname, ops) in non_assoc_groups {
        for &(op1, ty1) in *ops {
            for &(op2, ty2) in *ops {
                let rhs1 = if ty1 { "Int" } else { "b" };
                let rhs2 = if ty2 { "Long" } else { "c" };
                let src = format!("a {op1} {rhs1} {op2} {rhs2}");
                assert_expr_rejected(&format!("nonassoc-{gname}-{op1}-{op2}"), &src);
                non_assoc_total += 1;
            }
        }
    }
    assert_eq!(non_assoc_total, 4 + 16 + 16 + 4);
}
