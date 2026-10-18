//! Exact same-analysis assignment epochs; declaration history never substitutes for current birth.
pub(super) mod ast;
mod phi;
use super::Case;
use crate::ssa::map_provider_exits_pipeline_tests::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolId, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupOwnerValue as O, CompilationUnitOwnership, DropPoint, DropTarget,
        IterationCleanupAction as A, IterationExitKind as E, IterationPhiBoundary as B,
        MapIterationSourceAccess, OwnershipCheckedFile, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as UA, UnitIterationExitKind as UE, UnitIterationSourceAccess,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{
        CallableTarget, CompilationUnitTypes, TypedFile, UnitCallTarget, UnitExpressionId,
        UnitStatementId,
    },
};
fn reference(
    parsed: &ParsedFile,
    names: &NameResolution,
    id: lang_frontend::ast::ExpressionId,
) -> Result<SymbolId, Failure> {
    one(
        names
            .references()
            .iter()
            .filter(|r| r.span() == ast::expression(parsed, id))
            .filter_map(|r| match r.target() {
                ReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "actual source/root Name identity",
    )
}
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

pub(super) fn single(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    for site in ast::sites(case, sources, parsed)? {
        let root = reference(parsed, names, site.target)?;
        need(
            names
                .symbols()
                .get(root.index())
                .is_some_and(|s| s.span() == site.binding)
                && owned.formed_owned_locals().contains(&root),
            "original declared formed root",
            root,
        )?;
        if let Some((_, init)) = site.alias {
            need(
                reference(parsed, names, init)? == root,
                "alias moves actual replaced root",
                init,
            )?;
        }
        for (epoch, id) in [site.birth, site.rhs].into_iter().enumerate() {
            let call = typed
                .call(id)
                .ok_or_else(|| failure("actual epoch typed Call", id))?;
            let CallableTarget::Source(target) = call.target() else {
                return Err(failure("actual source factory target", call));
            };
            need(parsed.roots().iter().any(|item|parsed.ast().items().get(*item).is_ok_and(|n|n.span()==site.factories[epoch] && matches!(n.payload(),lang_frontend::parser::Item::Function{name:lang_frontend::parser::NameMarker::Present(name),..} if names.symbols().get(target.index()).is_some_and(|s|s.span()==*name)))) && call.receiver().is_none() && call.arguments().is_empty() && typed.expression_type(id)==Some(call.return_type()),"epoch complete RHS chooses its original factory",call)?;
        }
        let alias = site
            .loops
            .last()
            .map(|l| reference(parsed, names, l.source))
            .transpose()?
            .unwrap();
        let roots = [root, alias];
        let drops = owned
            .drops()
            .iter()
            .filter(|f| matches!(f.target(),DropTarget::Named(s) if roots.contains(&s)))
            .collect::<Vec<_>>();
        need(
            drops.len() == 2,
            "exact old/new root obligations without alias duplicate",
            &drops,
        )?;
        let old_point = if case.payload != 0 {
            DropPoint::AfterExpression(site.rhs)
        } else if case.history == 1 {
            DropPoint::LoopExit(site.loops[0].statement)
        } else {
            DropPoint::AfterStatement(site.declaration)
        };
        let new_point = if case.payload != 0 {
            DropPoint::AfterStatement(site.scope)
        } else {
            DropPoint::LoopExit(site.loops.last().unwrap().statement)
        };
        let old = one(
            drops.iter().copied().filter(|f| f.point() == old_point),
            "actual old epoch Drop boundary",
        )?;
        let new = one(
            drops.iter().copied().filter(|f| f.point() == new_point),
            "actual new epoch Drop boundary",
        )?;
        need(
            old.target() == DropTarget::Named(root)
                && old.value_origin() == site.binding
                && new.target() == DropTarget::Named(alias)
                && new.value_origin() == site.loops.last().unwrap().marker
                && drops
                    .iter()
                    .all(|f| f.capture_slot().is_none() && f.instance_address().is_none()),
            "root Drop original epoch and alias identities",
            (&old, &new),
        )?;
        if case.history != 1 {
            need(
                old.condition().is_none()
                    && matches!(old.owner().and_then(|v|owned.cleanup_conditions().owner_value(v)),Some(O::Expression{expression,origin}) if *expression==site.birth && *origin==ast::expression(parsed,site.birth)),
                "old non-iterated epoch remains exact make Call owner",
                old,
            )?;
        }
        for l in &site.loops {
            let lr = reference(parsed, names, l.source)?;
            let p = owned
                .map_iteration(l.statement)
                .ok_or_else(|| failure("actual original epoch plan", l.statement))?;
            need(
                typed.map_iteration(l.statement) == Some(p.descriptor())
                    && p.descriptor().source() == l.source
                    && p.source_access() == MapIterationSourceAccess::Owned
                    && owned.formed_owned_locals().contains(&lr),
                "same-analysis epoch descriptor and formed source",
                p,
            )?;
            need(
                matches!(p.source(), Some(lang_frontend::ownership_checking::LoanTarget::Place(place)) if place.is_root() && place.root()==lr)
                    && typed.expression_type(if l.epoch == 0 { site.birth } else { site.rhs })
                        == Some(p.descriptor().source_type())
                    && p.loans().len() == 2
                    && p.loans().iter().enumerate().all(|(index, loan)| {
                        loan.statement() == l.statement
                            && loan.source() == l.source
                            && loan.binding() == p.descriptor().bindings()[index]
                            && owned.binding_kind(loan.binding())
                                == Some(
                                    lang_frontend::ownership_checking::OwnershipBindingKind::Shared,
                                )
                    }),
                "epoch exact root, represented RHS type and Shared component loans",
                p,
            )?;
            phi::check(
                &site,
                l,
                lr,
                p,
                owned,
                parsed,
                if l.epoch == 0 { old } else { new },
                case.payload != 0,
            )?;
            cleanup_single(parsed, owned, p, l)?;
        }
    }
    Ok(())
}
fn cleanup_single(
    parsed: &ParsedFile,
    owned: &OwnershipCheckedFile,
    p: &lang_frontend::ownership_checking::MapIterationOwnershipPlan,
    l: &ast::Loop,
) -> Result<(), Failure> {
    need(
        p.exits().len() == 2,
        "complete normal epoch exit set",
        p.exits(),
    )?;
    for (kind, point, body) in [
        (E::Fallthrough, DropPoint::AfterStatement(l.body), true),
        (E::Exhaustion, DropPoint::LoopExit(l.statement), false),
    ] {
        let exit = one(
            p.exits()
                .iter()
                .filter(|e| e.kind() == kind && e.point() == point),
            "actual normal exit point",
        )?;
        let mut expected = owned
            .drops()
            .iter()
            .filter(|f| {
                f.point() == point
                    && f.value_origin().start() > l.span.start()
                    && f.value_origin().end() < l.span.end()
            })
            .copied()
            .map(A::Drop)
            .collect::<Vec<_>>();
        if body {
            need(
                expected.len() == 1,
                "actual body Guard Drop prefix",
                &expected,
            )?;
            expected.extend([
                A::EndBinding {
                    statement: l.statement,
                    symbol: p.descriptor().value_binding(),
                },
                A::EndBinding {
                    statement: l.statement,
                    symbol: p.descriptor().key_binding(),
                },
            ]);
        } else {
            expected.extend([A::FinishProvider(l.statement), A::EndSource(l.statement)]);
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
                    .map(A::Drop),
            );
        }
        need(
            exit.actions() == expected
                && owned
                    .cleanup_steps()
                    .iter()
                    .filter_map(|(at, a)| (*at == point).then_some(*a))
                    .collect::<Vec<_>>()
                    == expected,
            "complete original Guard/component/provider/source cleanup order",
            exit,
        )?;
    }
    let _ = parsed;
    Ok(())
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
            UnitDropPoint::AfterExpression(UnitExpressionId::new(u, site.rhs))
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
