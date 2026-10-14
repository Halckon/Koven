//! A candidate ASAP fact is not a Return identity; the active lowering path proves that later.
use lang_frontend::{
    ownership_checking::{
        DropFact, DropPoint, DropTarget, IterationCleanupAction, IterationClosurePhiBinding,
        IterationExitKind, IterationExitPlan, IterationOwnershipPlan, LoanTarget,
        OwnershipCheckedFile,
    },
    parser::{Expression, ParsedFile},
    type_checking::{BuiltinType, IterationProvider, SequentialContainerKind, TypeKind, TypedFile},
};

use super::{presence_on_path, unique};

pub(super) fn drop(
    parsed: &ParsedFile,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    plan: &IterationOwnershipPlan,
    exit: &IterationExitPlan,
    header: &IterationClosurePhiBinding,
) -> Option<DropFact> {
    let IterationExitKind::Return(expression) = exit.kind() else {
        return None;
    };
    let Expression::Return {
        value: Some(value), ..
    } = parsed.ast().expressions().get(expression).ok()?.payload()
    else {
        return None;
    };
    if plan.descriptor().provider() != IterationProvider::Sequential(SequentialContainerKind::Array)
        || !matches!(plan.source(), LoanTarget::Temporary(_))
        || typed.types().get(plan.descriptor().element_type())
            != Some(&TypeKind::Builtin(BuiltinType::Int))
        || typed.types().get(typed.expression_type(*value)?)
            != Some(&TypeKind::Builtin(BuiltinType::Int))
        || plan.closure_phis().len() != 2
        || exit.actions().iter().any(|action| {
            matches!(action, IterationCleanupAction::Drop(drop) if drop.owner() == Some(header.owner()))
        })
    {
        return None;
    }
    let fact = *unique(owned.drops().iter().filter(|drop| {
        drop.owner() == Some(header.owner())
            && drop.target() == DropTarget::Named(header.symbol())
            && matches!(drop.point(), DropPoint::CallReturn(_))
            && drop.condition().is_some_and(|condition| {
                presence_on_path(
                    owned.cleanup_conditions(),
                    condition,
                    header.availability_selector(),
                    exit.condition(),
                )
            })
    }))?;
    let DropPoint::CallReturn(call) = fact.point() else {
        return None;
    };
    let receiver = typed
        .map_remove(call)
        .map(|descriptor| descriptor.receiver())
        .or_else(|| {
            typed
                .map_contains(call)
                .map(|descriptor| descriptor.receiver())
        })?;
    let loan = unique(
        owned
            .loans_ending_at(call)
            .filter(|loan| loan.call() == call && loan.argument() == receiver),
    )?;
    matches!(loan.target(), LoanTarget::Place(place) if place.is_root() && place.root() == header.symbol())
        .then_some(fact)
}
