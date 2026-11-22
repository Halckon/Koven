//! SPEC-0483 (F3d -> F4a): Independent reference parser differential tests for F3d
//! high-risk constructs and SPEC-0479 ratified syntax rules.
//!
//! Covers all four F3d external predicates without reusing production parser internals:
//! 1. Zero-side-effect trial: `<...>` typed-call commit/fallback and lambda header trial.
//! 2. Contextual soft keywords: `move` lambda prefix and statement-start `block { ... }`.
//! 3. Newline predicates: `return` operand cutoff, closed-call cross-line trailing lambdas,
//!    comment-separated trailing lambdas, and `when` compound-condition lookahead.
//! 4. Statement vs expression `{ ... }` dispatch: bare `{ ... }` rejection (`L0030`/`L0032`)
//!    vs grouped `({ ... })` and `move { ... }`.

#[path = "f3d_reference_diff/reference_parser.rs"]
mod reference_parser;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    lexer::lex,
    parser::{
        BinaryOperator, Expression, Item, LiteralKind, NameMarker, PrefixOperator, Statement,
        SyntaxAst, TypeRef, VariableKind, WhenCondition, parse_block,
    },
    source::{SourceMap, Span},
};
use reference_parser::{Outcome, format_call, run_reference};

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
            let mut out = sources.slice(seg.name_span).unwrap().to_owned();
            if !seg.arguments.is_empty() {
                let inner: Vec<_> = seg
                    .arguments
                    .iter()
                    .map(|&arg| prod_type(ast, sources, arg, span))
                    .collect();
                out = format!("{out}<{}>", inner.join(","));
            }
            if let Some(q_span) = nullable_span {
                within(span, *q_span);
                out.push('?');
            }
            out
        }
        other => panic!("unexpected TypeRef in F3d slice: {other:?}"),
    }
}

fn prod_item(ast: &SyntaxAst, sources: &SourceMap, id: ItemId, parent: Span) -> String {
    let node = ast.items().get(id).unwrap();
    within(parent, node.span());
    match node.payload() {
        Item::Variable {
            kind: VariableKind::Val,
            name: NameMarker::Present(name_span),
            initializer,
            ..
        } => {
            within(node.span(), *name_span);
            let name = sources.slice(*name_span).unwrap();
            let init = prod_expr(ast, sources, *initializer, node.span());
            format!("(val {name} {init})")
        }
        other => panic!("unexpected item in F3d slice: {other:?}"),
    }
}

fn prod_stmt(
    ast: &SyntaxAst,
    sources: &SourceMap,
    id: StatementId,
    parent: Span,
    is_root_block: bool,
) -> String {
    let node = ast.statements().get(id).unwrap();
    within(parent, node.span());
    match node.payload() {
        Statement::Block { elements } => {
            let tag = if is_root_block {
                "block"
            } else {
                "nested-block"
            };
            if elements.is_empty() {
                format!("({tag})")
            } else {
                let parts: Vec<_> = elements
                    .iter()
                    .map(|&s| prod_stmt(ast, sources, s, node.span(), false))
                    .collect();
                format!("({tag} {})", parts.join(" "))
            }
        }
        Statement::LambdaBody { elements } => {
            if elements.is_empty() {
                "(lbody)".to_owned()
            } else {
                let parts: Vec<_> = elements
                    .iter()
                    .map(|&s| prod_stmt(ast, sources, s, node.span(), false))
                    .collect();
                format!("(lbody {})", parts.join(" "))
            }
        }
        Statement::ControlBody { elements } => {
            if elements.is_empty() {
                "(control)".to_owned()
            } else {
                let parts: Vec<_> = elements
                    .iter()
                    .map(|&s| prod_stmt(ast, sources, s, node.span(), false))
                    .collect();
                format!("(control {})", parts.join(" "))
            }
        }
        Statement::LocalVariable { declaration } => {
            prod_item(ast, sources, *declaration, node.span())
        }
        Statement::Expression { expression } => prod_expr(ast, sources, *expression, node.span()),
        other => panic!("unexpected statement in F3d slice: {other:?}"),
    }
}

fn prod_expr(ast: &SyntaxAst, sources: &SourceMap, id: ExpressionId, parent: Span) -> String {
    let node = ast.expressions().get(id).unwrap();
    let span = node.span();
    within(parent, span);
    match node.payload() {
        Expression::Name => format!("(id {})", sources.slice(span).unwrap()),
        Expression::Literal(LiteralKind::Integer(_)) => {
            format!("(int {})", sources.slice(span).unwrap())
        }
        Expression::Literal(LiteralKind::Boolean(b)) => format!("(bool {b})"),
        Expression::Group { expression } => {
            format!("(group {})", prod_expr(ast, sources, *expression, span))
        }
        Expression::Return { value, .. } => match value {
            None => "(return)".to_owned(),
            Some(v) => format!("(return {})", prod_expr(ast, sources, *v, span)),
        },
        Expression::Prefix {
            operator, operand, ..
        } => {
            let op = match operator {
                PrefixOperator::Minus => "neg",
                PrefixOperator::Not => "not",
                PrefixOperator::Plus => "pos",
            };
            format!("({op} {})", prod_expr(ast, sources, *operand, span))
        }
        Expression::Binary {
            left,
            operator,
            right,
            ..
        } => {
            let op = match operator {
                BinaryOperator::Add => "add",
                BinaryOperator::Subtract => "sub",
                BinaryOperator::Less => "lt",
                BinaryOperator::Greater => "gt",
                BinaryOperator::LogicalAnd => "and",
                BinaryOperator::LogicalOr => "or",
                other => panic!("unexpected binary operator: {other:?}"),
            };
            format!(
                "({op} {} {})",
                prod_expr(ast, sources, *left, span),
                prod_expr(ast, sources, *right, span)
            )
        }
        Expression::Member {
            receiver,
            name_span,
            safe: false,
            ..
        } => {
            within(span, *name_span);
            format!(
                "(member {} (id {}))",
                prod_expr(ast, sources, *receiver, span),
                sources.slice(*name_span).unwrap()
            )
        }
        Expression::Call {
            callee,
            type_arguments,
            type_arguments_span,
            arguments,
            ..
        } => {
            let c = prod_expr(ast, sources, *callee, span);
            let targs_parent = if type_arguments.is_empty() {
                span
            } else {
                let tspan = type_arguments_span.expect("typed call must record <...> span");
                within(span, tspan);
                tspan
            };
            let targs: Vec<_> = type_arguments
                .iter()
                .map(|&t| prod_type(ast, sources, t, targs_parent))
                .collect();
            let args: Vec<_> = arguments
                .iter()
                .map(|a| prod_expr(ast, sources, a.value, span))
                .collect();
            format_call(&c, &targs, &args)
        }
        Expression::Lambda {
            move_span,
            parameters,
            arrow_span,
            body,
            ..
        } => {
            let head = if move_span.is_some() {
                "move-lambda"
            } else {
                "lambda"
            };
            let header_s = if arrow_span.is_none() {
                "no-header".to_owned()
            } else if parameters.is_empty() {
                "params".to_owned()
            } else {
                let ps: Vec<_> = parameters
                    .iter()
                    .map(|&pspan| sources.slice(pspan).unwrap())
                    .collect();
                format!("params {}", ps.join(" "))
            };
            let body_s = prod_stmt(ast, sources, *body, span, false);
            format!("({head} ({header_s}) {body_s})")
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            let c = prod_expr(ast, sources, *condition, span);
            let t = prod_stmt(ast, sources, *then_branch, span, false);
            if let Some(e) = else_branch {
                format!("(if {c} {t} {})", prod_stmt(ast, sources, *e, span, false))
            } else {
                format!("(if {c} {t})")
            }
        }
        Expression::When {
            subject: None,
            entries,
            ..
        } => {
            let rendered: Vec<_> = entries
                .iter()
                .map(|entry| {
                    let conds = if entry.else_span.is_some() {
                        "else".to_owned()
                    } else {
                        let cs: Vec<_> = entry
                            .conditions
                            .iter()
                            .map(|c| match c {
                                WhenCondition::Expression(e) => prod_expr(ast, sources, *e, span),
                                other => panic!("unexpected when condition: {other:?}"),
                            })
                            .collect();
                        format!("conds {}", cs.join(" "))
                    };
                    let b = prod_stmt(ast, sources, entry.body, span, false);
                    format!("(entry ({conds}) {b})")
                })
                .collect();
            format!("(when {})", rendered.join(" "))
        }
        other => panic!("unexpected expression in F3d slice: {other:?}"),
    }
}

fn run_production(source: &str) -> (Outcome, Vec<String>) {
    let mut sources = SourceMap::new();
    let id = sources.add_source("prod.ko", source).unwrap();
    let lexed = lex(&sources, id).expect("lex must not fail internally");
    if !lexed.diagnostics().is_empty() {
        let codes = lexed
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        return (Outcome::Rejected, codes);
    }
    let parsed = parse_block(&sources, &lexed).expect("parse_block must not fail internally");
    if !parsed.diagnostics().is_empty() {
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        return (Outcome::Rejected, codes);
    }
    let ast = parsed.ast();
    let root_span = ast.statements().get(parsed.root()).unwrap().span();
    let sexpr = prod_stmt(ast, &sources, parsed.root(), root_span, true);
    (Outcome::Accepted(sexpr), Vec::new())
}

fn compare(prod: &Outcome, reference: &Outcome) -> Result<(), String> {
    if prod == reference {
        Ok(())
    } else {
        Err(format!(
            "F3d differential mismatch:\n  production: {prod:?}\n  reference:  {reference:?}"
        ))
    }
}

fn assert_accepted_match(label: &str, source: &str) {
    let (prod, codes) = run_production(source);
    assert!(
        matches!(prod, Outcome::Accepted(_)),
        "{label}: production unexpectedly rejected with {codes:?}"
    );
    let reference = run_reference(source);
    compare(&prod, &reference).unwrap_or_else(|err| panic!("{label}:\n{source}\n{err}"));
}

fn assert_rejected_match(label: &str, source: &str, expected_code: &str) {
    let (prod, codes) = run_production(source);
    assert_eq!(prod, Outcome::Rejected, "{label}: production must reject");
    assert!(
        codes.iter().any(|c| c == expected_code),
        "{label}: expected diagnostic {expected_code}, got {codes:?}"
    );
    let reference = run_reference(source);
    assert_eq!(
        reference,
        Outcome::Rejected,
        "{label}: reference must reject"
    );
}

#[test]
fn fault_controls_detect_f3d_structural_and_predicate_drift() {
    let sample = "{\n    block { work() }\n    val b = f()\n    { x() }\n}";
    let (prod, _) = run_production(sample);
    let reference = run_reference(sample);
    assert_eq!(compare(&prod, &reference), Ok(()));

    let Outcome::Accepted(baseline) = &reference else {
        panic!("baseline must accept");
    };
    assert!(compare(&prod, &Outcome::Rejected).is_err());
    assert!(
        compare(
            &prod,
            &Outcome::Accepted(baseline.replace("nested-block", "lambda"))
        )
        .is_err()
    );
    assert!(
        compare(
            &prod,
            &Outcome::Accepted(baseline.replace("(call (id f) ", "(call (id f)) "))
        )
        .is_err()
    );
    let (ret_prod, _) = run_production("{ return\nx }");
    let ret_wrong = Outcome::Accepted("(block (return (id x)))".to_owned());
    assert!(compare(&ret_prod, &ret_wrong).is_err());

    let (tc_prod, _) = run_production("{ f<T>(x) }");
    let tc_wrong = Outcome::Accepted("(block (gt (lt (id f) (id T)) (group (id x))))".to_owned());
    assert!(compare(&tc_prod, &tc_wrong).is_err());
}

#[test]
fn f3d_typed_calls_and_comparison_fallbacks_match_reference() {
    let accepted = [
        ("simple-typed-call", "{ f<T>(x) }"),
        ("typed-call-crossline-paren", "{\n f<T>\n (x)\n}"),
        ("nested-generic-typed-call", "{ f<Outer<Inner?>>(x, y) }"),
        ("typed-call-same-line-lambda", "{ f<T> { x() } }"),
        ("typed-call-crossline-lambda", "{\n f<T>()\n { x() }\n}"),
        ("crossline-call-inside-parens", "{\n f(g\n (x)) { y() }\n}"),
        ("comparison-less-fallback", "{ a < b }"),
        ("comparison-add-fallback", "{ a < b + c }"),
        ("grouped-comparisons", "{ (a < b) && (b > c) }"),
        ("member-typed-call", "{ obj.run<Int>(1) }"),
    ];
    for (label, src) in accepted {
        assert_accepted_match(label, src);
    }

    assert_rejected_match("chained-comparison", "{ a < b > c }", "L0012");
    assert_rejected_match("trailing-comma-in-args", "{ f<T>(x,) { y() } }", "L0036");
    let (prod, _) = run_production("{ f<T> move { x } }");
    assert_eq!(prod, Outcome::Rejected);
    assert_eq!(run_reference("{ f<T> move { x } }"), Outcome::Rejected);
}

#[test]
fn spec_0479_blocks_trailing_lambdas_move_and_when_match_reference() {
    let accepted = [
        ("explicit-block", "{ block { val x = 1\n work(x) } }"),
        ("nested-explicit-blocks", "{ block { block { x() } } }"),
        (
            "ordinary-block-name",
            "{ val block = 1\n block(block)\n obj.block()\n obj.block { x() } }",
        ),
        ("same-line-trailing-lambda", "{ html { render() } }"),
        (
            "crossline-closed-call-lambda",
            "{\n val b = f()\n { x() }\n}",
        ),
        (
            "comment-separated-crossline-lambda",
            "{\n val c = f() // why\n { work() }\n}",
        ),
        (
            "block-comment-separated-crossline-lambda",
            "{\n val d = f() /* why */\n { work() }\n}",
        ),
        (
            "lambda-headers",
            "{\n f { -> 1 }\n f { a -> a }\n f { a, b -> a + b }\n}",
        ),
        (
            "grouped-and-move-lambdas",
            "{\n ({ x() })\n move { x() }\n val m = move\n { 1 }\n move(1)\n obj.move()\n}",
        ),
        (
            "return-newline-cutoff",
            "{\n return\n x\n return x\n return/*\n*/x\n}",
        ),
        ("if-control-block", "{ if (ok) { x() } else { y() } }"),
        (
            "when-next-entry-lambda-call",
            "{\n when {\n  flag -> f()\n  { -> true }() -> g()\n }\n}",
        ),
        (
            "when-next-entry-compound-and",
            "{\n when {\n  flag -> f()\n  { -> true }() && flag -> g()\n }\n}",
        ),
        (
            "when-next-entry-multiline-and",
            "{\n when {\n  flag -> f()\n  { -> true }() &&\n  flag -> g()\n }\n}",
        ),
        (
            "when-next-entry-after-return-call",
            "{\n when {\n  flag -> return f()\n  { -> true }() -> g()\n }\n}",
        ),
        (
            "when-next-entry-comma-list",
            "{\n when {\n  flag -> f()\n  { -> true }(), other -> g()\n }\n}",
        ),
        (
            "when-attached-trailing-lambda",
            "{\n when {\n  flag -> f()\n  { work() }\n  other -> g()\n }\n}",
        ),
    ];
    for (label, src) in accepted {
        assert_accepted_match(label, src);
    }

    assert_rejected_match("bare-block-in-block", "{ { x() } }", "L0030");
    assert_rejected_match("bare-block-in-lambda", "{ f { { x() } } }", "L0032");
    assert_rejected_match("expr-position-block", "{ val x = block { 1 } }", "L0009");
    assert_rejected_match("open-call-crossline-lambda", "{\n f\n { x() }\n}", "L0030");
    let (prod, _) = run_production("{ f() { x() } { y() } }");
    assert_eq!(prod, Outcome::Rejected);
    assert_eq!(run_reference("{ f() { x() } { y() } }"), Outcome::Rejected);
}

#[test]
fn combinatorial_f3d_matrix_matches_reference() {
    let calls = ["f()", "f(1)", "f<T>()", "f<Outer<Inner>>(1)", "obj.f()"];
    let gaps = [" ", "\n", " // note\n", " /* note */\n"];
    let bodies = ["{ x() }", "{ -> 1 }", "{ a -> a }", "{ block { x() } }"];

    let mut total = 0usize;
    for (ci, call) in calls.iter().enumerate() {
        for (gi, gap) in gaps.iter().enumerate() {
            for (bi, body) in bodies.iter().enumerate() {
                let src = format!("{{\n    val r = {call}{gap}{body}\n    return\n    r\n}}");
                assert_accepted_match(&format!("f3d-combo-{ci}-{gi}-{bi}"), &src);
                total += 1;
            }
        }
    }
    assert_eq!(total, 80);
}
