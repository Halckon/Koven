//! Reject unproved borrowed environments only at owned escape delivery boundaries.

pub(in crate::ssa::verify) mod proof;

use std::collections::VecDeque;

use super::{VerifyError, VerifyErrorKind, VerifyLocation};
use crate::ssa::model::{
    ClosureCaptureMode, EntityId, Function, Module, Operation, SsaTypeId, SsaTypeKind,
    TerminatorKind,
};

#[cfg(test)]
#[path = "closure_escape/types/graph_tests.rs"]
mod graph_tests;

const REASON: &str = "borrowed closure cannot escape through owned value delivery";

pub(in crate::ssa) struct TypeCaptures {
    may_contain: Vec<bool>,
    #[cfg(test)]
    stats: ClassificationStats,
}

#[cfg(test)]
#[derive(Default)]
struct ClassificationStats {
    node_visits: usize,
    storage_edge_visits: usize,
    propagation_edge_visits: usize,
}

impl TypeCaptures {
    /// The module's type and deinit checks must have succeeded before indexing storage edges.
    pub(in crate::ssa::verify) fn compute(module: &Module) -> Self {
        let mut parents = vec![Vec::new(); module.types.len()];
        let mut may_contain = vec![false; module.types.len()];
        let mut pending = VecDeque::new();
        #[cfg(test)]
        let mut stats = ClassificationStats::default();
        for (index, kind) in module.types.iter().enumerate() {
            #[cfg(test)]
            {
                stats.node_visits += 1;
            }
            let children = match kind {
                SsaTypeKind::Aggregate { fields, .. } => fields.clone(),
                SsaTypeKind::TaggedUnion { variants, .. } => variants.clone(),
                SsaTypeKind::HeapOwner { payload, .. }
                | SsaTypeKind::SharedOwner { payload, .. } => payload.iter().copied().collect(),
                SsaTypeKind::NullableHandle { inner }
                | SsaTypeKind::InlineNullable { inner, .. } => vec![*inner],
                SsaTypeKind::SequentialContainer { element, .. } => vec![*element],
                SsaTypeKind::ConcreteClosure { captures, .. } => {
                    if captures
                        .iter()
                        .any(|capture| capture.mode == ClosureCaptureMode::Shared)
                    {
                        may_contain[index] = true;
                        pending.push_back(index);
                    }
                    captures
                        .iter()
                        .filter(|capture| capture.mode == ClosureCaptureMode::Owned)
                        .map(|capture| capture.ty)
                        .collect()
                }
                // Signatures and shared-reference targets do not describe owned contents.
                _ => Vec::new(),
            };
            for child in children {
                parents[child.index()].push(index);
                #[cfg(test)]
                {
                    stats.storage_edge_visits += 1;
                }
            }
        }
        while let Some(child) = pending.pop_front() {
            for parent in &parents[child] {
                #[cfg(test)]
                {
                    stats.propagation_edge_visits += 1;
                }
                if !may_contain[*parent] {
                    may_contain[*parent] = true;
                    pending.push_back(*parent);
                }
            }
        }
        Self {
            may_contain,
            #[cfg(test)]
            stats,
        }
    }

    pub(in crate::ssa::verify) fn contains(&self, ty: SsaTypeId) -> bool {
        self.may_contain[ty.index()]
    }
}

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
                Operation::BorrowCall { arguments, .. }
                | Operation::RangeCall { arguments, .. }
                | Operation::CallableInvoke { arguments, .. }
                | Operation::CallableInvokeView { arguments, .. } => arguments
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
                | Operation::ContainerAppend { element: value, .. }
                | Operation::ContainerInsertAt { element: value, .. } => may(*value),
                Operation::HeapFieldExchange { replacement, .. } => may(*replacement),
                Operation::Mutate { place, value } => proof.is_projected(*place) && may(*value),
                Operation::LoanMutate { value, .. } => may(*value),
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
