//! Program-point content proofs. These facts never replace capture-loan lifetime checks.

use std::collections::BTreeMap;

use super::TypeCaptures;
use crate::ssa::verify::content_flow;
use crate::ssa::{
    model::{
        ClosureCaptureOperand, EntityId, Function, Instruction, Operation, PlaceAccess, PlaceId,
        TerminatorKind, ValueId,
    },
    verify_ownership::{AliasRoots, all_entities, edges, root_exchange},
};

pub(super) type State = Vec<bool>;

pub(super) struct ValueProof<'a> {
    function: &'a Function,
    defaults: State,
    projected: Vec<bool>,
    affected: BTreeMap<EntityId, Vec<(usize, bool)>>,
}

impl<'a> ValueProof<'a> {
    pub(super) fn new(function: &'a Function, types: &TypeCaptures) -> Self {
        // Keep Values first so delivery checks can still index directly by ValueId.
        let entities = all_entities(function);
        let defaults = entities
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
        let aliases = AliasRoots::compute(function);
        let projected = projected_places(function, &aliases);
        let mut affected = BTreeMap::new();
        for instruction in &function.instructions {
            let access = match instruction.operation {
                Operation::Mutate { place, .. } => EntityId::Place(place),
                Operation::HeapFieldReplace { receiver, .. }
                | Operation::InlineFieldReplace { receiver, .. } => EntityId::Loan(receiver),
                Operation::HeapFieldExchange { owner, .. }
                | Operation::ContainerReplace { owner, .. } => EntityId::Value(owner),
                _ => continue,
            };
            affected.entry(access).or_insert_with(|| {
                let exact_owners = match access {
                    EntityId::Place(place) if !projected[place.index()] => entities
                        .iter()
                        .filter_map(|entity| match entity {
                            EntityId::Value(owner)
                                if aliases.overlap(access, *entity)
                                    && root_exchange::exact_place(
                                        function, *owner, place, &aliases,
                                    ) =>
                            {
                                Some(*owner)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                entities
                    .iter()
                    .enumerate()
                    .filter_map(|(index, entity)| {
                        if !defaults[index] || !aliases.overlap(access, *entity) {
                            return None;
                        }
                        let exact = match entity {
                            EntityId::Value(owner) => exact_owners.contains(owner),
                            EntityId::Place(place) => exact_owners.iter().any(|owner| {
                                root_exchange::exact_inline_place(
                                    function, *owner, *place, &aliases,
                                )
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
            projected,
            affected,
        }
    }

    pub(super) fn is_projected(&self, place: PlaceId) -> bool {
        self.projected[place.index()]
    }

    /// OR joins preserve proofs only when every incoming execution is known clean.
    pub(super) fn block_entries(&self) -> Vec<State> {
        let function = self.function;
        content_flow::block_entries(
            function,
            &self.defaults,
            |block, state| {
                for id in &block.instructions {
                    self.apply(
                        function.instruction(*id).expect("verified instruction"),
                        state,
                    );
                }
                self.apply_terminator(
                    &block.terminator.as_ref().expect("verified terminator").kind,
                    state,
                );
            },
            |edge, state| {
                let target = function.block(edge.target).expect("verified target");
                let rebound = edge
                    .arguments
                    .iter()
                    .zip(&target.parameters)
                    .map(|(argument, parameter)| {
                        (
                            content_index(function, *parameter),
                            state[content_index(function, *argument)],
                        )
                    })
                    .collect::<Vec<_>>();
                let mut incoming = state.clone();
                for (parameter, may_capture) in rebound {
                    incoming[parameter] = may_capture;
                }
                incoming
            },
            |previous, incoming| {
                let mut changed = false;
                for (previous, incoming) in previous.iter_mut().zip(incoming) {
                    if incoming && !*previous {
                        *previous = true;
                        changed = true;
                    }
                }
                changed
            },
        )
    }

    pub(super) fn apply(&self, instruction: &Instruction, state: &mut State) {
        let may = |value: ValueId| state[value.index()];
        let results = instruction
            .results
            .iter()
            .filter_map(|entity| match entity {
                EntityId::Value(value) => Some(*value),
                _ => None,
            })
            .collect::<Vec<_>>();
        let proof = match &instruction.operation {
            Operation::FunctionAddress { .. } | Operation::NullableNull { .. } => Some(false),
            Operation::ClosureConstruct { captures, .. } => {
                Some(captures.iter().any(|capture| match capture {
                    ClosureCaptureOperand::Shared(_) => true,
                    ClosureCaptureOperand::Owned(value) => may(*value),
                }))
            }
            Operation::AggregateConstruct { fields, .. } => Some(fields.iter().copied().any(may)),
            Operation::ContainerConstruct { elements, .. } => {
                Some(elements.iter().copied().any(may))
            }
            Operation::TaggedConstruct { payload, .. }
            | Operation::HeapAllocate { payload, .. }
            | Operation::SharedAllocate { payload, .. } => Some(may(*payload)),
            Operation::NullableWrap { owner, .. }
            | Operation::NullableTake { owner, .. }
            | Operation::RootPlaceTake { owner, .. } => Some(may(*owner)),
            Operation::Copy { source } => Some(may(*source)),
            Operation::SharedRetain { owner }
            | Operation::SharedPayloadPlace { owner }
            | Operation::ContainerElementPlace { owner, .. } => {
                Some(state[content_index(self.function, *owner)])
            }
            Operation::RootPlace { owner }
            | Operation::HeapPayloadPlace { owner }
            | Operation::TaggedPayloadPlace { owner, .. } => Some(may(*owner)),
            Operation::BorrowBegin { place, .. } | Operation::FieldPlace { base: place, .. } => {
                Some(state[content_index(self.function, EntityId::Place(*place))])
            }
            Operation::SharedReborrow { source }
            | Operation::SharedFieldLoan { base: source, .. }
            | Operation::SharedHeapFieldLoan { base: source, .. } => {
                Some(state[content_index(self.function, EntityId::Loan(*source))])
            }
            // A reference slot does not prove its target's owned contents. Use the target default.
            Operation::SharedReferenceFollow { .. } => None,
            Operation::Read { source } => Some(
                state[content_index(
                    self.function,
                    match source {
                        PlaceAccess::Place(place) => EntityId::Place(*place),
                        PlaceAccess::Loan(loan) => EntityId::Loan(*loan),
                    },
                )],
            ),
            Operation::AggregateProject { aggregate, .. }
            | Operation::AggregateExplode { aggregate }
            | Operation::AggregateCopyExplode { aggregate }
                if !may(*aggregate) =>
            {
                Some(false)
            }
            Operation::RootReplace {
                owner, replacement, ..
            } => {
                let old = may(*owner);
                let replacement = may(*replacement);
                state[results[0].index()] = replacement;
                state[results[1].index()] = old;
                return;
            }
            Operation::RootSwap { owners, .. } => {
                let left = may(owners[0]);
                let right = may(owners[1]);
                state[results[0].index()] = right;
                state[results[1].index()] = left;
                return;
            }
            Operation::HeapFieldExchange { owner, .. } if !may(*owner) => Some(false),
            _ => None,
        };
        let write = match instruction.operation {
            Operation::Mutate { place, value } => Some((
                EntityId::Place(place),
                may(value),
                !self.is_projected(place),
            )),
            Operation::HeapFieldReplace {
                receiver, value, ..
            }
            | Operation::InlineFieldReplace {
                receiver, value, ..
            } => Some((EntityId::Loan(receiver), may(value), false)),
            Operation::HeapFieldExchange {
                owner, replacement, ..
            } => Some((EntityId::Value(owner), may(replacement), false)),
            Operation::ContainerReplace { owner, value, .. } => {
                Some((EntityId::Value(owner), may(value), false))
            }
            _ => None,
        };
        if let Some((access, replacement, whole_root)) = write {
            for (owner, exact) in &self.affected[&access] {
                // Equal overlap sets alone do not prove owner/place pairing across CFG joins.
                if whole_root && *exact {
                    state[*owner] = replacement;
                } else {
                    state[*owner] |= replacement;
                }
            }
            if matches!(instruction.operation, Operation::Mutate { .. }) {
                // The written operand itself denotes exactly the overwritten contents;
                // other overlapping places/owners need the pairing proof above to be cleared.
                let target = content_index(self.function, access);
                state[target] = self.defaults[target] && replacement;
            }
        }
        for result in &instruction.results {
            let index = content_index(self.function, *result);
            state[index] = self.defaults[index] && proof.unwrap_or(true);
        }
    }

    pub(super) fn apply_terminator(&self, terminator: &TerminatorKind, state: &mut State) {
        if let TerminatorKind::NullableBranch { owner, view, .. } = terminator {
            let view = content_index(self.function, EntityId::Loan(*view));
            state[view] = self.defaults[view] && state[owner.index()];
        }
    }
}

pub(in crate::ssa::verify) fn content_index(function: &Function, entity: EntityId) -> usize {
    match entity {
        EntityId::Value(value) => value.index(),
        EntityId::Place(place) => function.values.len() + place.index(),
        EntityId::Loan(loan) => function.values.len() + function.places.len() + loan.index(),
    }
}

/// A mixed root/projected CFG join is a possible storage write, never a proven local root.
pub(in crate::ssa::verify) fn projected_places(
    function: &Function,
    aliases: &AliasRoots,
) -> Vec<bool> {
    let mut projected = vec![false; function.places.len()];
    let mut incoming = vec![false; function.places.len()];
    for block in &function.blocks {
        for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind) {
            for parameter in &function
                .block(edge.target)
                .expect("verified target")
                .parameters
            {
                if let EntityId::Place(place) = parameter {
                    incoming[place.index()] = true;
                }
            }
        }
    }
    for block in &function.blocks {
        for parameter in &block.parameters {
            if let EntityId::Place(place) = parameter {
                projected[place.index()] = !incoming[place.index()]
                    || !aliases.overlap(EntityId::Place(*place), EntityId::Place(*place));
            }
        }
    }
    for instruction in &function.instructions {
        if matches!(
            instruction.operation,
            Operation::FieldPlace { .. }
                | Operation::ContainerElementPlace { .. }
                | Operation::TaggedPayloadPlace { .. }
                | Operation::HeapPayloadPlace { .. }
                | Operation::SharedPayloadPlace { .. }
        ) {
            let EntityId::Place(place) = instruction.results[0] else {
                unreachable!("verified place result")
            };
            projected[place.index()] = true;
        }
    }
    loop {
        let mut changed = false;
        for block in &function.blocks {
            for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind) {
                for (argument, parameter) in edge.arguments.iter().zip(
                    &function
                        .block(edge.target)
                        .expect("verified target")
                        .parameters,
                ) {
                    if let (EntityId::Place(argument), EntityId::Place(parameter)) =
                        (argument, parameter)
                        && projected[argument.index()]
                        && !projected[parameter.index()]
                    {
                        projected[parameter.index()] = true;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    projected
}
