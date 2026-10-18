//! Real Resource Map continuations: lexical owner lifetime is independent of statement shape.
pub(super) mod borrowed;
pub(super) mod graph;
use super::super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: &'static str,
    pub owned: bool,
    pub source: String,
    pub golden: String,
}

pub(crate) fn sources() -> Vec<Case> {
    let base =
        include_str!("../../../../tests/fixtures/map-provider-resource-0331/map-resource.ko");
    let mut cases = Vec::new();
    for (label, prefix, suffix, nonempty, empty) in [
        (
            "original-extra",
            "",
            "println(\"exhausted\"); println(\"extra-continuation\")",
            "exhausted\nextra-continuation\n",
            "exhausted\nextra-continuation\n",
        ),
        ("no-continuation", "", "", "", ""),
        (
            "prefix-and-calls",
            "println(\"before-for\");",
            "println(\"first\"); println(\"second\"); println(\"third\")",
            "first\nsecond\nthird\n",
            "first\nsecond\nthird\n",
        ),
        (
            "later-source-use",
            "",
            "readMap(source); println(\"tail\")",
            "later-borrow\ntail\n",
            "later-borrow\ntail\n",
        ),
        (
            "cfg-then-source-use",
            "",
            "if (source.size == 0) { println(\"zero\") } else { println(\"two\") }; readMap(source); println(\"tail\")",
            "two\nlater-borrow\ntail\n",
            "zero\nlater-borrow\ntail\n",
        ),
    ] {
        let mut source = base
            .replace(
                "  for ((key, value) in source)",
                &format!("  {prefix} for ((key, value) in source)"),
            )
            .replace("println(\"exhausted\")", suffix)
            + "\nfun readMap(source: Map<String, Resource>): Unit { println(\"later-borrow\") }\n";
        if label == "original-extra" {
            source = include_str!("../../../../tests/fixtures/map-provider-resource-0331/boundary-extra-continuation.ko").to_owned();
        }
        let before = if prefix.is_empty() {
            ""
        } else {
            "before-for\n"
        };
        let golden = format!(
            "nonempty\n{before}z\nvalue-borrow\na\nvalue-borrow\n{nonempty}resource-drop\nresource-drop\nempty\n{before}{empty}entry-end\n"
        );
        cases.push(Case {
            label,
            owned: true,
            source,
            golden,
        });
    }
    cases
}

pub(super) fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    observe_with(case, unit, |_, _, _, _| Ok(()))
}
pub(super) fn observe_with(
    case: &Case,
    unit: bool,
    inspect: impl FnOnce(
        &crate::ssa::model::Program,
        &super::identity::Input,
        &SourceMap,
        &lang_frontend::parser::ParsedFile,
    ) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", &case.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex diagnostics", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse diagnostics", parsed.diagnostics())?;
    let input = super::identity::ast(&sources, &parsed, case.owned)?;
    let mut report = Report::default();
    let outcome = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                report.query(
                    "actual Resource FE scope",
                    &super::identity::unit(&input, &parsed, names, typed, owned),
                );
            },
        )
    } else {
        pipeline::single_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                report.query(
                    "actual Resource FE scope",
                    &super::identity::single(&input, &parsed, names, typed, owned),
                );
            },
        )
    };
    println!(
        "RESOURCE CONTINUATION case={} unit={unit} report={report:#?} outcome={:?}",
        case.label,
        outcome.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = outcome?;
    assert!(report.passed(), "{report:#?}");
    if case.owned {
        graph::check(&program, &input, &sources, &parsed)?;
    } else {
        borrowed::check(&program, &input, &parsed)?;
    }
    inspect(&program, &input, &sources, &parsed)?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA verifier", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM verifier", e))?;
    Ok(())
}

#[test]
fn resource_continuations_reach_both_verified_pipelines() {
    let results = sources()
        .iter()
        .flat_map(|case| [false, true].map(|unit| (case.label, unit, observe(case, unit))))
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 10);
    assert!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
}
