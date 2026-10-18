//! The original temporary Call owns one LoopExit obligation in each public FE product.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::ast::Site;
use lang_frontend::{
    ownership_checking::{
        CleanupCondition as C, CleanupOwnerValue as O, CleanupOwnerValueId,
        CompilationUnitOwnership, DropPoint, DropTarget, IterationCleanupAction as A,
        IterationExitKind as E, IterationPhiBoundary as B, IterationPhiIncomingKind as K,
        OwnershipCheckedFile, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE,
    },
    type_checking::{UnitExpressionId, UnitStatementId},
};

pub(super) fn single(
    site: &Site,
    owned: &OwnershipCheckedFile,
) -> Result<CleanupOwnerValueId, Failure> {
    let p = owned
        .map_iteration(site.statement)
        .ok_or_else(|| failure("temporary form actual cleanup plan", site))?;
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Temporary(site.source)),
        "temporary form unique original Call Drop",
    )?;
    let owner = drop
        .owner()
        .ok_or_else(|| failure("temporary form actual cleanup owner", drop))?;
    let table = owned.cleanup_conditions();
    need(
        drop.point() == DropPoint::LoopExit(site.statement)
            && drop.value_origin() == site.call
            && drop.condition().is_none()
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none()
            && matches!(table.owner_value(owner), Some(O::Expression {expression, origin}) if *expression == site.source && *origin == site.call),
        "temporary form unique Call-origin owner and precise LoopExit",
        (drop, table.owner_value(owner)),
    )?;
    let d = p.descriptor();
    let expected = [
        (
            E::Fallthrough,
            DropPoint::AfterStatement(site.body),
            vec![
                A::EndBinding {
                    statement: site.statement,
                    symbol: d.value_binding(),
                },
                A::EndBinding {
                    statement: site.statement,
                    symbol: d.key_binding(),
                },
            ],
        ),
        (
            E::Exhaustion,
            DropPoint::LoopExit(site.statement),
            vec![
                A::FinishProvider(site.statement),
                A::EndSource(site.statement),
                A::Drop(*drop),
            ],
        ),
    ];
    need(
        p.exits().len() == 2 && p.closure_phis().is_empty() && p.closure_phi_incomings().len() == 3,
        "temporary form complete normal exits and hidden-owner incoming facts",
        p,
    )?;
    for (kind, point, actions) in expected {
        let exit = one(
            p.exits().iter().filter(|e| e.kind() == kind),
            "temporary form original normal exit",
        )?;
        let complete = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, action)| (*at == point).then_some(*action))
            .collect::<Vec<_>>();
        need(
            exit.point() == point
                && exit.actions() == actions
                && complete == actions
                && table.get(exit.condition()) == Some(&C::Always),
            "temporary form complete same-point ordered cleanup and Always",
            (exit, complete),
        )?;
    }
    for (kind, point, boundary) in [
        (K::Entry, DropPoint::AfterExpression(site.source), B::Header),
        (
            K::Fallthrough,
            DropPoint::AfterStatement(site.body),
            B::Header,
        ),
        (K::Exhaustion, DropPoint::LoopExit(site.statement), B::Exit),
    ] {
        let incoming = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == kind),
            "temporary form complete actual incoming kind",
        )?;
        need(
            incoming.point() == point
                && incoming.boundary() == boundary
                && incoming.bindings().is_empty()
                && table.get(incoming.condition()) == Some(&C::Always),
            "temporary form original incoming point/boundary/Always/empty bindings",
            incoming,
        )?;
    }
    Ok(owner)
}

pub(super) fn unit(
    site: &Site,
    statement: UnitStatementId,
    source: UnitExpressionId,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("temporary form qualified cleanup plan", statement))?;
    let d = p.descriptor();
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Temporary(source)),
        "temporary form qualified unique original Call Drop",
    )?;
    need(
        drop.point() == UnitDropPoint::LoopExit(statement) && drop.value_origin() == site.call,
        "temporary form qualified Call-origin LoopExit",
        drop,
    )?;
    let expected = [
        (
            UE::Fallthrough,
            UnitDropPoint::AfterStatement(UnitStatementId::new(statement.source_unit(), site.body)),
            vec![
                UA::EndBinding {
                    statement,
                    symbol: d.value_binding(),
                },
                UA::EndBinding {
                    statement,
                    symbol: d.key_binding(),
                },
            ],
        ),
        (
            UE::Exhaustion,
            UnitDropPoint::LoopExit(statement),
            vec![
                UA::FinishProvider(statement),
                UA::EndSource(statement),
                UA::Drop(*drop),
            ],
        ),
    ];
    need(
        p.exits().len() == 2,
        "temporary form qualified complete normal exits",
        p,
    )?;
    for (kind, point, actions) in expected {
        let exit = one(
            p.exits().iter().filter(|e| e.kind() == kind),
            "temporary form actual qualified exit",
        )?;
        need(
            exit.point() == point
                && exit.actions() == actions
                && owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
            "temporary form qualified independent complete cleanup table",
            (exit, actions),
        )?;
    }
    Ok(())
}
