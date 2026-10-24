//! Original D6 positive observations with same-analysis owner and actual provider proofs.
mod ast;
mod facts;
pub(super) mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure, need},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) struct Case {
    pub label: &'static str,
    pub source: &'static str,
    pub golden: &'static str,
    code: u8,
    provider: bool,
}
pub(crate) fn cases() -> [Case; 6] {
    macro_rules! case {
        ($label:literal,$code:literal,$provider:literal) => {
            Case {
                label: $label,
                code: $code,
                provider: $provider,
                source: include_str!(concat!(
                    "../../../tests/fixtures/map-provider-scope-0331/",
                    $label,
                    ".ko"
                )),
                golden: include_str!(concat!(
                    "../../../tests/fixtures/map-provider-scope-0331/",
                    $label,
                    ".stdout"
                )),
            }
        };
    }
    [
        case!("P19-for", 19, true),
        case!("P19-control", 19, false),
        case!("P20-for", 20, true),
        case!("P20-control", 20, false),
        case!("P21-for", 21, true),
        case!("P21-control", 21, false),
    ]
}
fn observe(c: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "SCOPE D FUTURE INPUT label={} rooted={unit} source={:?} golden={:?}",
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
        "SCOPE D FUTURE AST label={} rooted={unit} input={i:#?}",
        c.label
    );
    let mut report = Report::default();
    let mut observed = false;
    let mut proof = None;
    let lowered = pipeline::unit_input_observed(
        &sources,
        &p,
        "provider.ko",
        &mut report,
        unit,
        |n, t, o, r| {
            observed = true;
            let result = facts::unit(c, &p, &i, n, t, o);
            r.query(
                "D original qualified future owner/return/move/put facts",
                &result,
            );
            proof = result.ok();
        },
    );
    println!(
        "SCOPE D FUTURE LOWER label={} rooted={unit} observed={observed} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    need(observed, "D same original analysis observer ran", c.label)?;
    let (program, entry) = lowered?;
    let graph = proof
        .as_ref()
        .ok_or_else(|| failure("D mandatory original FE identity proof", &report))
        .and_then(|proof| graph::check(c, &p, &i, proof, &program, entry));
    report.query(
        "D original complete provider/control owner delivery and cleanup",
        &graph,
    );
    report.query(
        "D actual SSA verifier",
        &crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e)),
    );
    report.query(
        "D actual LLVM verifier",
        &crate::llvm::render_verified_program_with_entry(&program, entry)
            .map(|_| ())
            .map_err(|e| failure("LLVM", e)),
    );
    println!(
        "SCOPE D FUTURE FINAL label={} rooted={unit} report={report:#?}",
        c.label
    );
    need(
        report.passed(),
        "D original positive ordinary observation",
        report,
    )
}
#[test]
fn original_future_owner_handoffs_report_real_facts() {
    let mut outcomes = Vec::new();
    for c in cases() {
        for unit in [false, true] {
            let result = observe(&c, unit);
            println!(
                "SCOPE D FUTURE CASE label={} rooted={unit} result={result:?}",
                c.label
            );
            outcomes.push((c.label, unit, result));
        }
    }
    assert_eq!(outcomes.len(), 12);
    assert!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}
