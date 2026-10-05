//! Current callable contents for borrowed generation; no general loan-lifecycle inference.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    closure_escape::{
        TypeCaptures,
        proof::{content_index, projected_places},
    },
    content_flow,
};
use crate::ssa::{
    model::{
        ClosureCaptureOperand, Definition, EntityId, EntityType, Function, Instruction,
        InstructionId, LoanId, Module, Operation, PlaceAccess, TerminatorKind,
    },
    verify_ownership::{AliasRoots, all_entities, root_exchange},
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
    affected: BTreeMap<EntityId, Vec<(usize, bool)>>,
}

/// Only generator reads are exposed; other ownership operations keep their existing contract.
pub(in crate::ssa) fn generator_contents(
    module: &Module,
    function: &Function,
) -> BTreeMap<InstructionId, CallableContent> {
    if !function.instructions.iter().any(|instruction| {
        matches!(
            instruction.operation,
            Operation::ContainerGenerateBorrowed { .. }
        )
    }) {
        return BTreeMap::new();
    }
    let proof = Proof::new(module, function);
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
            if let Operation::ContainerGenerateBorrowed { initializer, .. } = instruction.operation
            {
                reads.insert(
                    instruction.id,
                    state[content_index(function, EntityId::Loan(initializer))].clone(),
                );
            }
            proof.apply(instruction, &mut state);
        }
    }
    reads
}

impl<'a> Proof<'a> {
    fn new(module: &Module, function: &'a Function) -> Self {
        let types = TypeCaptures::compute(module);
        let entities = all_entities(function);
        let borrowed = entities
            .iter()
            .map(|entity| {
                types.contains(
                    function
                        .entity(*entity)
                        .expect("verified entity")
                        .ty
                        .semantic_type(),
                )
            })
            .collect::<Vec<_>>();
        let defaults = entities.iter().map(|entity| {
            let data = function.entity(*entity).expect("verified entity");
            let external_borrow = matches!(data.ty, EntityType::Loan { .. })
                && matches!(data.definition, Definition::BlockParameter { block, .. } if Some(block) == function.entry_block());
            // Borrow parameters rely on the caller's active loan, never on invented caller-local IDs.
            CallableContent { known: external_borrow || !types.contains(data.ty.semantic_type()), loans: BTreeSet::new() }
        }).collect();
        let aliases = AliasRoots::compute(function);
        let projected = projected_places(function, &aliases);
        let mut affected = BTreeMap::new();
        for instruction in &function.instructions {
            let Operation::Mutate { place, .. } = instruction.operation else {
                continue;
            };
            affected.entry(EntityId::Place(place)).or_insert_with(|| {
                let exact_owners = if projected[place.index()] {
                    vec![]
                } else {
                    entities
                        .iter()
                        .filter_map(|entity| match entity {
                            EntityId::Value(owner)
                                if root_exchange::exact_place(
                                    function, *owner, place, &aliases,
                                ) =>
                            {
                                Some(*owner)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                };
                entities
                    .iter()
                    .enumerate()
                    .filter_map(|(index, entity)| {
                        if !borrowed[index] || !aliases.overlap(EntityId::Place(place), *entity) {
                            return None;
                        }
                        let exact = match entity {
                            EntityId::Value(owner) => exact_owners.contains(owner),
                            EntityId::Place(other) => exact_owners.iter().any(|owner| {
                                root_exchange::exact_place(function, *owner, *other, &aliases)
                            }),
                            _ => false,
                        };
                        Some((index, exact))
                    })
                    .collect()
            });
        }
        Self {
            function,
            defaults,
            borrowed,
            affected,
        }
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
            Operation::SharedReborrow { source } => Some(content(EntityId::Loan(*source))),
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
            for (index, exact) in &self.affected[&EntityId::Place(place)] {
                if *exact {
                    state[*index] = replacement.clone();
                } else {
                    state[*index].join(&replacement);
                }
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
