//! The current physical value of a direct root slot can differ from its SSA owner.

use super::*;
use crate::ssa::model::InstructionId;

pub(super) fn take_sources(function: &Function) -> BTreeMap<InstructionId, ValueId> {
    let mut sources = BTreeMap::new();
    for block in &function.blocks {
        let mut writes = BTreeMap::new();
        for id in &block.instructions {
            let instruction = function.instruction(*id).expect("verified instruction");
            match instruction.operation {
                Operation::Mutate { place, value } => {
                    writes.insert(place, value);
                }
                // The operation contract requires the direct RootPlace result, and
                // linear live-ins forbid using that result across a block boundary.
                Operation::RootPlaceTake { owner, place } => {
                    sources.insert(*id, writes.get(&place).copied().unwrap_or(owner));
                }
                _ => {}
            }
        }
    }
    sources
}
