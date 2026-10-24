//! Independent observations of the original outer syntax and canonical public owner.
use super::*;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{CompilationUnitOwnership, UnitDropTarget, UnitLoanTarget},
    parser::{Expression, ParsedFile},
    type_checking::{CompilationUnitTypes, ParameterMode, UnitExpressionId},
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
