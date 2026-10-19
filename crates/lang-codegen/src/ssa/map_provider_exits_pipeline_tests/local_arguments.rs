//! The already supported argument factory forms a named owner before the same provider.
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, temporary_arguments,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) fn sources() -> Vec<temporary_arguments::Case> {
    temporary_arguments::sources()
        .into_iter()
        .map(|mut case| {
            let old = "for((key,value) in produce(argument()))";
            assert_eq!(case.source.matches(old).count(), 1);
            case.source = case.source.replace(
                old,
                "val source=produce(argument());for((key,value) in source)",
            );
            case.label = case
                .label
                .replacen("temporary-argument", "local-argument", 1);
            case
        })
        .collect()
}
fn observe(text: &str, unit: bool) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let (program, _) = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |_, _, _, _| {},
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |_, _, _, _| {})
    }?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}
#[test]
fn selected_argument_factory_forms_named_owner_in_both_pipelines() {
    let mut results = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            println!(
                "LOCAL ARG INPUT label={} unit={unit} source={:?} golden={:?}",
                case.label, case.source, case.golden
            );
            let result = observe(&case.source, unit);
            println!(
                "LOCAL ARG CASE label={} unit={unit} result={result:?}",
                case.label
            );
            results.push(result);
        }
    }
    assert_eq!(results.len(), 4);
    assert!(results.iter().all(Result::is_ok), "{results:#?}");
}
