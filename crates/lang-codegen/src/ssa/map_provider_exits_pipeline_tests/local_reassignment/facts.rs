//! Exact same-analysis assignment epochs; declaration history never substitutes for current birth.
pub(super) mod ast;
use super::Case;
use crate::ssa::map_provider_exits_pipeline_tests::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE, UnitIterationSourceAccess,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{CompilationUnitTypes, UnitCallTarget, UnitExpressionId, UnitStatementId},
};

fn unit_reference(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    id: lang_frontend::ast::ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == ast::expression(parsed, id))
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "actual qualified source/root Name identity",
    )
}

pub(super) fn unit(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    for site in ast::sites(case, sources, parsed)? {
        let root = unit_reference(parsed, names, site.target)?;
        let u = root.source_unit();
        need(
            owned.formed_owned_locals().contains(&root),
            "qualified original formed root",
            root,
        )?;
        if let Some((_, init)) = site.alias {
            need(
                unit_reference(parsed, names, init)? == root,
                "qualified alias original root",
                init,
            )?;
        }
        let rhs_type = typed
            .expression_type(UnitExpressionId::new(u, site.rhs))
            .ok_or_else(|| failure("qualified canonical RHS type", site.rhs))?;
        let chain = ast::rhs_chain(parsed, site.rhs_outer);
        need(
            chain.last() == Some(&site.rhs),
            "qualified complete Group chain reaches canonical Call",
            &chain,
        )?;
        for id in chain {
            let id = UnitExpressionId::new(u, id);
            need(
                typed.expression_type(id) == Some(rhs_type)
                    && typed.expression_category(id)
                        == Some(lang_frontend::type_checking::ExpressionCategory::Temporary),
                "every qualified RHS Group layer preserves represented type and Temporary category",
                id,
            )?;
        }
        for (epoch, id) in [site.birth, site.rhs].into_iter().enumerate() {
            let id = UnitExpressionId::new(u, id);
            let call = typed
                .call(id)
                .ok_or_else(|| failure("qualified epoch Call", id))?;
            let UnitCallTarget::Declaration(decl) = call.target() else {
                return Err(failure("qualified source factory", call));
            };
            let signature = typed
                .signatures()
                .declaration(decl)
                .and_then(|s| s.callable())
                .ok_or_else(|| failure("actual factory signature", decl))?;
            need(
                site.factories[epoch].start() <= signature.name_span().start()
                    && signature.name_span().end() <= site.factories[epoch].end()
                    && call.receiver().is_none()
                    && call.arguments().is_empty()
                    && typed.expression_type(id) == Some(call.return_type()),
                "qualified epoch Call selected original declaration",
                call,
            )?;
        }
        let alias = unit_reference(parsed, names, site.loops.last().unwrap().source)?;
        let roots = [root, alias];
        let drops = owned
            .drops()
            .iter()
            .filter(|f| matches!(f.target(),UnitDropTarget::Named(r) if roots.contains(&r)))
            .collect::<Vec<_>>();
        need(
            drops.len() == 2,
            "qualified old/new obligations no alias duplicate",
            &drops,
        )?;
        let old_point = if case.payload != 0 {
            UnitDropPoint::AfterExpression(UnitExpressionId::new(u, site.rhs_outer))
        } else if case.history == 1 {
            UnitDropPoint::LoopExit(UnitStatementId::new(u, site.loops[0].statement))
        } else {
            UnitDropPoint::AfterStatement(UnitStatementId::new(u, site.declaration))
        };
        let last = site.loops.last().unwrap();
        let new_point = if case.payload != 0 {
            UnitDropPoint::AfterStatement(UnitStatementId::new(u, site.scope))
        } else {
            UnitDropPoint::LoopExit(UnitStatementId::new(u, last.statement))
        };
        let old = one(
            drops.iter().copied().filter(|d| d.point() == old_point),
            "qualified old epoch point",
        )?;
        let new = one(
            drops.iter().copied().filter(|d| d.point() == new_point),
            "qualified new epoch point",
        )?;
        let new_origin = if case.history == 2 {
            last.marker
        } else {
            ast::expression(parsed, site.assignment)
        };
        need(
            old.target() == UnitDropTarget::Named(root)
                && old.value_origin() == site.binding
                && new.target() == UnitDropTarget::Named(alias)
                && new.value_origin() == new_origin,
            "qualified actual assignment/alias epoch origin",
            (old, new),
        )?;
        for l in &site.loops {
            let statement = UnitStatementId::new(u, l.statement);
            let p = owned
                .map_iteration(statement)
                .ok_or_else(|| failure("qualified epoch plan", statement))?;
            let d = p.descriptor();
            let lr = unit_reference(parsed, names, l.source)?;
            need(
                typed.map_iteration(statement) == Some(d)
                    && d.source() == UnitExpressionId::new(u, l.source)
                    && p.source_access() == UnitIterationSourceAccess::Owned
                    && owned.formed_owned_locals().contains(&lr),
                "qualified epoch descriptor identity",
                p,
            )?;
            need(
                matches!(p.source(), Some(lang_frontend::ownership_checking::UnitLoanTarget::Place(place)) if place.is_root() && place.root()==lr)
                    && typed.expression_type(UnitExpressionId::new(
                        u,
                        if l.epoch == 0 { site.birth } else { site.rhs },
                    )) == Some(d.source_type())
                    && p.loans().len() == 2
                    && p.loans().iter().enumerate().all(|(index, loan)| {
                        loan.statement() == statement
                            && loan.source() == d.source()
                            && loan.binding() == d.bindings()[index]
                            && p.bindings()[index].kind()
                                == lang_frontend::ownership_checking::OwnershipBindingKind::Shared
                    }),
                "qualified epoch original root/RHS type and Shared component loans",
                p,
            )?;
            need(p.exits().len() == 2, "qualified complete epoch exits", p)?;
            for (kind, point, body) in [
                (
                    UE::Fallthrough,
                    UnitDropPoint::AfterStatement(UnitStatementId::new(u, l.body)),
                    true,
                ),
                (UE::Exhaustion, UnitDropPoint::LoopExit(statement), false),
            ] {
                let exit = one(
                    p.exits()
                        .iter()
                        .filter(|e| e.kind() == kind && e.point() == point),
                    "qualified exact normal exit",
                )?;
                let mut expected = owned
                    .drops()
                    .iter()
                    .copied()
                    .filter(|f| {
                        f.point() == point
                            && f.value_origin().start() > l.span.start()
                            && f.value_origin().end() < l.span.end()
                    })
                    .map(UA::Drop)
                    .collect::<Vec<_>>();
                if body {
                    need(
                        expected.len() == 1,
                        "qualified actual Guard prefix",
                        &expected,
                    )?;
                    expected.extend([
                        UA::EndBinding {
                            statement,
                            symbol: d.value_binding(),
                        },
                        UA::EndBinding {
                            statement,
                            symbol: d.key_binding(),
                        },
                    ]);
                } else {
                    expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
                    expected.extend(
                        owned
                            .drops()
                            .iter()
                            .copied()
                            .filter(|f| {
                                f.point() == point
                                    && !(f.value_origin().start() > l.span.start()
                                        && f.value_origin().end() < l.span.end())
                            })
                            .map(UA::Drop),
                    );
                }
                need(
                    exit.actions() == expected
                        && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
                    "qualified full cleanup order",
                    exit,
                )?;
            }
        }
    }
    Ok(())
}
