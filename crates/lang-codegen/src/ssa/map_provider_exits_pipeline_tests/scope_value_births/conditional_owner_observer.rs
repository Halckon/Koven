//! Original Unit If-owned Map observations; every case retains its positive oracle.
mod ast;
mod facts;
mod graph;
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
    provider: bool,
    temporary: bool,
}
pub(crate) fn cases() -> [Case; 4] {
    macro_rules! case {
        ($label:literal, $provider:literal, $temporary:literal) => {
            Case {
                label: $label,
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
                provider: $provider,
                temporary: $temporary,
            }
        };
    }
    [
        case!("P03-for", true, false),
        case!("P03-control", false, false),
        case!("P04-for", true, true),
        case!("P04-control", false, false),
    ]
}
fn observe(c: &Case) -> Result<(), Failure> {
    println!(
        "SCOPE B IF INPUT label={} unit=true source={:?} golden={:?}",
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
    println!("SCOPE B IF AST label={} unit=true input={i:#?}", c.label);
    let mut report = Report::default();
    let mut observed = false;
    let mut proof = None;
    let lowered =
        pipeline::unit_observed(&sources, &p, "provider.ko", &mut report, |n, t, o, r| {
            observed = true;
            let checked = facts::check(c, &p, &i, n, t, o);
            r.query(
                "If original qualified type/call/owner/plan/drop observations",
                &checked,
            );
            proof = checked.ok();
        });
    println!(
        "SCOPE B IF LOWER label={} unit=true observed={observed} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    need(
        observed,
        "If observer received the original Unit analysis",
        c.label,
    )?;
    let (program, _) = lowered?;
    let proof = proof
        .as_ref()
        .ok_or_else(|| failure("If same-analysis original observation proof", &report))?;
    let query = graph::check(c, &p, &i, proof, &program);
    report.query(
        "If actual branches/current owner/full provider and ordinary cleanup",
        &query,
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    println!(
        "SCOPE B IF FINAL label={} unit=true report={report:#?}",
        c.label
    );
    need(
        report.passed(),
        "If original positive same-analysis observation",
        report,
    )
}
#[test]
fn original_unit_if_owner_sources_report_real_facts() {
    let mut results = Vec::new();
    for c in cases() {
        let result = observe(&c);
        println!(
            "SCOPE B IF CASE label={} unit=true result={result:?}",
            c.label
        );
        results.push((c.label, result));
    }
    assert_eq!(results.len(), 4);
    assert!(results.iter().all(|(_, r)| r.is_ok()), "{results:#?}");
}
