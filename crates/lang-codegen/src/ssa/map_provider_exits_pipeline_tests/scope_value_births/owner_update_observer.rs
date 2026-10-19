//! Original bounded owner-update inputs; failures remain ordinary positive observations.
mod ast;
mod facts;
mod graph;
mod provider_graph;
use super::super::{
    Report,
    fixture::{Failure, clean, failure, need},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) struct Case {
    pub label: &'static str,
    pub source: &'static str,
    pub golden: &'static str,
    pub code: u8,
    provider: bool,
}
pub(crate) fn cases() -> [Case; 10] {
    macro_rules! case {
        ($label:literal,$code:literal,$provider:literal) => {
            Case {
                label: $label,
                code: $code,
                provider: $provider,
                source: include_str!(concat!(
                    "../../../../tests/fixtures/map-provider-scope-0331/",
                    $label,
                    ".ko"
                )),
                golden: include_str!(concat!(
                    "../../../../tests/fixtures/map-provider-scope-0331/",
                    $label,
                    ".stdout"
                )),
            }
        };
    }
    [
        case!("P07-for", 7, true),
        case!("P07-control", 7, false),
        case!("P08-for", 8, true),
        case!("P08-control", 8, false),
        case!("P09-for", 9, true),
        case!("P09-control", 9, false),
        case!("P10-for", 10, true),
        case!("P10-control", 10, false),
        case!("P11-for", 11, true),
        case!("P11-control", 11, false),
    ]
}
fn observe(c: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "SCOPE C UPDATE INPUT label={} unit={unit} source={:?} golden={:?}",
        c.label, c.source, c.golden
    );
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("provider.ko", c.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, source).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let p = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", p.diagnostics())?;
    let i = ast::input(c, &sources, &p)?;
    println!(
        "SCOPE C UPDATE AST label={} unit={unit} input={i:#?}",
        c.label
    );
    let mut report = Report::default();
    let mut observed = false;
    let mut proof = None;
    let lowered = if unit {
        pipeline::unit_observed(&sources, &p, "provider.ko", &mut report, |n, t, o, r| {
            observed = true;
            let query = facts::unit(c, &p, &i, n, t, o);
            r.query("update original qualified AST/type/ownership facts", &query);
            proof = query.ok();
        })
    } else {
        pipeline::single_observed(&sources, &p, &mut report, |n, t, o, r| {
            observed = true;
            let query = facts::single(c, &p, &i, n, t, o);
            r.query("update original AST/type/ownership facts", &query);
            proof = query.ok();
        })
    };
    println!(
        "SCOPE C UPDATE LOWER label={} unit={unit} observed={observed} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    need(
        observed,
        "update original analysis observer actually ran",
        c.label,
    )?;
    let (program, _) = lowered?;
    if c.provider {
        let query = proof
            .as_ref()
            .ok_or_else(|| failure("update exact same-analysis epoch cleanup proof", &report))
            .and_then(|proof| provider_graph::check(c, &p, &i, proof, &program));
        report.query(
            "update complete current For owner/provider/all incoming/protected uses",
            &query,
        );
    } else {
        report.query(
            "update complete actual control owner paths",
            &graph::check(c, &p, &i, &program),
        );
    }
    let verified = crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e));
    report.query("update actual SSA verifier", &verified);
    let llvm = crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e));
    report.query("update actual LLVM verifier", &llvm);
    println!(
        "SCOPE C UPDATE FINAL label={} unit={unit} report={report:#?}",
        c.label
    );
    need(
        report.passed(),
        "update original positive same-analysis observation",
        report,
    )
}
#[test]
fn original_owner_updates_report_real_facts() {
    let mut results = Vec::new();
    for c in cases() {
        for unit in [false, true] {
            if c.code <= 8 && !unit {
                continue;
            }
            let result = observe(&c, unit);
            println!(
                "SCOPE C UPDATE CASE label={} unit={unit} result={result:?}",
                c.label
            );
            results.push((c.label, unit, result));
        }
    }
    assert_eq!(results.len(), 16);
    assert!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
}
