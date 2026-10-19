//! Same-analysis FE facts retain the exact owner and event identities.
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{
    Case,
    ast::{sites, span},
};
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, UnitReferenceTarget, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupCondition as C, CleanupOwnerValue as O, CompilationUnitOwnership, DropPoint,
        DropTarget, IterationPhiBoundary as B, IterationPhiIncomingKind as K, OwnershipCheckedFile,
        UnitDropPoint, UnitDropTarget,
    },
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::{
        CompilationUnitTypes, ExpressionCategory, TypedFile, UnitExpressionId, UnitStatementId,
    },
};

pub(super) fn single(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    for s in sites(sources, parsed)? {
        let root = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == span(parsed, s.source))
                .filter_map(|r| match r.target() {
                    ReferenceTarget::Symbol(r) => Some(*r),
                    _ => None,
                }),
            "completed epoch original source root",
        )?;
        let mut id = s.rhs;
        let ty = typed.expression_type(s.call);
        let mut depth = 0;
        loop {
            need(
                typed.expression_type(id) == ty
                    && typed.expression_category(id) == Some(ExpressionCategory::Temporary),
                "every completed RHS Group preserves actual type/category",
                id,
            )?;
            if let Expression::Group { expression } =
                parsed.ast().expressions().get(id).unwrap().payload()
            {
                depth += 1;
                id = *expression;
            } else {
                break;
            }
        }
        need(
            depth == if case.grouped { 3 } else { 0 } && id == s.call,
            "exact complete RHS canonical Call",
            id,
        )?;
        let p = owned
            .map_iteration(s.statement)
            .ok_or_else(|| failure("actual old epoch plan", s.statement))?;
        let table = owned.cleanup_conditions();
        let h = one(
            p.closure_phis()
                .iter()
                .filter(|p| p.symbol() == root && p.boundary() == B::Header),
            "old current Header",
        )?;
        let x = one(
            p.closure_phis()
                .iter()
                .filter(|p| p.symbol() == root && p.boundary() == B::Exit),
            "old current Exit",
        )?;
        let drops = owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(root))
            .collect::<Vec<_>>();
        let new = one(
            drops
                .iter()
                .copied()
                .filter(|d| d.point() == DropPoint::AfterExpression(s.assignment)),
            "new installation's actual ASAP Drop",
        )?;
        need(
            new.value_origin() == span(parsed, s.assignment)
                && new.condition().is_none()
                && matches!(
                    new.owner().and_then(|v| table.owner_value(v)),
                    Some(O::Expression { expression, origin })
                        if *expression == s.call && *origin == span(parsed, s.call)
                ),
            "new Drop uses actual other Call epoch, never old declaration",
            new,
        )?;
        let entry = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == K::Entry),
            "old actual Entry",
        )?;
        let b = one(entry.bindings().iter(), "old Entry complete binding")?;
        let v = one(b.values().iter(), "old Entry actual owner")?;
        need(
            matches!(
                table.owner_value(v.source()),
                Some(O::Expression { expression, origin })
                    if *expression == s.old && *origin == span(parsed, s.old)
            ) && b.target() == h.owner(),
            "old provider originates in actual make Call",
            entry,
        )?;
        let fall = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == K::Fallthrough),
            "old actual backedge",
        )?;
        let b = one(fall.bindings().iter(), "old backedge binding")?;
        let v = one(b.values().iter(), "old backedge owner")?;
        need(
            b.target() == h.owner()
                && v.source() == h.owner()
                && v.condition() == h.availability_condition(),
            "old complete live backedge",
            fall,
        )?;
        let exhaustion = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == K::Exhaustion),
            "old actual exhaustion",
        )?;
        let absent = one(exhaustion.bindings().iter(), "old complete Exit binding")?;
        need(
            absent.target() == x.owner()
                && absent.values().is_empty()
                && table.get(absent.available_when()) == Some(&C::Never),
            "Pure old epoch has already been consumed before assignment",
            exhaustion,
        )?;
        let actual = one(
            drops
                .iter()
                .copied()
                .filter(|d| d.point() == DropPoint::LoopExit(s.statement)),
            "old real ASAP provider Drop",
        )?;
        need(
            actual.owner() == Some(h.owner())
                && actual.condition() == Some(h.availability_condition())
                && actual.value_origin() == s.binding,
            "old Drop consumes actual current Header",
            actual,
        )?;
        for drop in drops {
            if drop == new || drop == actual {
                continue;
            }
            need(
                drop.point() == DropPoint::AfterExpression(s.rhs)
                    && drop.value_origin() == s.binding
                    && drop.owner() == Some(x.owner())
                    && drop.condition() == Some(x.availability_condition()),
                "any residual old fact belongs only to absent Exit at complete outer RHS",
                drop,
            )?;
        }
        println!("COMPLETED EPOCH SINGLE actual-old={actual:#?} new={new:#?} absent={absent:#?}");
    }
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
    for s in sites(sources, parsed)? {
        let root = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == span(parsed, s.source))
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(r) => Some(*r),
                    _ => None,
                }),
            "qualified completed source root",
        )?;
        let u = root.source_unit();
        let mut id = s.rhs;
        let ty = typed.expression_type(UnitExpressionId::new(u, s.call));
        let mut depth = 0;
        loop {
            let q = UnitExpressionId::new(u, id);
            need(
                typed.expression_type(q) == ty
                    && typed.expression_category(q) == Some(ExpressionCategory::Temporary),
                "qualified completed Group type/category",
                q,
            )?;
            if let Expression::Group { expression } =
                parsed.ast().expressions().get(id).unwrap().payload()
            {
                depth += 1;
                id = *expression;
            } else {
                break;
            }
        }
        need(
            depth == if case.grouped { 3 } else { 0 } && id == s.call,
            "qualified canonical actual Call",
            id,
        )?;
        let drops = owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(root))
            .collect::<Vec<_>>();
        let new = one(
            drops.iter().copied().filter(|d| {
                d.point() == UnitDropPoint::AfterExpression(UnitExpressionId::new(u, s.assignment))
            }),
            "qualified new installation ASAP Drop",
        )?;
        need(
            new.value_origin() == span(parsed, s.assignment),
            "qualified new value origin is complete assignment event",
            new,
        )?;
        let old = one(
            drops.iter().copied().filter(|d| {
                d.point() == UnitDropPoint::LoopExit(UnitStatementId::new(u, s.statement))
            }),
            "qualified old actual provider Drop",
        )?;
        need(
            old.value_origin() == s.binding,
            "qualified old original binding origin",
            old,
        )?;
        for d in drops {
            need(
                d == new
                    || d == old
                    || (d.point()
                        == UnitDropPoint::AfterExpression(UnitExpressionId::new(u, s.rhs))
                        && d.value_origin() == s.binding),
                "qualified old/new exact distinct cleanup events",
                d,
            )?;
        }
        println!("COMPLETED EPOCH UNIT old={old:#?} new={new:#?}");
    }
    Ok(())
}
