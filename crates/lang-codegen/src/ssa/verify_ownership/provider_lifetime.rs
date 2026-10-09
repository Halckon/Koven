//! Bounded path-correlated loan provenance for provider places; no may-origin union.
use super::super::{
    model::{BlockId, EntityId, Function, LoanId, Operation, PlaceId, TerminatorKind},
    verify::{VerifyError, VerifyErrorKind, VerifyLocation},
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[cfg(test)]
#[path = "provider_lifetime_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Reference {
    lineage: BTreeSet<LoanId>,
    live: bool,
}

#[derive(Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    loans: BTreeMap<LoanId, Reference>,
    places: BTreeMap<PlaceId, Reference>,
}

impl State {
    fn end(&mut self, loan: LoanId) {
        let Some(reference) = self.loans.get(&loan).cloned() else {
            return;
        };
        // Each alias keeps the defining loan token; derivations append their own token.
        let token = reference
            .lineage
            .iter()
            .find(|token| {
                self.loans
                    .get(token)
                    .is_some_and(|root| root.lineage == reference.lineage)
            })
            .copied()
            .unwrap_or(loan);
        for reference in self.loans.values_mut().chain(self.places.values_mut()) {
            if reference.lineage.contains(&token) {
                reference.live = false;
            }
        }
    }
}

pub(super) fn verify(function: &Function, errors: &mut Vec<VerifyError>) {
    if !function.instructions.iter().any(|instruction| {
        matches!(
            instruction.operation,
            Operation::ContainerElementPlace { .. } | Operation::RangeElementPlace { .. }
        )
    }) {
        return;
    }
    let Some(entry) = function.entry_block() else {
        return;
    };
    let mut initial = State::default();
    for entity in &function.block(entry).expect("verified entry").parameters {
        if let EntityId::Loan(loan) = entity {
            initial.loans.insert(
                *loan,
                Reference {
                    lineage: BTreeSet::from([*loan]),
                    live: true,
                },
            );
        }
    }
    let mut queue = VecDeque::from([(entry, initial)]);
    let mut visited = BTreeSet::<(BlockId, State)>::new();
    // Independent finite budget. Exhaustion is an explicit rejection, never acceptance.
    let budget = function.blocks.len().saturating_mul(256).max(256);
    while let Some((block_id, mut state)) = queue.pop_front() {
        if !visited.insert((block_id, state.clone())) {
            continue;
        }
        if visited.len() > budget {
            errors.push(VerifyError {
                kind: VerifyErrorKind::OperationContract {
                    reason: "provider provenance fixed-point budget exceeded",
                },
                location: VerifyLocation::Block(block_id),
                origin: Some(function.origin.clone()),
            });
            return;
        }
        let block = function.block(block_id).expect("verified block");
        for id in &block.instructions {
            let instruction = function.instruction(*id).expect("verified instruction");
            let result = instruction.results.first().copied();
            let inherited = match instruction.operation {
                Operation::ContainerElementPlace {
                    owner: EntityId::Loan(loan),
                    ..
                }
                | Operation::RangeElementPlace { view: loan, .. }
                | Operation::SharedReborrow { source: loan }
                | Operation::SharedReferenceFollow { source: loan }
                | Operation::SharedFieldLoan { base: loan, .. }
                | Operation::SharedHeapFieldLoan { base: loan, .. } => {
                    let reference = state.loans.get(&loan).cloned();
                    if reference.as_ref().is_some_and(|reference| !reference.live) {
                        errors.push(VerifyError {
                            kind: VerifyErrorKind::LoanInactive { loan },
                            location: VerifyLocation::Instruction(*id),
                            origin: Some(instruction.origin.clone()),
                        });
                        return;
                    }
                    reference
                }
                Operation::BorrowBegin { place, .. } => {
                    let reference = state.places.get(&place).cloned();
                    if reference.as_ref().is_some_and(|reference| !reference.live) {
                        errors.push(VerifyError {
                            kind: VerifyErrorKind::PlaceUnavailable { place },
                            location: VerifyLocation::Instruction(*id),
                            origin: Some(instruction.origin.clone()),
                        });
                        return;
                    }
                    reference
                }
                Operation::BorrowEnd { loan } => {
                    state.end(loan);
                    None
                }
                _ => None,
            };
            match result {
                Some(EntityId::Loan(loan)) => {
                    let mut reference = inherited.unwrap_or(Reference {
                        lineage: BTreeSet::new(),
                        live: true,
                    });
                    reference.lineage.insert(loan);
                    state.loans.insert(loan, reference);
                }
                Some(EntityId::Place(place)) => {
                    if let Some(reference) = inherited {
                        state.places.insert(place, reference);
                    } else {
                        state.places.remove(&place);
                    }
                }
                _ => {}
            }
        }
        let terminator = &block.terminator.as_ref().expect("verified terminator").kind;
        for edge in super::edges(terminator) {
            let mut successor = state.clone();
            let target = function.block(edge.target).expect("verified edge");
            for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                match (argument, parameter) {
                    (EntityId::Loan(from), EntityId::Loan(to)) => {
                        if let Some(reference) = state.loans.get(from) {
                            successor.loans.insert(*to, reference.clone());
                        }
                    }
                    (EntityId::Place(from), EntityId::Place(to)) => {
                        if let Some(reference) = state.places.get(from) {
                            successor.places.insert(*to, reference.clone());
                        }
                    }
                    _ => {}
                }
            }
            // Nullable proof births belong to their actual non-null edge.
            if let TerminatorKind::NullableBranch {
                view,
                when_non_null,
                ..
            } = terminator
                && std::ptr::eq(edge, when_non_null)
            {
                successor.loans.insert(
                    *view,
                    Reference {
                        lineage: BTreeSet::from([*view]),
                        live: true,
                    },
                );
            }
            queue.push_back((edge.target, successor));
        }
    }
}
