//! NullableTake requires an exact owner/proof pair on every incoming CFG edge.
use std::collections::{BTreeMap, BTreeSet};

use crate::ssa::{
    model::{Definition, EntityId, Function, LoanId, TerminatorKind, ValueId},
    verify_ownership::edges,
};

/// Follow simultaneous owner and proof rebinding back to real NullableBranch births.
/// Every path must preserve the pair; every cycle must have a reachable birth witness.
pub(super) fn has_exact_branch_proof(function: &Function, owner: ValueId, proof: LoanId) -> bool {
    let mut pending = vec![(owner, proof)];
    let mut dependencies = BTreeMap::new();
    let mut grounded = BTreeSet::new();
    while let Some(pair @ (owner, proof)) = pending.pop() {
        if dependencies.contains_key(&pair) {
            continue;
        }
        let Some(Definition::BlockParameter { block, index }) = function
            .entity(EntityId::Loan(proof))
            .map(|entity| entity.definition)
        else {
            return false;
        };
        let Some(owner_definition) = function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.definition)
        else {
            return false;
        };
        let mut parents = Vec::new();
        let mut has_incoming = false;
        for source in &function.blocks {
            let Some(terminator) = &source.terminator else {
                return false;
            };
            for edge in edges(&terminator.kind)
                .into_iter()
                .filter(|edge| edge.target == block)
            {
                has_incoming = true;
                let incoming_owner = match owner_definition {
                    Definition::BlockParameter {
                        block: owner_block,
                        index,
                    } if owner_block == block => {
                        let Some(EntityId::Value(value)) = edge.arguments.get(index) else {
                            return false;
                        };
                        *value
                    }
                    _ => owner,
                };
                if let Some(EntityId::Loan(incoming_proof)) = edge.arguments.get(index) {
                    parents.push((incoming_owner, *incoming_proof));
                } else if let TerminatorKind::NullableBranch {
                    owner: source_owner,
                    when_non_null,
                    view,
                    ..
                } = &terminator.kind
                {
                    if !std::ptr::eq(edge, when_non_null)
                        || *view != proof
                        || index != edge.arguments.len()
                        || incoming_owner != *source_owner
                    {
                        return false;
                    }
                    grounded.insert(pair);
                } else {
                    return false;
                }
            }
        }
        if !has_incoming {
            // A function parameter or disconnected cycle is not a proof birth.
            return false;
        }
        pending.extend(parents.iter().copied());
        dependencies.insert(pair, parents);
    }
    // Coinductive loops are valid only when each obligation can reach a real birth.
    loop {
        let before = grounded.len();
        for (pair, parents) in &dependencies {
            if parents.iter().any(|parent| grounded.contains(parent)) {
                grounded.insert(*pair);
            }
        }
        if grounded.len() == before {
            return grounded.len() == dependencies.len();
        }
    }
}
