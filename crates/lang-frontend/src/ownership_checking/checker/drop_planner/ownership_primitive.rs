//! 原子 root commit 只运输 owner 身份，不调用 assignment 的旧值析构。
use super::{CleanupConditionId, DropPlanner, OwnerVersion, OwnershipCheckingError, ValueState};
use crate::{
    ast::ExpressionId,
    type_checking::{Copyability, OwnershipPrimitiveKind},
};

impl DropPlanner<'_, '_> {
    pub(super) fn commit_ownership_primitive(
        &mut self,
        call: ExpressionId,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(plan) = self.checker.ownership_primitives.get(&call.index()) else {
            return Ok(());
        };
        if self
            .checker
            .typed
            .copyability(plan.descriptor().value_type())
            != Some(Copyability::MoveOnly)
        {
            return Ok(());
        }
        let invalid = || OwnershipCheckingError::InvalidOwnershipPrimitive {
            expression: call.index(),
        };
        let origin = self.checker.parsed.ast().expressions().get(call)?.span();
        let first = plan.places()[0].root();
        match plan.descriptor().kind() {
            OwnershipPrimitiveKind::Replace => {
                let index = state.position(first).ok_or_else(invalid)?;
                let mut new = state
                    .nullable_temporaries
                    .iter()
                    .rev()
                    .find(|temporary| {
                        temporary.control == call
                            && temporary.subject == plan.descriptor().operands()[1]
                            && temporary.transfers_at_call
                    })
                    .map(|temporary| temporary.versions.clone())
                    .filter(|versions| !versions.is_empty())
                    .ok_or_else(invalid)?;
                self.fold_completed_primitive_value(
                    plan.descriptor().operands()[1],
                    origin,
                    state.path,
                    &mut new,
                );
                let mut old = std::mem::replace(&mut state.values[index].versions, new);
                self.fold_completed_primitive_value(call, origin, state.path, &mut old);
                state.result_owners = old;
                let value = &mut state.values[index];
                value.origin = origin;
                value.condition = state.path;
                for version in &mut value.versions {
                    version.origin = origin;
                }
            }
            OwnershipPrimitiveKind::Swap => {
                let second = plan.places()[1].root();
                if state.position(first).is_none() || state.position(second).is_none() {
                    return Err(invalid());
                }
                if let (Some(mut left), Some(mut right)) =
                    (state.remove_value(first), state.remove_value(second))
                {
                    std::mem::swap(&mut left.versions, &mut right.versions);
                    self.fold_completed_primitive_value(
                        plan.descriptor().operands()[1],
                        origin,
                        state.path,
                        &mut left.versions,
                    );
                    self.fold_completed_primitive_value(
                        plan.descriptor().operands()[0],
                        origin,
                        state.path,
                        &mut right.versions,
                    );
                    left.origin = origin;
                    right.origin = origin;
                    for version in left.versions.iter_mut().chain(&mut right.versions) {
                        version.origin = origin;
                    }
                    state.insert(left);
                    state.insert(right);
                }
            }
        }
        Ok(())
    }

    /// The checked exclusive commit receives one complete nonclosure value on each
    /// reachable edge. Earlier alternative identities have no outstanding loans or
    /// closure provenance to transport; keep one owner for the completed SSA value.
    fn fold_completed_primitive_value(
        &mut self,
        expression: ExpressionId,
        origin: crate::source::Span,
        path: CleanupConditionId,
        versions: &mut Vec<OwnerVersion>,
    ) {
        if versions.len() > 1 {
            *versions = vec![self.expression_owner(expression, origin, path)];
        }
    }
}
