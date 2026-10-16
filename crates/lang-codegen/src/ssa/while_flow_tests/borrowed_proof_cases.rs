//! Ordinary borrowed-for/nullable-while first-red candidates; no fabricated facts.
use lang_frontend::{
    lexer::lex,
    parser::{Expression, Item, NameMarker, Statement, parse_file},
    source::{SourceMap, Span},
};

use super::{collect_sources, holder, verify_program};

#[path = "borrowed_proof_contract.rs"]
mod borrowed_proof_contract;

struct Spans {
    outer: Span,
    borrowed: Span,
    construct: Span,
    branch: Span,
    continued: Span,
    bindings: [Span; 2],
    members: [Span; 2],
}

pub(super) fn borrowed_for_inside_while_source() -> String {
    holder(
        "val items = listOf(1, 2)\nwhile (current != null) {\nfor (item in items) { if (item == 1) { continue }\ntotal = total + item + current.value }\nreturn total + current.value\n}\nreturn total",
    )
}

pub(super) fn while_inside_borrowed_for_source() -> String {
    holder(
        "val items = listOf(1, 2)\nfor (item in items) {\nwhile (current != null) { total = total + item + current.value\nbreak }\ntotal = total + item\n}\nreturn total",
    )
}

pub(super) fn borrowed_for_inside_while_sources() -> [String; 2] {
    let source = borrowed_for_inside_while_source();
    let grouped = source.replace("in items", "in ((items))");
    [source, grouped]
}

pub(super) fn while_inside_borrowed_for_sources() -> [String; 2] {
    let source = while_inside_borrowed_for_source();
    let grouped = source.replace("in items", "in ((items))");
    [source, grouped]
}

#[test]
fn borrowed_for_preserves_the_outer_nullable_while_proof() {
    observe_borrowed_sources("for-inside-while", &borrowed_for_inside_while_sources());
}

#[test]
fn nullable_while_preserves_the_outer_borrowed_for_access() {
    observe_borrowed_sources("while-inside-for", &while_inside_borrowed_for_sources());
}

fn observe_borrowed_sources(relation: &str, sources: &[String; 2]) {
    let outcomes = collect_sources(sources)
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            let result = result.and_then(|program| {
                verify_program(&program).map_err(|error| format!("SSA verify: {error:?}"))?;
                let llvm = crate::llvm::render_verified_program(&program)
                    .map_err(|error| format!("LLVM: {error:?}"))?;
                println!("BORROWED PROOF LLVM relation={relation} case={index} {llvm}");
                Ok(program)
            });
            println!(
                "BORROWED PROOF OUTCOME relation={relation} source={} unit={} {:?}",
                index / 2,
                index % 2 == 1,
                result.as_ref().map(|_| ()),
            );
            result
        })
        .collect::<Vec<_>>();
    // Retain all original/Group single/unit verify/render outcomes before asserting success.
    assert_eq!(outcomes.len(), 4);
    for (index, outcome) in outcomes.into_iter().enumerate() {
        let program =
            outcome.expect("ordinary borrowed for and nullable while preserve enclosing access");
        if relation == "for-inside-while" {
            let unit = index % 2 == 1;
            let spans = source_spans(&sources[index / 2], unit);
            borrowed_proof_contract::assert_borrowed_proof(&program, &spans, unit);
        }
    }
}

fn source_spans(source: &str, unit: bool) -> Spans {
    let text = if unit {
        format!("package p\n{source}")
    } else {
        source.to_owned()
    };
    let mut sources = SourceMap::new();
    let id = sources
        .add_source(if unit { "p/range.ko" } else { "range.ko" }, &text)
        .unwrap();
    let lexed = lex(&sources, id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty());
    let ast = file.ast();
    let outer = only(ast.statements().iter().filter_map(|(_, node)| {
        matches!(node.payload(), Statement::While { .. }).then_some(node.span())
    }));
    let (borrowed, mut source) = only(ast.statements().iter().filter_map(|(_, node)| {
        let Statement::For { source, .. } = node.payload() else {
            return None;
        };
        Some((node.span(), *source))
    }));
    while let Expression::Group { expression } = ast.expressions().get(source).unwrap().payload() {
        source = *expression;
    }
    let source = ast.expressions().get(source).unwrap();
    assert!(matches!(source.payload(), Expression::Name));
    assert_eq!(&text[source.span().start()..source.span().end()], "items");
    let bindings = ["current", "items"].map(|expected| {
        only(ast.items().iter().filter_map(|(_, node)| {
            let Item::Variable {
                name: NameMarker::Present(span),
                ..
            } = node.payload()
            else {
                return None;
            };
            (&text[span.start()..span.end()] == expected).then_some(*span)
        }))
    });
    let expression = |predicate: fn(&Expression) -> bool| {
        only(
            ast.expressions()
                .iter()
                .filter_map(|(_, node)| predicate(node.payload()).then_some(node.span())),
        )
    };
    let construct = expression(|node| matches!(node, Expression::Call { .. }));
    let branch = expression(|node| matches!(node, Expression::If { .. }));
    let continued = expression(|node| matches!(node, Expression::Continue { .. }));
    let mut members = ast
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            matches!(node.payload(), Expression::Member { .. }).then_some(node.span())
        })
        .collect::<Vec<_>>();
    members.sort_by_key(|span| span.start());
    let [inner, final_read] = members.as_slice() else {
        panic!("two real Members")
    };
    Spans {
        outer,
        borrowed,
        construct,
        branch,
        continued,
        bindings,
        members: [*inner, *final_read],
    }
}

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one real source node");
    assert!(values.next().is_none(), "source node is unique");
    value
}
