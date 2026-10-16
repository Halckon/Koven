//! Ordinary borrowed-for/nullable-while first-red candidates; no fabricated facts.
use super::{collect_sources, holder, verify_program};

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
                Ok(())
            });
            println!(
                "BORROWED PROOF OUTCOME relation={relation} source={} unit={} {result:?}",
                index / 2,
                index % 2 == 1,
            );
            result
        })
        .collect::<Vec<_>>();
    // Retain all original/Group single/unit verify/render outcomes before asserting success.
    assert_eq!(outcomes.len(), 4);
    for outcome in outcomes {
        outcome.expect("ordinary borrowed for and nullable while preserve enclosing access");
    }
}
