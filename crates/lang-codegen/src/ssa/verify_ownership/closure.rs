//! Closure-specific linear ownership effects.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    AliasRoots, BlockState, consume_value, edges, error, has_exclusive_value_loan, require_value,
};
use crate::ssa::{
    model::{
        ClosureCaptureOperand, EntityId, Function, LoanId, Module, Operation, Origin, ValueId,
    },
    verify::{VerifyError, VerifyErrorKind, VerifyLocation},
};

pub(super) struct ClosureLoans {
    by_owner: BTreeMap<ValueId, BTreeSet<LoanId>>,
}

impl ClosureLoans {
    pub(super) fn compute(function: &Function) -> Self {
        let mut by_owner = BTreeMap::<ValueId, BTreeSet<LoanId>>::new();
        for instruction in &function.instructions {
            let Operation::ClosureConstruct { captures, .. } = &instruction.operation else {
                continue;
            };
            let EntityId::Value(owner) = instruction.results[0] else {
                continue;
            };
            by_owner
                .entry(owner)
                .or_default()
                .extend(captures.iter().filter_map(|capture| match capture {
                    ClosureCaptureOperand::Shared(loan) => Some(*loan),
                    ClosureCaptureOperand::Owned(_) => None,
                }));
        }
        loop {
            let mut changed = false;
            for block in &function.blocks {
                let terminator = block.terminator.as_ref().expect("terminator must exist");
                for edge in edges(&terminator.kind) {
                    let target = function.block(edge.target).expect("target must exist");
                    let loan_rebindings = edge
                        .arguments
                        .iter()
                        .zip(&target.parameters)
                        .filter_map(|(argument, parameter)| match (argument, parameter) {
                            (EntityId::Loan(argument), EntityId::Loan(parameter)) => {
                                Some((*argument, *parameter))
                            }
                            _ => None,
                        })
                        .collect::<BTreeMap<_, _>>();
                    for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                        let (EntityId::Value(argument), EntityId::Value(parameter)) =
                            (argument, parameter)
                        else {
                            continue;
                        };
                        let dependencies = by_owner
                            .get(argument)
                            .into_iter()
                            .flatten()
                            .map(|loan| loan_rebindings.get(loan).copied().unwrap_or(*loan))
                            .collect::<BTreeSet<_>>();
                        let entry = by_owner.entry(*parameter).or_default();
                        let before = entry.len();
                        entry.extend(dependencies);
                        changed |= entry.len() != before;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        Self { by_owner }
    }

    pub(super) fn activate_entry(&self, owner: ValueId, state: &mut BlockState) {
        state.loans.extend(self.dependencies(owner).iter().copied());
    }

    pub(super) fn release(&self, owner: ValueId, state: &mut BlockState) {
        for loan in self.dependencies(owner) {
            state.loans.remove(loan);
        }
    }

    pub(super) fn live_owner_holding(&self, loan: LoanId, state: &BlockState) -> Option<ValueId> {
        state
            .values
            .iter()
            .copied()
            .find(|owner| self.dependencies(*owner).contains(&loan))
    }

    pub(super) fn dependencies(&self, owner: ValueId) -> &BTreeSet<LoanId> {
        self.by_owner.get(&owner).unwrap_or(&EMPTY_LOANS)
    }
}

static EMPTY_LOANS: BTreeSet<LoanId> = BTreeSet::new();

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_construct(
    module: &Module,
    function: &Function,
    captures: &[ClosureCaptureOperand],
    aliases: &AliasRoots,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    for capture in captures {
        match capture {
            ClosureCaptureOperand::Owned(value) => {
                consume_value(
                    module,
                    function,
                    *value,
                    aliases,
                    state,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    location.clone(),
                    origin,
                    errors,
                );
            }
            ClosureCaptureOperand::Shared(loan) => {
                if !state.loans.contains(loan) {
                    errors.push(error(
                        VerifyErrorKind::LoanInactive { loan: *loan },
                        location.clone(),
                        origin,
                    ));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_invoke(
    module: &Module,
    function: &Function,
    callable: ValueId,
    arguments: &[EntityId],
    aliases: &AliasRoots,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    if require_value(
        module,
        function,
        callable,
        state,
        location.clone(),
        origin,
        errors,
    ) && has_exclusive_value_loan(function, callable, aliases, state)
    {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value: callable },
            location.clone(),
            origin,
        ));
    }
    for argument in arguments {
        match argument {
            EntityId::Value(value) => {
                consume_value(
                    module,
                    function,
                    *value,
                    aliases,
                    state,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    location.clone(),
                    origin,
                    errors,
                );
            }
            EntityId::Loan(loan) if !state.loans.contains(loan) => errors.push(error(
                VerifyErrorKind::LoanInactive { loan: *loan },
                location.clone(),
                origin,
            )),
            EntityId::Loan(_) => {}
            EntityId::Place(place) => {
                super::require_place(*place, state, location.clone(), origin, errors);
            }
        }
    }
}
