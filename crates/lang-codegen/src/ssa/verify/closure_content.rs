//! Current callable contents at synchronous Borrow reads and source call boundaries.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    closure_escape::{TypeCaptures, proof::content_index},
    content_flow,
};
use crate::ssa::{
    model::{
        ClosureCaptureMode, ClosureCaptureOperand, Definition, EntityId, EntityType, Function,
        FunctionId, Instruction, InstructionId, LoanId, Module, Operation, PlaceAccess,
        TerminatorKind,
    },
    verify_ownership::{AliasRoots, all_entities},
};

#[derive(Clone)]
pub(in crate::ssa) struct CallableContent {
    pub(in crate::ssa) known: bool,
    pub(in crate::ssa) loans: BTreeSet<LoanId>,
}

impl CallableContent {
    fn empty() -> Self {
        Self {
            known: true,
            loans: BTreeSet::new(),
        }
    }
    fn join(&mut self, other: &Self) -> bool {
        let before = (self.known, self.loans.len());
        self.known &= other.known;
        self.loans.extend(&other.loans);
        before != (self.known, self.loans.len())
    }
}

type State = Vec<CallableContent>;

struct Proof<'a> {
    function: &'a Function,
    defaults: State,
    borrowed: Vec<bool>,
    affected: BTreeMap<EntityId, Vec<usize>>,
    view_loans: Vec<bool>,
    trusted_view_fields: Option<&'a BTreeSet<usize>>,
}

/// The erased environment is trusted only where every closure formation binds a checked
/// shared view and no other instruction can call or expose the thunk directly.
pub(super) fn trusted_view_fields(module: &Module) -> BTreeMap<FunctionId, BTreeSet<usize>> {
    let mut fields = BTreeMap::<_, BTreeSet<_>>::new();
    let mut other_entries = BTreeSet::new();
    for instruction in module.functions.iter().flat_map(|body| &body.instructions) {
        match instruction.operation {
            Operation::ClosureConstruct { thunk, closure, .. } => {
                let shared = module
                    .concrete_closure(closure)
                    .map(|(_, captures)| {
                        captures
                            .iter()
                            .enumerate()
                            .filter_map(|(index, capture)| {
                                (capture.mode == ClosureCaptureMode::Shared
                                    && module.callable_view_signature(capture.ty).is_some())
                                .then_some(index)
                            })
                            .collect::<BTreeSet<_>>()
                    })
                    .unwrap_or_default();
                fields
                    .entry(thunk)
                    .and_modify(|previous| {
                        previous.retain(|index| shared.contains(index));
                    })
                    .or_insert(shared);
            }
            Operation::DirectCall { callee, .. }
            | Operation::BorrowCall { callee, .. }
            | Operation::RangeCall { callee, .. }
            | Operation::FunctionAddress { target: callee }
            | Operation::ContainerGenerate {
                initializer: callee,
                ..
            } => {
                other_entries.insert(callee);
            }
            _ => {}
        }
    }
    fields.retain(|thunk, fields| !fields.is_empty() && !other_entries.contains(thunk));
    fields
}

/// A helper's entry Borrow relies on the caller proving the same current capture contents.
pub(in crate::ssa) fn borrowed_callable_contents(
    module: &Module,
    function: &Function,
    capture_types: &TypeCaptures,
    aliases: &AliasRoots,
    trusted_view_fields: Option<&BTreeSet<usize>>,
) -> BTreeMap<(InstructionId, EntityId), CallableContent> {
    if !function
        .instructions
        .iter()
        .any(|instruction| !callable_reads(module, function, instruction).is_empty())
    {
        return BTreeMap::new();
    }
    let proof = Proof::new(
        module,
        function,
        capture_types,
        aliases,
        trusted_view_fields,
    );
    let entries = content_flow::block_entries(
        function,
        &proof.defaults,
        |block, state| {
            for id in &block.instructions {
                proof.apply(
                    function.instruction(*id).expect("verified instruction"),
                    state,
                );
            }
            proof.apply_terminator(
                &block.terminator.as_ref().expect("verified terminator").kind,
                state,
            );
        },
        |edge, state| {
            let target = function.block(edge.target).expect("verified target");
            let rebindings = edge
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
            let rebound = edge
                .arguments
                .iter()
                .zip(&target.parameters)
                .map(|(argument, parameter)| {
                    let mut content = state[content_index(function, *argument)].clone();
                    content.loans = content
                        .loans
                        .iter()
                        .map(|loan| rebindings.get(loan).copied().unwrap_or(*loan))
                        .collect();
                    (content_index(function, *parameter), content)
                })
                .collect::<Vec<_>>();
            let mut incoming = state.clone();
            for (parameter, content) in rebound {
                incoming[parameter] = content;
            }
            incoming
        },
        |previous, incoming| {
            let mut changed = false;
            for (previous, incoming) in previous.iter_mut().zip(incoming) {
                changed |= previous.join(&incoming);
            }
            changed
        },
    );
    let mut reads = BTreeMap::new();
    for block in &function.blocks {
        let mut state = entries[block.id.index()].clone();
        for id in &block.instructions {
            let instruction = function.instruction(*id).expect("verified instruction");
            for initializer in callable_reads(module, function, instruction) {
                reads.insert(
                    (instruction.id, initializer),
                    state[content_index(function, initializer)].clone(),
                );
            }
            proof.apply(instruction, &mut state);
        }
    }
    reads
}

/// Ordinary non-callable Borrow operands retain their existing ownership contract.
fn callable_reads(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
) -> Vec<EntityId> {
    let (receiver, arguments) = match &instruction.operation {
        Operation::ContainerGenerateBorrowed { initializer, .. }
        | Operation::MapWithValue {
            action: initializer,
            ..
        } => {
            return vec![EntityId::Loan(*initializer)];
        }
        Operation::ClosureConstruct { captures, .. } => {
            return captures
                .iter()
                .filter_map(|capture| match capture {
                    ClosureCaptureOperand::Shared(loan) => Some(EntityId::Loan(*loan)),
                    ClosureCaptureOperand::Owned(_) => None,
                })
                .filter(|entity| {
                    let target = function
                        .entity(*entity)
                        .expect("verified entity")
                        .ty
                        .semantic_type();
                    module.callable_view_signature(target).is_some()
                })
                .collect();
        }
        Operation::DirectCall {
            receiver,
            arguments,
            ..
        } => (*receiver, arguments.as_slice()),
        Operation::BorrowCall { arguments, .. } | Operation::RangeCall { arguments, .. } => {
            (None, arguments.as_slice())
        }
        Operation::CallableInvoke {
            callable,
            arguments,
        } => (Some(EntityId::Value(*callable)), arguments.as_slice()),
        Operation::CallableInvokeView { view, arguments } => {
            (Some(EntityId::Loan(*view)), arguments.as_slice())
        }
        _ => return Vec::new(),
    };
    receiver
        .into_iter()
        .chain(arguments.iter().copied())
        .filter(|entity| matches!(entity, EntityId::Loan(_)) || Some(*entity) == receiver)
        .filter(|entity| {
            let target = function
                .entity(*entity)
                .expect("verified entity")
                .ty
                .semantic_type();
            match entity {
                EntityId::Loan(_) => {
                    module.callable_signature(target).is_some()
                        || module.callable_view_signature(target).is_some()
                }
                EntityId::Value(_) => module.concrete_closure(target).is_some(),
                EntityId::Place(_) => false,
            }
        })
        .collect()
}

fn direct_root(function: &Function, entity: EntityId) -> bool {
    matches!(entity, EntityId::Place(_))
        && matches!(function.entity(entity).expect("verified entity").definition,
            Definition::InstructionResult { instruction, .. }
                if matches!(function.instruction(instruction).expect("verified instruction").operation,
                    Operation::RootPlace { .. }))
}

impl<'a> Proof<'a> {
    fn new(
        module: &Module,
        function: &'a Function,
        types: &TypeCaptures,
        aliases: &AliasRoots,
        trusted_view_fields: Option<&'a BTreeSet<usize>>,
    ) -> Self {
        let entities = all_entities(function);
        let view_loans = entities
            .iter()
            .map(|entity| {
                matches!(entity, EntityId::Loan(_))
                    && module
                        .callable_view_signature(
                            function
                                .entity(*entity)
                                .expect("verified entity")
                                .ty
                                .semantic_type(),
                        )
                        .is_some()
            })
            .collect::<Vec<_>>();
        let borrowed = entities
            .iter()
            .zip(&view_loans)
            .map(|(entity, view)| {
                *view
                    || types.contains(
                        function
                            .entity(*entity)
                            .expect("verified entity")
                            .ty
                            .semantic_type(),
                    )
            })
            .collect::<Vec<_>>();
        let defaults = entities.iter().zip(&borrowed).map(|(entity, borrowed)| {
            let data = function.entity(*entity).expect("verified entity");
            let external_borrow = matches!(data.ty, EntityType::Loan { .. })
                && matches!(data.definition, Definition::BlockParameter { block, .. } if Some(block) == function.entry_block());
            // Borrow parameters rely on the caller's active loan, never on invented caller-local IDs.
            CallableContent { known: external_borrow || !borrowed, loans: BTreeSet::new() }
        }).collect();
        let mut affected = BTreeMap::new();
        for instruction in &function.instructions {
            let Operation::Mutate { place, .. } = instruction.operation else {
                continue;
            };
            let written = EntityId::Place(place);
            affected.entry(written).or_insert_with(|| {
                entities
                    .iter()
                    .enumerate()
                    .filter_map(|(index, &entity)| {
                        // A store does not modify an SSA value or a different allocation.
                        if !borrowed[index]
                            || matches!(entity, EntityId::Value(_))
                            || (entity != written
                                && direct_root(function, written)
                                && direct_root(function, entity))
                            || !aliases.overlap(written, entity)
                        {
                            return None;
                        }
                        Some(index)
                    })
                    .collect()
            });
        }
        Self {
            function,
            defaults,
            borrowed,
            affected,
            view_loans,
            trusted_view_fields,
        }
    }

    fn follows_entry_capture(&self, source: LoanId) -> bool {
        let Some(Definition::InstructionResult { instruction, .. }) = self
            .function
            .entity(EntityId::Loan(source))
            .map(|entity| entity.definition)
        else {
            return false;
        };
        let Operation::SharedFieldLoan { base, field } = self
            .function
            .instruction(instruction)
            .expect("verified instruction")
            .operation
        else {
            return false;
        };
        self.trusted_view_fields
            .is_some_and(|fields| fields.contains(&field))
            && self.function.blocks[0].parameters.first() == Some(&EntityId::Loan(base))
    }

    fn apply(&self, instruction: &Instruction, state: &mut State) {
        let content = |entity| state[content_index(self.function, entity)].clone();
        let value = |value| content(EntityId::Value(value));
        let result = match &instruction.operation {
            Operation::ClosureConstruct { captures, .. } => {
                let mut result = CallableContent::empty();
                for capture in captures {
                    match capture {
                        ClosureCaptureOperand::Shared(loan) => {
                            result.loans.insert(*loan);
                            result.join(&content(EntityId::Loan(*loan)));
                        }
                        ClosureCaptureOperand::Owned(owner) => {
                            result.join(&value(*owner));
                        }
                    }
                }
                Some(result)
            }
            Operation::FunctionAddress { .. } | Operation::NullableNull { .. } => {
                Some(CallableContent::empty())
            }
            Operation::AggregateConstruct { fields, .. }
            | Operation::ContainerConstruct {
                elements: fields, ..
            } => {
                let mut result = CallableContent::empty();
                for field in fields {
                    result.join(&value(*field));
                }
                Some(result)
            }
            Operation::TaggedConstruct { payload, .. }
            | Operation::HeapAllocate { payload, .. }
            | Operation::SharedAllocate { payload, .. } => Some(value(*payload)),
            Operation::NullableWrap { owner, .. } | Operation::NullableTake { owner, .. } => {
                Some(value(*owner))
            }
            Operation::SharedRetain { owner } => Some(content(*owner)),
            Operation::RootPlace { owner } => Some(value(*owner)),
            Operation::BorrowBegin { place, .. } => Some(content(EntityId::Place(*place))),
            Operation::SharedReborrow { source } | Operation::CallableViewOf { source } => {
                Some(content(EntityId::Loan(*source)))
            }
            // Following an address cannot recover captured LoanIds from a capture-free slot.
            // Capturing targets keep the unknown default; capture-free targets keep exact emptiness.
            Operation::SharedReferenceFollow { source }
                if self.view_loans[content_index(self.function, instruction.results[0])]
                    && self.follows_entry_capture(*source) =>
            {
                Some(CallableContent::empty())
            }
            Operation::SharedReferenceFollow { .. } => None,
            Operation::RootPlaceTake { place, .. } => Some(content(EntityId::Place(*place))),
            Operation::Read { source } => Some(content(match source {
                PlaceAccess::Place(place) => EntityId::Place(*place),
                PlaceAccess::Loan(loan) => EntityId::Loan(*loan),
            })),
            Operation::Copy { source } => Some(value(*source)),
            Operation::RootReplace {
                loan, replacement, ..
            } => {
                let replacement = value(*replacement);
                let old = content(EntityId::Loan(*loan));
                for (result, contents) in instruction.results.iter().zip([replacement, old]) {
                    state[content_index(self.function, *result)] = contents;
                }
                return;
            }
            Operation::RootSwap { loans, .. } => {
                let left = content(EntityId::Loan(loans[0]));
                let right = content(EntityId::Loan(loans[1]));
                for (result, contents) in instruction.results.iter().zip([right, left]) {
                    state[content_index(self.function, *result)] = contents;
                }
                return;
            }
            // These projections cannot identify a field's capture IDs. Capture-free target types
            // still use the exact empty default; borrowed callable contents remain explicitly unknown.
            Operation::FieldPlace { .. }
            | Operation::SharedFieldLoan { .. }
            | Operation::SharedHeapFieldLoan { .. }
            | Operation::HeapPayloadPlace { .. }
            | Operation::SharedPayloadPlace { .. }
            | Operation::ContainerElementPlace { .. }
            | Operation::RangeElementPlace { .. }
            | Operation::TaggedPayloadPlace { .. }
            | Operation::AggregateProject { .. }
            | Operation::AggregateExplode { .. }
            | Operation::AggregateCopyExplode { .. } => None,
            _ => None,
        };
        if let Operation::Mutate {
            place,
            value: replacement,
        } = instruction.operation
        {
            let replacement = value(replacement);
            for index in &self.affected[&EntityId::Place(place)] {
                state[*index].join(&replacement);
            }
            // This operand denotes the complete overwritten value even for a projected place.
            state[content_index(self.function, EntityId::Place(place))] = replacement;
        }
        for entity in &instruction.results {
            let index = content_index(self.function, *entity);
            state[index] = if !self.borrowed[index] {
                CallableContent::empty()
            } else {
                result
                    .as_ref()
                    .map_or_else(|| self.defaults[index].clone(), |result| result.clone())
            };
        }
    }

    fn apply_terminator(&self, terminator: &TerminatorKind, state: &mut State) {
        if let TerminatorKind::NullableBranch { owner, view, .. } = terminator {
            state[content_index(self.function, EntityId::Loan(*view))] =
                state[content_index(self.function, EntityId::Value(*owner))].clone();
        }
    }
}
