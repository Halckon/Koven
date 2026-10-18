//! Independent observations of the original outer syntax and canonical public owner.
use super::*;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolKind, UnitReferenceTarget,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, DropTarget, IterationPhiIncomingKind, LoanTarget,
        OwnershipCheckedFile, UnitDropTarget, UnitLoanTarget,
    },
    parser::{Expression, ParsedFile},
    type_checking::{CompilationUnitTypes, ParameterMode, TypedFile, UnitExpressionId},
};

// Deliberately inspect the source AST, without the production transparent-source helper.
fn chain(parsed: &ParsedFile, outer: ExpressionId) -> Vec<ExpressionId> {
    let mut ids = vec![outer];
    while let Expression::Group { expression } = parsed
        .ast()
        .expressions()
        .get(*ids.last().unwrap())
        .unwrap()
        .payload()
    {
        assert!(!ids.contains(expression));
        ids.push(*expression);
    }
    ids
}

pub(in super::super) fn single(
    _sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    for d in typed.map_iterations() {
        let plan = owned.map_iteration(d.statement());
        report.check(
            "single original descriptor",
            plan.is_some_and(|p| p.descriptor() == d),
            d,
        );
        let Some(plan) = plan else {
            continue;
        };
        let ids = chain(parsed, d.source());
        let leaf = *ids.last().unwrap();
        let node = parsed.ast().expressions().get(leaf).unwrap();
        report.check("single outer AST and entire Group type/category chain", matches!(parsed.ast().statements().get(d.statement()).unwrap().payload(), Statement::For { source, .. } if *source == d.source()) && ids.iter().all(|&id| typed.expression_type(id) == Some(d.source_type()) && typed.expression_category(id) == typed.expression_category(d.source())), &ids);
        report.check(
            "single original outer loan sources and canonical origins",
            plan.loans().iter().all(|loan| {
                loan.statement() == d.statement()
                    && loan.source() == d.source()
                    && Some(loan.origin()) == plan.source()
            }),
            plan.loans(),
        );
        match plan.source().unwrap() {
            LoanTarget::Temporary(owner) => {
                let drops = owned
                    .drops()
                    .iter()
                    .filter(|f| f.target() == DropTarget::Temporary(leaf))
                    .collect::<Vec<_>>();
                report.check(
                    "single canonical Call owns every original temporary Drop",
                    *owner == leaf
                        && matches!(node.payload(), Expression::Call { .. })
                        && !drops.is_empty()
                        && drops.iter().all(|f| f.value_origin() == node.span()),
                    &drops,
                );
                let entry = plan
                    .closure_phi_incomings()
                    .iter()
                    .filter(|i| i.kind() == IterationPhiIncomingKind::Entry)
                    .collect::<Vec<_>>();
                report.check(
                    "single temporary Entry point retains outer expression",
                    entry.len() == 1 && entry[0].point() == DropPoint::AfterExpression(d.source()),
                    entry,
                );
            }
            LoanTarget::Place(place) => {
                let references = names
                    .references()
                    .iter()
                    .filter(|r| r.span() == node.span())
                    .collect::<Vec<_>>();
                report.check("single canonical Name resolves to original place root", matches!(node.payload(), Expression::Name) && matches!(references.as_slice(), [r] if r.target() == &ReferenceTarget::Symbol(place.root())), &references);
                let symbol = &names.symbols()[place.root().index()];
                if symbol.kind() == SymbolKind::ValueParameter {
                    report.check(
                        "single parameter mode agrees with owned/shared public access",
                        matches!(
                            (typed.parameter_mode(place.root()), plan.source_access()),
                            (
                                Some(ParameterMode::Value),
                                lang_frontend::ownership_checking::MapIterationSourceAccess::Owned
                            ) | (
                                Some(ParameterMode::Borrow),
                                lang_frontend::ownership_checking::MapIterationSourceAccess::Shared
                            )
                        ),
                        (typed.parameter_mode(place.root()), plan.source_access()),
                    );
                }
            }
            other => report.check(
                "single source retains existing representation",
                false,
                other,
            ),
        }
    }
}

pub(in super::super) fn unit(
    _sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    for d in typed.map_iterations() {
        let plan = owned.map_iteration(d.statement());
        report.check(
            "unit original qualified descriptor",
            plan.is_some_and(|p| p.descriptor() == d),
            d,
        );
        let Some(plan) = plan else {
            continue;
        };
        let ids = chain(parsed, d.source().expression());
        let leaf = UnitExpressionId::new(d.source().source_unit(), *ids.last().unwrap());
        let node = parsed.ast().expressions().get(leaf.expression()).unwrap();
        report.check("unit outer AST and entire Group type/category chain", matches!(parsed.ast().statements().get(d.statement().statement()).unwrap().payload(), Statement::For { source, .. } if *source == d.source().expression()) && ids.iter().all(|&id| { let id=UnitExpressionId::new(d.source().source_unit(), id); typed.expression_type(id) == Some(d.source_type()) && typed.expression_category(id) == typed.expression_category(d.source()) }), &ids);
        report.check(
            "unit outer loan sources and canonical origins",
            plan.loans().iter().all(|loan| {
                loan.statement() == d.statement()
                    && loan.source() == d.source()
                    && Some(loan.origin()) == plan.source()
            }),
            plan.loans(),
        );
        match plan.source().unwrap() {
            UnitLoanTarget::Temporary(owner) => {
                let drops = owned
                    .drops()
                    .iter()
                    .filter(|f| f.target() == UnitDropTarget::Temporary(leaf))
                    .collect::<Vec<_>>();
                report.check(
                    "unit canonical Call owns original temporary Drops",
                    *owner == leaf
                        && matches!(node.payload(), Expression::Call { .. })
                        && !drops.is_empty()
                        && drops.iter().all(|f| f.value_origin() == node.span()),
                    &drops,
                );
            }
            UnitLoanTarget::Place(place) => {
                let references = names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == node.span())
                    .collect::<Vec<_>>();
                report.check("unit canonical Name resolves to original qualified place root", matches!(node.payload(), Expression::Name) && matches!(references.as_slice(), [r] if r.target() == &UnitReferenceTarget::Symbol(place.root())), &references);
                let symbol = names
                    .names()
                    .source_units()
                    .iter()
                    .find(|s| s.source_unit() == place.root().source_unit())
                    .and_then(|s| s.resolution().symbols().get(place.root().symbol().index()))
                    .unwrap();
                if symbol.kind() == SymbolKind::ValueParameter {
                    let parameters = typed
                        .signatures()
                        .declarations()
                        .iter()
                        .filter_map(|s| s.callable())
                        .flat_map(|c| c.parameters())
                        .filter(|p| p.symbol() == Some(place.root()))
                        .collect::<Vec<_>>();
                    report.check("unit parameter mode agrees with owned/shared public access", matches!(parameters.as_slice(), [p] if matches!((p.mode(),plan.source_access()), (ParameterMode::Value,lang_frontend::ownership_checking::UnitIterationSourceAccess::Owned) | (ParameterMode::Borrow,lang_frontend::ownership_checking::UnitIterationSourceAccess::Shared))), parameters);
                }
            }
            other => report.check("unit source retains existing representation", false, other),
        }
    }
}
