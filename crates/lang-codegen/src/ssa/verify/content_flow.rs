//! Deterministic CFG worklist shared by program-point content proofs.

use crate::ssa::{
    model::{Block, Edge, Function},
    verify_ownership::edges,
};
use std::collections::VecDeque;

/// The caller supplies domain transfer, simultaneous edge transport, and its monotone join.
/// Structural verification must have succeeded before walking the CFG.
pub(in crate::ssa::verify) fn block_entries<State: Clone>(
    function: &Function,
    defaults: &State,
    transfer: impl Fn(&Block, &mut State),
    transport: impl Fn(&Edge, &State) -> State,
    join: impl Fn(&mut State, State) -> bool,
) -> Vec<State> {
    let mut reachable = vec![false; function.blocks.len()];
    let mut pending = VecDeque::from([0]);
    reachable[0] = true;
    while let Some(index) = pending.pop_front() {
        for edge in edges(
            &function.blocks[index]
                .terminator
                .as_ref()
                .expect("verified terminator")
                .kind,
        ) {
            if !reachable[edge.target.index()] {
                reachable[edge.target.index()] = true;
                pending.push_back(edge.target.index());
            }
        }
    }
    let mut entries = vec![None; function.blocks.len()];
    let mut queued = vec![false; function.blocks.len()];
    for index in 0..function.blocks.len() {
        // Unreachable components have no caller proof, including closed CFG cycles.
        if index == 0 || !reachable[index] {
            entries[index] = Some(defaults.clone());
            queued[index] = true;
            pending.push_back(index);
        }
    }
    while let Some(index) = pending.pop_front() {
        queued[index] = false;
        let mut state = entries[index]
            .as_ref()
            .expect("queued entry exists")
            .clone();
        let block = &function.blocks[index];
        transfer(block, &mut state);
        for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind) {
            let incoming = transport(edge, &state);
            let changed = match &mut entries[edge.target.index()] {
                None => {
                    entries[edge.target.index()] = Some(incoming);
                    true
                }
                Some(previous) => join(previous, incoming),
            };
            if changed && !queued[edge.target.index()] {
                queued[edge.target.index()] = true;
                pending.push_back(edge.target.index());
            }
        }
    }
    entries
        .into_iter()
        .map(|entry| entry.expect("all components were seeded"))
        .collect()
}
