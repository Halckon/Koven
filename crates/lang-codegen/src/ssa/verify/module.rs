//! Module validation and shared proofs for function verification.

use std::collections::VecDeque;

use super::{
    super::model::{Block, Edge, EntityData, EntityId, EntityType, Function, LoanKind, Module},
    *,
};
use crate::ssa::verify_ownership::edges;

pub(super) fn verify_module(module: &Module, errors: &mut Vec<VerifyError>) {
    let module_before = errors.len();
    crate::ssa::verify_types::verify_types(module, errors);
    verify_deinits(module, errors);
    let capture_types =
        (errors.len() == module_before).then(|| closure_escape::TypeCaptures::compute(module));

    let trusted_view_fields = closure_content::trusted_view_fields(module);

    for (function_index, function) in module.functions.iter().enumerate() {
        if function.id.module() != module.id || function.id.index() != function_index {
            errors.push(VerifyError {
                kind: VerifyErrorKind::InvalidFunctionId,
                location: VerifyLocation::Function(function.id),
                origin: Some(function.origin.clone()),
            });
            continue;
        }
        let before = errors.len();
        verify_function_structure(module, function, errors);
        if errors.len() == before {
            verify_cfg_types(module, function, errors);
            verify_dominance(function, errors);
            // Ownership content proofs require a validated module type/deinit graph.
            if errors.len() == before
                && let Some(types) = &capture_types
            {
                crate::ssa::verify_ownership::verify_ownership(
                    module,
                    function,
                    types,
                    trusted_view_fields.get(&function.id),
                    errors,
                );
                if errors.len() == before {
                    closure_escape::verify(function, types, errors);
                }
            }
        }
    }
}

fn verify_deinits(module: &Module, errors: &mut Vec<VerifyError>) {
    for (&owner, &function) in &module.deinits {
        let valid = crate::ssa::types::valid_deinit_signature(module, owner, function)
            && module.function(function).is_some_and(|function| {
                function.blocks.first().is_some_and(|entry| {
                    entry.parameters.len() == 1
                        && function
                            .entity(entry.parameters[0])
                            .is_some_and(|parameter| Some(parameter.ty) == function.receiver)
                })
            });
        if !valid {
            errors.push(VerifyError {
                kind: VerifyErrorKind::InvalidTypeDefinition {
                    reason: "deinit requires one exact shared heap-owner receiver and a Unit result",
                },
                location: VerifyLocation::Type(owner),
                origin: module.function(function).map(|function| function.origin.clone()),
            });
        }
    }
}

pub(super) fn verify_callable_view_entity(
    module: &Module,
    entity: EntityId,
    data: &EntityData,
    errors: &mut Vec<VerifyError>,
) {
    let misplaced = match data.ty {
        EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        } => false,
        other => module
            .callable_view_signature(other.semantic_type())
            .is_some(),
    };
    if misplaced {
        errors.push(entity_error(
            entity,
            data.origin.clone(),
            VerifyErrorKind::InvalidTypeDefinition {
                reason: "a callable view is only a shared loan target",
            },
        ));
    }
}

pub(super) fn verify_callable_view_returns(
    module: &Module,
    function: &Function,
    errors: &mut Vec<VerifyError>,
) {
    if function
        .return_types
        .iter()
        .any(|ty| module.callable_view_signature(*ty).is_some())
    {
        errors.push(function_error(
            function,
            VerifyErrorKind::InvalidTypeDefinition {
                reason: "a callable view cannot be returned",
            },
        ));
    }
}

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
