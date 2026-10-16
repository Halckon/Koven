//! Forget unavailable history, while retaining every observable provenance ancestor.
use super::{BlockId, EntityId, Function, State};
use crate::ssa::{model::Definition, verify::compute_dominators};
use std::collections::BTreeSet;

pub(super) struct EntryProjection {
    dominators: Vec<BTreeSet<usize>>,
}

impl EntryProjection {
    pub(super) fn new(function: &Function) -> Self {
        Self {
            dominators: compute_dominators(function),
        }
    }

    pub(super) fn retain_entry(&self, function: &Function, block: BlockId, state: &mut State) {
        // Edge parameters were copied simultaneously, and nullable proof births were
        // installed, before this entry. Keep inactive references as well as live ones.
        let mut loans = state
            .loans
            .keys()
            .copied()
            .filter(|loan| self.available(function, block, EntityId::Loan(*loan)))
            .collect::<BTreeSet<_>>();
        state
            .places
            .retain(|place, _| self.available(function, block, EntityId::Place(*place)));
        let mut pending = loans.iter().copied().collect::<Vec<_>>();
        for reference in state.places.values() {
            pending.extend(reference.lineage.iter().copied());
        }
        let mut expanded = BTreeSet::new();
        while let Some(loan) = pending.pop() {
            if !expanded.insert(loan) {
                continue;
            }
            if let Some(reference) = state.loans.get(&loan) {
                // These records need not dominate the entry: an alias/place can carry
                // their original lineage. State::end needs their unchanged token identity.
                loans.insert(loan);
                pending.extend(reference.lineage.iter().copied());
            }
        }
        state.loans.retain(|loan, _| loans.contains(loan));
    }

    fn available(&self, function: &Function, block: BlockId, entity: EntityId) -> bool {
        // The caller has already proved structure and SSA dominance. An unavailable
        // direct definition must execute again before any later legal direct use.
        let definition = function.entity(entity).expect("verified entity").definition;
        let defining_block = match definition {
            Definition::BlockParameter { block, .. } => block,
            Definition::InstructionResult { instruction, .. } => {
                let defining_block = function
                    .instruction(instruction)
                    .expect("verified instruction")
                    .block;
                if defining_block == block {
                    return false;
                }
                defining_block
            }
        };
        self.dominators[block.index()].contains(&defining_block.index())
    }
}

#[cfg(test)]
#[path = "provider_entry_projection_tests.rs"]
mod tests;
