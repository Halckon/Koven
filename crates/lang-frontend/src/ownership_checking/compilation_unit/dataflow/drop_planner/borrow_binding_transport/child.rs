//! One direct child dependency carried inside the existing live parent transport.
use super::super::action_source::{ActionHandle, binding_child_links};
use super::{DropPlanner, PendingBindingTransport, PlannerDropPoint, UseReturn, ValueState};
use crate::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{BorrowBindingStorage, UnitLoanFact},
    parser::Expression,
    type_checking::{BuiltinType, CallableResultSource, ExpressionCategory, UnitTypeKind},
};

#[derive(Clone, Debug)]
pub(super) enum ChildState {
    Never,
    Live {
        binding: UnitSymbolId,
        commit: ActionHandle,
        used: UseReturn,
    },
    Retired {
        binding: UnitSymbolId,
        retirement: ActionHandle,
    },
    Lost,
}

impl ChildState {
    pub(super) fn lose(&mut self) {
        *self = Self::Lost;
    }

    fn activate(&mut self, binding: UnitSymbolId, commit: Option<ActionHandle>) {
        *self = match (&*self, commit) {
            (Self::Never, Some(commit)) => Self::Live {
                binding,
                commit,
                used: UseReturn::Unused,
            },
            _ => Self::Lost,
        };
    }

    pub(super) fn reconcile(&mut self, other: &Self) {
        match (self, other) {
            (Self::Never, Self::Never) => {}
            (
                Self::Retired {
                    binding,
                    retirement,
                },
                Self::Retired {
                    binding: other_binding,
                    retirement: other_retirement,
                },
            ) if binding == other_binding && retirement.same_record(other_retirement) => {}
            (
                Self::Live {
                    binding,
                    commit,
                    used,
                },
                Self::Live {
                    binding: other_binding,
                    commit: other_commit,
                    used: other_used,
                },
            ) if binding == other_binding && commit.same_record(other_commit) => {
                let same = match (&*used, other_used) {
                    (UseReturn::Unused, UseReturn::Unused) => true,
                    (UseReturn::Unique(left), UseReturn::Unique(right)) => left.same_record(right),
                    _ => false,
                };
                if !same {
                    *used = UseReturn::Ambiguous;
                }
            }
            (state, _) => state.lose(),
        }
    }
}

impl PendingBindingTransport {
    fn child_parent_evidence(&self) -> binding_child_links::Parent {
        binding_child_links::Parent {
            binding: self.binding,
            source: self.source.clone(),
            source_begin: self.source_begin.clone(),
            handoff: self.handoff.clone(),
        }
    }
}

impl DropPlanner<'_, '_> {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn commit_borrow_binding_child(
        &mut self,
        binding: UnitSymbolId,
        initializer: ExpressionId,
        state: &mut ValueState,
    ) {
        let Some(fact) = self
            .checker
            .borrow_results
            .bindings
            .iter()
            .find(|fact| fact.binding() == binding)
        else {
            return;
        };
        let Some(parent) = fact.parent() else {
            return;
        };
        let direct_parent = state
            .borrow_binding_transports
            .iter()
            .position(|pending| pending.binding == parent);
        let Some(index) = direct_parent else {
            // A grandchild is outside this slice. Do not let its ancestor recover single-child evidence.
            for pending in &mut state.borrow_binding_transports {
                if matches!(&pending.child, ChildState::Live { binding, .. } if *binding == parent)
                {
                    pending.child.lose();
                }
            }
            return;
        };
        let raw_parent = self.checker.place(initializer).ok().flatten();
        let supported = self.scope_depth == 1
            && self.owner_branch_depth == 0
            && self.loop_boundaries.is_empty()
            && state.iterations.is_empty()
            && state.borrow_binding_transports.len() == 1
            && state.borrow_bindings.as_slice() == [parent, binding]
            && fact.storage() == BorrowBindingStorage::BorrowedStorage
            && fact.source_loan().is_none()
            && fact.initializer() == self.checker.unit_expression(initializer)
            && binding != parent
            && self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(initializer)
                .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            && raw_parent
                .as_ref()
                .is_some_and(|place| place.is_root() && place.root() == parent)
            && self
                .checker
                .typed
                .expression_type(fact.initializer())
                .is_some_and(|ty| {
                    self.checker.typed.types().get(ty)
                        == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                });
        let pending = &mut state.borrow_binding_transports[index];
        if !supported
            || !pending.observable
            || fact.origin() != pending.source.target()
            || !matches!(pending.child, ChildState::Never)
        {
            pending.child.lose();
            return;
        }
        let handle = self
            .iteration_actions
            .commit_binding_child(binding_child_links::Commit {
                fact: fact.clone(),
                initializer: self.checker.unit_expression(initializer),
                raw_parent: parent,
                parent: pending.child_parent_evidence(),
            });
        pending.child.activate(binding, handle);
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn finish_binding_child_use(
        &mut self,
        loan: UnitLoanFact,
        acquisition: Option<ActionHandle>,
        pending: &mut [PendingBindingTransport],
    ) -> Option<(UnitLoanFact, Option<ActionHandle>)> {
        let Some(place) = self
            .checker
            .place(loan.argument().expression())
            .ok()
            .flatten()
        else {
            return Some((loan, acquisition));
        };
        let single = pending.len() == 1;
        let Some(parent) = pending.iter_mut().find(|parent| matches!(&parent.child, ChildState::Live { binding, .. } if *binding == place.root())) else {
            return Some((loan, acquisition));
        };
        let supported = single
            && parent.observable
            && place.is_root()
            && self.scope_depth == 1
            && self.owner_branch_depth == 0
            && self.loop_boundaries.is_empty()
            && !self.liveness.expression_after[loan.call().expression().index()]
                .contains(&parent.binding)
            && !self.liveness.expression_after[loan.call().expression().index()]
                .contains(&place.root())
            && self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(loan.argument().expression())
                .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            && self
                .consumer_region_loan(
                    loan.call().expression(),
                    loan.argument().expression(),
                    ExpressionCategory::Place,
                )
                .is_some_and(|(checked, _)| checked == loan)
            && self.checker.typed.call(loan.call()).is_some_and(|call| {
                call.result_source() == CallableResultSource::Owned
                    && self.checker.typed.types().get(call.return_type())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            })
            && loan.target() == parent.source.target();
        let evidence = parent.child_parent_evidence();
        let ChildState::Live {
            binding,
            commit,
            used,
        } = &mut parent.child
        else {
            unreachable!("selected live child")
        };
        if !supported || !matches!(used, UseReturn::Unused) {
            *used = UseReturn::Ambiguous;
            return None;
        }
        let handle =
            self.iteration_actions
                .return_binding_child_use(binding_child_links::UseReturn {
                    binding: *binding,
                    parent: evidence,
                    commit: commit.clone(),
                    loan,
                    begin: acquisition,
                });
        *used = match (&*used, handle) {
            (UseReturn::Unused, Some(handle)) => UseReturn::Unique(handle),
            _ => UseReturn::Ambiguous,
        };
        None
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn retire_borrow_binding_child(
        &mut self,
        binding: UnitSymbolId,
        point: PlannerDropPoint,
        binding_end: ActionHandle,
        state: &mut ValueState,
    ) -> Option<ActionHandle> {
        let parent = state.borrow_binding_transports.iter_mut().find(|parent| matches!(&parent.child, ChildState::Live { binding: child, .. } if *child == binding))?;
        let evidence = parent.child_parent_evidence();
        let ChildState::Live { commit, used, .. } =
            std::mem::replace(&mut parent.child, ChildState::Lost)
        else {
            unreachable!("selected live child")
        };
        let use_return = match used {
            UseReturn::Unique(handle) => Some(handle),
            _ => None,
        };
        let retirement =
            self.iteration_actions
                .retire_binding_child(binding_child_links::Retirement {
                    binding,
                    parent: evidence,
                    commit,
                    use_return,
                    binding_end,
                    point: point.into_unit(self.checker.source_unit),
                })?;
        parent.child = ChildState::Retired {
            binding,
            retirement: retirement.clone(),
        };
        Some(retirement)
    }
}

#[cfg(test)]
mod tests;
