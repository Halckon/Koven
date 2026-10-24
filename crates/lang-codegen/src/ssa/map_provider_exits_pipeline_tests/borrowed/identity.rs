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
