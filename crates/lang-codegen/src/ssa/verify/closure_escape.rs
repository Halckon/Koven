//! Reject unproved borrowed environments only at owned escape delivery boundaries.

pub(in crate::ssa::verify) mod proof;
mod types;

pub(super) use types::TypeCaptures;

use super::{VerifyError, VerifyErrorKind, VerifyLocation};
use crate::ssa::model::{EntityId, Function, Operation, TerminatorKind};

const REASON: &str = "borrowed closure cannot escape through owned value delivery";

pub(super) fn verify(function: &Function, types: &TypeCaptures, errors: &mut Vec<VerifyError>) {
    if !function
        .values
        .iter()
        .any(|entity| types.contains(entity.ty.semantic_type()))
    {
        return;
    }
    let proof = proof::ValueProof::new(function, types);
    let entries = proof.block_entries();
    for block in &function.blocks {
        let mut state = entries[block.id.index()].clone();
        for id in &block.instructions {
            let instruction = function.instruction(*id).expect("verified instruction");
            let may = |value: crate::ssa::model::ValueId| state[value.index()];
            let escapes = match &instruction.operation {
                Operation::DirectCall {
                    receiver,
                    arguments,
                    ..
                } => receiver
                    .iter()
                    .chain(arguments)
                    .any(|argument| matches!(argument, EntityId::Value(value) if may(*value))),
                Operation::CallableInvoke { arguments, .. } => arguments
                    .iter()
                    .any(|argument| matches!(argument, EntityId::Value(value) if may(*value))),
                Operation::AggregateConstruct { fields, .. } => fields.iter().copied().any(may),
                Operation::ContainerConstruct { elements, .. } => elements.iter().copied().any(may),
                Operation::TaggedConstruct { payload, .. }
                | Operation::HeapAllocate { payload, .. }
                | Operation::SharedAllocate { payload, .. } => may(*payload),
                Operation::HeapFieldReplace { value, .. }
                | Operation::InlineFieldReplace { value, .. }
                | Operation::ContainerReplace { value, .. }
                | Operation::ContainerAppend { element: value, .. } => may(*value),
                Operation::HeapFieldExchange { replacement, .. } => may(*replacement),
                Operation::Mutate { place, value } => proof.is_projected(*place) && may(*value),
                _ => false,
            };
            if escapes {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::OperationContract { reason: REASON },
                    location: VerifyLocation::Instruction(instruction.id),
                    origin: Some(instruction.origin.clone()),
                });
            }
            proof.apply(instruction, &mut state);
        }
        let terminator = block.terminator.as_ref().expect("verified terminator");
        proof.apply_terminator(&terminator.kind, &mut state);
        if let TerminatorKind::Return { values } = &terminator.kind
            && values.iter().any(|value| state[value.index()])
        {
            errors.push(VerifyError {
                kind: VerifyErrorKind::OperationContract { reason: REASON },
                location: VerifyLocation::Terminator(block.id),
                origin: Some(terminator.origin.clone()),
            });
        }
    }
}
