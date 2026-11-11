//! Metadata only: capture the live owner and close exactly one actual child-parent batch.
use super::super::{
    OwnedValue,
    action_source::{ActionHandle, ActionSource, BindingDropCause},
};
use super::{DropPlanner, ValueState};
use crate::{
    ownership_checking::{OwnershipBindingKind, UnitLoanFact, UnitLoanTarget},
    type_checking::{BuiltinType, UnitTypeKind},
};

pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) enum Batch {
    PendingChild(ActionHandle),
    Closed(BindingDropCause),
    Lost,
}

impl Batch {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn first(
        child: Option<ActionHandle>,
        parent: Option<ActionHandle>,
    ) -> Self {
        match (child, parent) {
            (Some(child), None) => Self::PendingChild(child),
            _ => Self::Lost,
        }
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn observe(
        &mut self,
        child: Option<ActionHandle>,
        parent: Option<ActionHandle>,
        actions: &ActionSource,
    ) {
        let prior = std::mem::replace(self, Self::Lost);
        if let (Self::PendingChild(child_retirement), None, Some(parent_retirement)) =
            (prior, child, parent)
            && let Some(cause) = actions.dependency_drop_cause(child_retirement, parent_retirement)
        {
            *self = Self::Closed(cause);
        }
        // A missing edge, reversed pair or third event cannot recover this batch.
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn cause(
        self,
    ) -> Option<BindingDropCause> {
        match self {
            Self::Closed(cause) => Some(cause),
            _ => None,
        }
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn binding_dependency_owner(
        &self,
        source: &UnitLoanFact,
        state: &ValueState,
    ) -> Option<OwnedValue> {
        let UnitLoanTarget::Place(place) = source.target() else {
            return None;
        };
        let [owner] = state.values.as_slice() else {
            return None;
        };
        let ty = self.checker.typed.symbol_type(owner.symbol)?;
        (owner.owner.is_some()
            && owner.symbol == place.root()
            && place.is_root()
            && owner.scope_depth == 1
            && owner.origin == owner.declaration
            && self.scope_depth == 1
            && self.owner_branch_depth == 0
            && self.owner_choice.is_none()
            && self.owner_loop.is_none()
            && self.loop_boundaries.is_empty()
            && self.call_return_copy_source.is_none()
            && self.checker.is_move_only_variable(owner.symbol)
            && self
                .checker
                .bindings
                .get(&owner.symbol)
                .is_none_or(|binding| binding.kind() == OwnershipBindingKind::Owned)
            && self.checker.typed.types().get(ty)
                == Some(&UnitTypeKind::Builtin(BuiltinType::String))
            && state.lambda.is_none()
            && state.this.is_none()
            && state.iterations.is_empty()
            && state.replacements.is_empty()
            && state.replace_consumer.is_none()
            && state.borrow_call_transports.is_empty()
            && state.borrow_binding_transports.is_empty()
            && state.borrow_forward_transports.is_empty()
            && state.pending_loans.is_empty()
            && state.pending_temporaries.is_empty()
            && state.pending_capture_borrows.is_empty()
            && state.closures.is_empty()
            && state.closure_asap.is_empty()
            && state
                .pending_borrows
                .iter()
                .all(|&(call, root)| call == source.call().expression() && root == owner.symbol))
        .then(|| owner.clone())
    }
}

#[cfg(test)]
mod tests;
