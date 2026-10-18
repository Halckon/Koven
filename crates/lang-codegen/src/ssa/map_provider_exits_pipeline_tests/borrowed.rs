//! Newly rebuilt ordinary Borrow Map exits; not a restoration of the unavailable cloud collector.
//! Both actual pipelines and verifiers must succeed before the separate native targets run.
mod identity;

use super::{
    Report,
    fixture::{Failure, clean, failure, verify_failure},
    pipeline,
};
use lang_frontend::{
    lexer::lex,
    ownership_checking::{
        DropTarget, LoanTarget, MapIterationSourceAccess, OwnershipBindingKind, UnitDropTarget,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::MapIterationKind,
};

pub(crate) fn sources() -> [(&'static str, &'static str, &'static str); 4] {
    [
        (
            "map-borrow-continue",
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-continue.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-continue.stdout"
            ),
        ),
        (
            "map-borrow-break",
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-break.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-break.stdout"
            ),
        ),
        (
            "map-borrow-return",
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-return.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-return.stdout"
            ),
        ),
        (
            "map-borrow-conditional",
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-conditional.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-conditional.stdout"
            ),
        ),
    ]
}

fn observe(label: &str, text: &str, unit: bool, report: &mut Report) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, source).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex diagnostics", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse diagnostics", parsed.diagnostics())?;
    let expected_calls = if label == "map-borrow-conditional" {
        5
    } else {
        2
    };
    let (program, entry) = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            report,
            |names, typed, owned, report| {
                identity::unit(&parsed, names, typed, owned, expected_calls, report);
                let plans = owned.map_iterations();
                report.check(
                    "unit one real borrowed Map provider",
                    plans.len() == 1,
                    plans,
                );
                for plan in plans {
                    let descriptor = plan.descriptor();
                    let origin = descriptor.source();
                    let span = parsed
                        .ast()
                        .expressions()
                        .get(origin.expression())
                        .map(|n| n.span());
                    let root = match plan.source() {
                        Some(UnitLoanTarget::Place(place)) if place.is_root() => Some(place.root()),
                        _ => None,
                    };
                    report.check(
                        "unit exact borrowed source identity and no owned source drop",
                        descriptor.kind() == MapIterationKind::Map
                            && typed.map_iteration(descriptor.statement()) == Some(descriptor)
                            && plan.source_access() == UnitIterationSourceAccess::Shared
                            && root.is_some_and(|root| {
                                owned.binding_kind(root) == Some(OwnershipBindingKind::Shared)
                                    && !owned
                                        .drops()
                                        .iter()
                                        .any(|d| d.target() == UnitDropTarget::Named(root))
                            })
                            && span.is_ok_and(|span| sources.slice(span) == Ok("source"))
                            && descriptor.key_binding() != descriptor.value_binding()
                            && plan.loans()[0] != plan.loans()[1],
                        (descriptor, root, span),
                    );
                }
            },
        )?
    } else {
        pipeline::single_observed(&sources, &parsed, report, |names, typed, owned, report| {
            identity::single(&parsed, names, typed, owned, expected_calls, report);
            let plans = owned.map_iterations();
            report.check(
                "single one real borrowed Map provider",
                plans.len() == 1,
                plans,
            );
            for plan in plans {
                let descriptor = plan.descriptor();
                let span = parsed
                    .ast()
                    .expressions()
                    .get(descriptor.source())
                    .map(|n| n.span());
                let root = match plan.source() {
                    Some(LoanTarget::Place(place)) if place.is_root() => Some(place.root()),
                    _ => None,
                };
                report.check(
                    "single exact borrowed source identity and no owned source drop",
                    descriptor.kind() == MapIterationKind::Map
                        && typed.map_iteration(descriptor.statement()) == Some(descriptor)
                        && plan.source_access() == MapIterationSourceAccess::Shared
                        && root.is_some_and(|root| {
                            owned.binding_kind(root) == Some(OwnershipBindingKind::Shared)
                                && !owned
                                    .drops()
                                    .iter()
                                    .any(|d| d.target() == DropTarget::Named(root))
                        })
                        && span.is_ok_and(|span| sources.slice(span) == Ok("source"))
                        && descriptor.key_binding() != descriptor.value_binding()
                        && plan.loans()[0] != plan.loans()[1],
                    (descriptor, root, span),
                );
            }
        })?
    };
    super::super::verify::verify_program(&program).map_err(|e| verify_failure(&sources, e))?;
    report.check("SSA verifier", true, entry);
    let llvm = crate::llvm::render_verified_program(&program)
        .map_err(|e| failure("LLVM render/verify", e))?;
    report.check("LLVM render/verify", true, llvm.len());
    Ok(())
}

#[test]
fn borrowed_map_direct_exits_reach_verified_llvm_in_both_entries() {
    let mut results = Vec::new();
    for (label, text, _) in sources() {
        for unit in [false, true] {
            let mut report = Report::default();
            let outcome = observe(label, text, unit, &mut report);
            println!(
                "SPEC0331 BORROW EXIT case={label} unit={unit} outcome={outcome:?} report={report:#?}"
            );
            results.push((label, unit, report.passed() && outcome.is_ok()));
        }
    }
    assert!(
        results.len() == 8 && results.iter().all(|(_, _, passed)| *passed),
        "all eight original real pipelines must pass: {results:?}"
    );
}
