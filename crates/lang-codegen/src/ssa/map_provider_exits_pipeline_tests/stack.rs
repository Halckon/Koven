//! G8 source-only nested-provider contracts, with every real observation collected.
mod facts;
mod query;
mod runtime;
mod runtime_owned_resource;
mod runtime_variants;

use super::{
    Report,
    fixture::{Failure, clean, failure, verify_failure},
    pipeline,
};
use lang_frontend::{
    lexer::lex,
    parser::{Statement, parse_file},
    source::SourceMap,
};

pub(crate) use runtime::sources as runtime_sources;
pub(crate) use runtime_owned_resource::sources as owned_resource_sources;
pub(crate) use runtime_variants::sources as variant_sources;

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
}

fn originals(group: usize) -> Vec<(&'static str, &'static str)> {
    match group {
        0 => vec![
            (
                "different-root",
                include_str!(
                    "../../../tests/fixtures/map-provider-stack-0331/g8-nested-different-root.ko"
                ),
            ),
            (
                "own",
                include_str!("../../../tests/fixtures/map-provider-stack-0331/g8-nested-own.ko"),
            ),
            (
                "resource",
                include_str!(
                    "../../../tests/fixtures/map-provider-stack-0331/g8-nested-resource.ko"
                ),
            ),
        ],
        1 => vec![
            (
                "temporary",
                include_str!(
                    "../../../tests/fixtures/map-provider-stack-0331/g8-nested-temporary.ko"
                ),
            ),
            (
                "nullable-resource",
                include_str!(
                    "../../../tests/fixtures/map-provider-stack-0331/g8-nested-nullable-resource.ko"
                ),
            ),
        ],
        2 => vec![
            (
                "depth-three",
                include_str!("../../../tests/fixtures/map-provider-stack-0331/g8-depth-three.ko"),
            ),
            (
                "inner-conditional",
                include_str!(
                    "../../../tests/fixtures/map-provider-stack-0331/g8-inner-conditional.ko"
                ),
            ),
        ],
        _ => unreachable!("closed test group"),
    }
}

fn observe(label: &str, text: &str, unit: bool) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, source).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let count = parsed
        .ast()
        .statements()
        .iter()
        .filter(|(_, n)| matches!(n.payload(), Statement::For { .. }))
        .count();
    let mut report = Report::default();
    let mut inputs = None;
    let lowered = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |names, typed, owned, report| {
            let observed = facts::unit(&sources, &parsed, names, typed, owned);
            report.query("stack original qualified FE bridge", &observed);
            inputs = Some(observed);
            report.check(
                "stack all original For plans",
                owned.map_iterations().len() == count,
                owned.map_iterations(),
            );
        },
    );
    println!(
        "G8 STACK LOWER case={label} rooted={unit} report={report:#?} result={:?}",
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    let queried = inputs
        .as_ref()
        .ok_or_else(|| failure("stack original callback input", "missing"))
        .and_then(|input| {
            input
                .as_ref()
                .map_err(|e| failure("stack original FE bridge", e))
        })
        .and_then(|input| query::check(&sources, &parsed, &program, input, true));
    println!("G8 STACK QUERY case={label} rooted={unit} result={queried:?}");
    report.query("stack independent complete identities", &queried);
    crate::ssa::verify::verify_program(&program).map_err(|e| verify_failure(&sources, e))?;
    let llvm = crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    println!(
        "G8 STACK SSA case={label} rooted={unit}\n{}",
        crate::ssa::render::render_program(&program)
    );
    println!("G8 STACK LLVM case={label} rooted={unit}\n{llvm}");
    if !report.passed() {
        return Err(failure("stack FE/SSA identities", report));
    }
    Ok(())
}

fn group(index: usize, expected: usize) {
    let mut observations = Vec::new();
    for (label, source) in originals(index) {
        for unit in [false, true] {
            println!("G8 STACK INPUT case={label} rooted={unit} source={source:?}");
            let result = observe(label, source, unit);
            println!("G8 STACK CASE case={label} rooted={unit} result={result:?}");
            observations.push(result);
        }
    }
    assert_eq!(observations.len(), expected);
    assert!(observations.iter().all(Result::is_ok), "{observations:#?}");
}

#[test]
fn different_root_owned_and_resource_stack_reach_verified_llvm() {
    group(0, 6);
}

#[test]
fn temporary_and_nullable_stack_reach_verified_llvm() {
    group(1, 4);
}

#[test]
fn depth_three_and_inner_conditional_stack_reach_verified_llvm() {
    group(2, 4);
}

#[test]
fn populated_empty_inner_and_empty_outer_stack_reach_both_pipelines() {
    runtime_group(runtime_sources(), 12);
}

#[test]
fn owned_resource_same_root_preserves_source_and_cleanup_identity() {
    runtime_group(owned_resource_sources(), 4);
}

#[test]
fn temporary_nullable_and_three_layers_preserve_each_exit() {
    runtime_group(variant_sources(), 18);
}

fn runtime_group(cases: Vec<Case>, count: usize) {
    assert_eq!(cases.len(), count);
    let mut observations = Vec::new();
    for case in cases {
        for unit in [false, true] {
            println!(
                "G8 STACK RUNTIME INPUT case={} rooted={unit} source={:?} golden={:?}",
                case.label, case.source, case.golden
            );
            let result = observe(&case.label, &case.source, unit);
            println!(
                "G8 STACK RUNTIME CASE case={} rooted={unit} result={result:?}",
                case.label
            );
            observations.push(result);
        }
    }
    assert_eq!(observations.len(), count * 2);
    assert!(observations.iter().all(Result::is_ok), "{observations:#?}");
}
