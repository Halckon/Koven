//! Newly rebuilt ordinary Borrow Map exits; not a restoration of the unavailable cloud collector.
//! MemoryDocument/rooted Unit pipelines and verifiers must succeed before separate native targets run.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod identity {
    //! Read-only original source references and caller delivery, independent of production qualification.
    use super::super::Report;
    use lang_frontend::{
        name_resolution::{SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames},
        ownership_checking::{
            CompilationUnitOwnership, LoanKind, UnitCallArgumentOwnershipKind, UnitLoanTarget,
        },
        parser::{Expression, ParsedFile},
        type_checking::{CompilationUnitTypes, ParameterMode, UnitCallTarget, UnitExpressionId},
    };

    pub(super) fn unit(
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        expected_calls: usize,
        report: &mut Report,
    ) {
        let [source] = names.names().source_units() else {
            report.check(
                "unit one original source",
                false,
                names.names().source_units().len(),
            );
            return;
        };
        let unit = source.source_unit();
        let scans = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| {
                d.name() == "scan" && d.kind() == SymbolKind::Function && d.source_unit() == unit
            })
            .collect::<Vec<_>>();
        let [scan] = scans.as_slice() else {
            report.check("unit original scan declaration", false, scans);
            return;
        };
        let scan_symbols = source
            .resolution()
            .symbols()
            .iter()
            .filter(|s| s.name() == "scan" && s.kind() == SymbolKind::Function && !s.is_synthetic())
            .map(|s| s.id())
            .collect::<Vec<_>>();
        let signatures = typed
            .signatures()
            .declarations()
            .iter()
            .filter(|d| d.symbol().source_unit() == unit && scan_symbols.contains(&d.symbol().symbol()))
            .filter_map(|d| d.callable())
            .collect::<Vec<_>>();
        let parameter = match signatures.as_slice() {
            [s] => match s.parameters() {
                [p] if p.mode() == ParameterMode::Borrow => p.symbol(),
                _ => None,
            },
            _ => None,
        };
        report.check(
            "unit scan actual Borrow parameter",
            parameter.is_some(),
            &signatures,
        );
        for plan in owned.map_iterations() {
            let span = parsed
                .ast()
                .expressions()
                .get(plan.descriptor().source().expression())
                .map(|n| n.span());
            let refs = names
                .names()
                .references()
                .iter()
                .filter(|r| span.as_ref().is_ok_and(|s| r.span() == *s))
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                })
                .collect::<Vec<_>>();
            report.check("unit original source reference equals exact scan parameter and planned root",
                matches!(refs.as_slice(), [root] if Some(*root) == parameter && root.source_unit() == unit && source.resolution().symbols().iter().any(|s| s.id() == root.symbol() && s.name() == "source" && s.kind() == SymbolKind::ValueParameter && !s.is_synthetic()) && matches!(plan.source(), Some(UnitLoanTarget::Place(p)) if p.is_root() && p.root() == *root)), (refs, parameter, plan.source()));
        }
        let mut calls = 0;
        for (id, node) in parsed.ast().expressions().iter() {
            let qualified = UnitExpressionId::new(unit, id);
            let Some(call) = typed
                .call(qualified)
                .filter(|c| c.target() == UnitCallTarget::Declaration(scan.id()))
            else {
                continue;
            };
            calls += 1;
            let Expression::Call { arguments, .. } = node.payload() else {
                report.check("unit original scan call AST", false, id);
                continue;
            };
            let [argument] = arguments.as_slice() else {
                report.check("unit scan original argument count", false, arguments);
                continue;
            };
            let argument_id = UnitExpressionId::new(unit, argument.value);
            let span = parsed
                .ast()
                .expressions()
                .get(argument.value)
                .map(|n| n.span());
            let refs = names
                .names()
                .references()
                .iter()
                .filter(|r| span.as_ref().is_ok_and(|s| r.span() == *s))
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let contracts = owned
                .call_argument_contracts_for(qualified)
                .collect::<Vec<_>>();
            let loans = owned
                .loans()
                .iter()
                .filter(|l| l.call() == qualified)
                .collect::<Vec<_>>();
            let deliveries = owned
                .value_deliveries()
                .iter()
                .filter(|d| d.call() == qualified)
                .collect::<Vec<_>>();
            report.check("unit original caller Shared contract and loan, with no Move delivery",
                call.receiver().is_none() && matches!(call.arguments(), [a] if a.argument_index() == 0 && a.parameter_index() == 0 && a.mode() == ParameterMode::Borrow && Some(a.parameter_type()) == typed.expression_type(argument_id))
                && matches!(contracts.as_slice(), [c] if c.argument() == argument_id && c.parameter_index() == 0 && c.kind() == UnitCallArgumentOwnershipKind::SharedLoan)
                && matches!(refs.as_slice(), [root] if root.source_unit() == unit && matches!(loans.as_slice(), [l] if l.argument() == argument_id && l.kind() == LoanKind::Shared && matches!(l.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == *root)))
                && deliveries.is_empty(), (qualified, call, refs, contracts, loans, deliveries));
        }
        report.check(
            "unit exact original caller count",
            calls == expected_calls,
            (calls, expected_calls),
        );
    }
}

use super::{
    Report,
    fixture::{Failure, clean, failure, verify_failure},
    pipeline,
};
use lang_frontend::{
    lexer::lex,
    name_resolution::ValidatedCompilationUnitNames,
    ownership_checking::{
        CompilationUnitOwnership, OwnershipBindingKind, UnitDropTarget, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{CompilationUnitTypes, MapIterationKind},
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

fn observe(label: &str, text: &str, rooted: bool, report: &mut Report) -> Result<(), Failure> {
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
    let observe = |names: &ValidatedCompilationUnitNames,
                   typed: &CompilationUnitTypes,
                   owned: &CompilationUnitOwnership,
                   report: &mut Report| {
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
    };
    let (program, entry) = if rooted {
        pipeline::unit_observed(&sources, &parsed, "provider.ko", report, observe)?
    } else {
        pipeline::memory_document_observed(&sources, &parsed, report, observe)?
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
        for rooted in [false, true] {
            let input = super::input_label(rooted);
            let mut report = Report::default();
            let outcome = observe(label, text, rooted, &mut report);
            diagnostic!(
                "SPEC0331 BORROW EXIT case={label} input={input} outcome={outcome:?} report={report:#?}"
            );
            results.push((label, input, report.passed() && outcome.is_ok()));
        }
    }
    assert_diagnostic!(
        results.len() == 8 && results.iter().all(|(_, _, passed)| *passed),
        "all eight original real pipelines must pass: {results:?}"
    );
}
