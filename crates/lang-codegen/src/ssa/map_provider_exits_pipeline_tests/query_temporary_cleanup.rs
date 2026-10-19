//! The original temporary Call owns one LoopExit obligation in each public FE product.
use super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::{
        CleanupCondition as C, CleanupOwnerValue as O, CleanupOwnerValueId,
        CompilationUnitOwnership, DropPoint, DropTarget, IterationCleanupAction as A,
        IterationExitKind as E, IterationPhiBoundary as B, IterationPhiIncomingKind as K,
        OwnershipCheckedFile, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};

pub(super) fn single(
    statement: StatementId,
    source: ExpressionId,
    body: StatementId,
    call: Span,
    owned: &OwnershipCheckedFile,
) -> Result<CleanupOwnerValueId, Failure> {
    let p = owned.map_iteration(statement).ok_or_else(|| {
        failure(
            "temporary form actual cleanup plan",
            (statement, source, body, call),
        )
    })?;
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Temporary(source)),
        "temporary form unique original Call Drop",
    )?;
    let owner = drop
        .owner()
        .ok_or_else(|| failure("temporary form actual cleanup owner", drop))?;
    let table = owned.cleanup_conditions();
    need(
        drop.point() == DropPoint::LoopExit(statement)
            && drop.value_origin() == call
            && drop.condition().is_none()
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none()
            && matches!(table.owner_value(owner), Some(O::Expression {expression, origin}) if *expression == source && *origin == call),
        "temporary form unique Call-origin owner and precise LoopExit",
        (drop, table.owner_value(owner)),
    )?;
    let d = p.descriptor();
    let expected = [
        (
            E::Fallthrough,
            DropPoint::AfterStatement(body),
            vec![
                A::EndBinding {
                    statement,
                    symbol: d.value_binding(),
                },
                A::EndBinding {
                    statement,
                    symbol: d.key_binding(),
                },
            ],
        ),
        (
            E::Exhaustion,
            DropPoint::LoopExit(statement),
            vec![
                A::FinishProvider(statement),
                A::EndSource(statement),
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
        (K::Entry, DropPoint::AfterExpression(source), B::Header),
        (K::Fallthrough, DropPoint::AfterStatement(body), B::Header),
        (K::Exhaustion, DropPoint::LoopExit(statement), B::Exit),
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
    body: UnitStatementId,
    call: Span,
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
        drop.point() == UnitDropPoint::LoopExit(statement) && drop.value_origin() == call,
        "temporary form qualified Call-origin LoopExit",
        drop,
    )?;
    let expected = [
        (
            UE::Fallthrough,
            UnitDropPoint::AfterStatement(body),
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
