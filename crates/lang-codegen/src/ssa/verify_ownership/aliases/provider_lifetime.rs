//! Bounded path-correlated loan provenance for provider places; no may-origin union.
use super::super::super::{
    model::{
        BlockId, Definition, EntityId, EntityType, Function, LoanId, LoanKind, Operation,
        PlaceAccess, PlaceId, TerminatorKind, ValueId,
    },
    verify::{VerifyError, VerifyErrorKind, VerifyLocation, compute_dominators},
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(in crate::ssa) struct EntryProjection {
    pub(in crate::ssa) dominators: Vec<BTreeSet<usize>>,
}

impl EntryProjection {
    pub(in crate::ssa) fn new(function: &Function) -> Self {
        Self {
            dominators: compute_dominators(function),
        }
    }

    pub(in crate::ssa) fn retain_entry(
        &self,
        function: &Function,
        block: BlockId,
        state: &mut State,
    ) {
        let mut loans = state
            .loans
            .keys()
            .copied()
            .filter(|loan| self.available(function, block, EntityId::Loan(*loan)))
            .collect::<BTreeSet<_>>();
        state
            .places
            .retain(|place, _| self.available(function, block, EntityId::Place(*place)));
        state
            .cursors
            .retain(|value, _| self.available(function, block, EntityId::Value(*value)));
        let mut pending = loans.iter().copied().collect::<Vec<_>>();
        for reference in state.places.values().chain(state.cursors.values()) {
            pending.extend(reference.lineage.iter().copied());
        }
        let mut expanded = BTreeSet::new();
        while let Some(loan) = pending.pop() {
            if !expanded.insert(loan) {
                continue;
            }
            if let Some(reference) = state.loans.get(&loan) {
                loans.insert(loan);
                pending.extend(reference.lineage.iter().copied());
            }
        }
        state.loans.retain(|loan, _| loans.contains(loan));
    }

    pub(in crate::ssa) fn available(
        &self,
        function: &Function,
        block: BlockId,
        entity: EntityId,
    ) -> bool {
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::ssa) struct Reference {
    pub(in crate::ssa) lineage: BTreeSet<LoanId>,
    pub(in crate::ssa) origin: LoanId,
    pub(in crate::ssa) root_storage: Option<LoanId>,
    pub(in crate::ssa) live: bool,
}

#[derive(Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::ssa) struct State {
    pub(in crate::ssa) loans: BTreeMap<LoanId, Reference>,
    pub(in crate::ssa) places: BTreeMap<PlaceId, Reference>,
    pub(in crate::ssa) cursors: BTreeMap<ValueId, Reference>,
}

impl State {
    /// An incoming parameter can name a hidden origin whose borrows are still live.
    /// Rename that origin throughout the state so equivalent incoming states can meet.
    /// Existing tokens keep their identities and each path retains its alias relationships.
    fn name_hidden_origins(
        &mut self,
        function: &Function,
        block: BlockId,
        projection: &EntryProjection,
    ) {
        let parameters = &function.block(block).expect("verified block").parameters;
        if parameters.is_empty() {
            return;
        }
        let tokens: BTreeSet<_> = self
            .loans
            .values()
            .chain(self.places.values())
            .chain(self.cursors.values())
            .flat_map(|reference| reference.lineage.iter().copied())
            .collect();
        let mut names = BTreeMap::new();
        for entity in parameters {
            let EntityId::Loan(parameter) = *entity else {
                continue;
            };
            if tokens.contains(&parameter) {
                continue;
            }
            let Some(reference) = self.loans.get(&parameter) else {
                continue;
            };
            let origin = reference.origin;
            if !projection.available(function, block, EntityId::Loan(origin)) {
                names.entry(origin).or_insert(parameter);
            }
        }
        if names.is_empty() {
            return;
        }
        self.loans
            .retain(|loan, reference| *loan != reference.origin || !names.contains_key(loan));
        for reference in self
            .loans
            .values_mut()
            .chain(self.places.values_mut())
            .chain(self.cursors.values_mut())
        {
            if let Some(&origin) = names.get(&reference.origin) {
                reference.origin = origin;
            }
            if let Some(root) = reference.root_storage {
                reference.root_storage = Some(names.get(&root).copied().unwrap_or(root));
            }
            reference.lineage = reference
                .lineage
                .iter()
                .map(|token| names.get(token).copied().unwrap_or(*token))
                .collect();
        }
    }

    fn invalidate_places(&mut self, loan: LoanId) {
        if let Some(reference) = self.loans.get(&loan) {
            self.invalidate_places_of(reference.root_storage.unwrap_or(reference.origin));
        }
    }

    fn invalidate_places_of(&mut self, origin: LoanId) {
        for place in self.places.values_mut() {
            if place.lineage.contains(&origin) {
                place.live = false;
            }
        }
    }

    fn commit_exchange(&mut self, loan: LoanId) {
        if let Some(root) = self
            .loans
            .get(&loan)
            .and_then(|reference| reference.root_storage)
        {
            self.invalidate_places_of(root);
        }
        self.end(loan);
    }

    pub(in crate::ssa) fn end(&mut self, loan: LoanId) {
        let Some(token) = self.loans.get(&loan).map(|reference| reference.origin) else {
            return;
        };
        for reference in self
            .loans
            .values_mut()
            .chain(self.places.values_mut())
            .chain(self.cursors.values_mut())
        {
            if reference.lineage.contains(&token) {
                reference.live = false;
            }
        }
    }
}

pub(in crate::ssa) fn verify(function: &Function, errors: &mut Vec<VerifyError>) {
    let has_nullable_borrow = function.blocks.iter().any(|block| {
        matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::NullableLoanBranch { .. })
        )
    });
    if !has_nullable_borrow
        && !function.instructions.iter().any(|instruction| {
            matches!(
                instruction.operation,
                Operation::ContainerElementPlace { .. }
                    | Operation::ExclusiveHeapFieldLoan { .. }
                    | Operation::ExclusiveRootReborrow { .. }
                    | Operation::SharedHeapFieldLoan { .. }
                    | Operation::NullableLoanIsNull { .. }
                    | Operation::RangeElementPlace { .. }
                    | Operation::MapIterationFirst { .. }
                    | Operation::MapIterationHasNext { .. }
                    | Operation::MapIterationNext { .. }
                    | Operation::MapIterationKeyLoan { .. }
                    | Operation::MapIterationValueLoan { .. }
            )
        })
    {
        return;
    }
    let Some(entry) = function.entry_block() else {
        return;
    };
    let mut initial = State::default();
    for entity in &function.block(entry).expect("verified entry").parameters {
        if let EntityId::Loan(loan) = entity {
            initial.loans.insert(
                *loan,
                Reference {
                    lineage: BTreeSet::from([*loan]),
                    origin: *loan,
                    root_storage: None,
                    live: true,
                },
            );
        }
    }
    let projection = EntryProjection::new(function);
    let mut queue = VecDeque::from([(entry, initial)]);
    let mut visited = BTreeSet::<(BlockId, State)>::new();
    let budget = function.blocks.len().saturating_mul(256).max(256);
    while let Some((block_id, mut state)) = queue.pop_front() {
        projection.retain_entry(function, block_id, &mut state);
        if has_nullable_borrow {
            state.name_hidden_origins(function, block_id, &projection);
        }
        if !visited.insert((block_id, state.clone())) {
            continue;
        }
        if visited.len() > budget {
            errors.push(VerifyError {
                kind: VerifyErrorKind::OperationContract {
                    reason: "provider provenance fixed-point budget exceeded",
                },
                location: VerifyLocation::Block(block_id),
                origin: Some(function.origin.clone()),
            });
            return;
        }
        let block = function.block(block_id).expect("verified block");
        for id in &block.instructions {
            let instruction = function.instruction(*id).expect("verified instruction");
            let result = instruction.results.first().copied();
            if matches!(
                instruction.operation,
                Operation::DirectCall { .. }
                    | Operation::BorrowCall { .. }
                    | Operation::RangeCall { .. }
                    | Operation::CallableInvoke { .. }
                    | Operation::CallableInvokeView { .. }
            ) {
                for entity in instruction.operation.entities() {
                    if let EntityId::Loan(loan) = entity
                        && matches!(
                            function.entity(entity).map(|entity| entity.ty),
                            Some(EntityType::Loan {
                                kind: LoanKind::Exclusive,
                                ..
                            })
                        )
                    {
                        state.invalidate_places(loan);
                    }
                }
            }
            let inherited = match instruction.operation {
                Operation::MapIterationFirst { source } => {
                    match map_reference(&state, source, None) {
                        Ok(reference) => Some(reference),
                        Err(kind) => {
                            errors.push(VerifyError {
                                kind,
                                location: VerifyLocation::Instruction(*id),
                                origin: Some(instruction.origin.clone()),
                            });
                            return;
                        }
                    }
                }
                Operation::MapIterationHasNext { source, cursor }
                | Operation::MapIterationNext { source, cursor }
                | Operation::MapIterationKeyLoan { source, cursor }
                | Operation::MapIterationValueLoan { source, cursor } => {
                    match map_reference(&state, source, Some(cursor)) {
                        Ok(reference) => Some(reference),
                        Err(kind) => {
                            errors.push(VerifyError {
                                kind,
                                location: VerifyLocation::Instruction(*id),
                                origin: Some(instruction.origin.clone()),
                            });
                            return;
                        }
                    }
                }
                Operation::ContainerElementPlace {
                    owner: EntityId::Loan(loan),
                    ..
                }
                | Operation::RangeElementPlace { view: loan, .. }
                | Operation::NullableLoanIsNull { source: loan }
                | Operation::SharedReborrow { source: loan }
                | Operation::ExclusiveRootReborrow { source: loan }
                | Operation::BorrowCall { source: loan, .. }
                | Operation::MapRequireValue { source: loan, .. }
                | Operation::SharedReferenceFollow { source: loan }
                | Operation::SharedFieldLoan { base: loan, .. }
                | Operation::SharedHeapFieldLoan { base: loan, .. }
                | Operation::ExclusiveHeapFieldLoan { base: loan, .. } => {
                    let reference = state.loans.get(&loan).cloned();
                    if reference.as_ref().is_some_and(|reference| !reference.live) {
                        errors.push(VerifyError {
                            kind: VerifyErrorKind::LoanInactive { loan },
                            location: VerifyLocation::Instruction(*id),
                            origin: Some(instruction.origin.clone()),
                        });
                        return;
                    }
                    reference
                }
                Operation::BorrowBegin { place, .. }
                | Operation::FieldPlace { base: place, .. }
                | Operation::Mutate { place, .. }
                | Operation::Read {
                    source: PlaceAccess::Place(place),
                } => {
                    let reference = state.places.get(&place).cloned();
                    if reference.as_ref().is_some_and(|reference| !reference.live) {
                        errors.push(VerifyError {
                            kind: VerifyErrorKind::PlaceUnavailable { place },
                            location: VerifyLocation::Instruction(*id),
                            origin: Some(instruction.origin.clone()),
                        });
                        return;
                    }
                    reference
                }
                Operation::LoanReplace { loan, .. }
                | Operation::LoanMutate { loan, .. }
                | Operation::ContainerAppendInout { loan, .. } => {
                    state.invalidate_places(loan);
                    None
                }
                Operation::ProjectedSwap { loans, .. } => {
                    for loan in loans {
                        state.commit_exchange(loan);
                    }
                    None
                }
                Operation::InoutExchange { loan, .. } => {
                    state.commit_exchange(loan);
                    None
                }
                Operation::BorrowEnd { loan }
                | Operation::HeapReceiverFieldExchange { loan, .. } => {
                    state.end(loan);
                    None
                }
                _ => None,
            };
            match result {
                Some(EntityId::Value(value)) => {
                    state.cursors.remove(&value);
                    if matches!(
                        instruction.operation,
                        Operation::MapIterationFirst { .. } | Operation::MapIterationNext { .. }
                    ) && let Some(reference) = inherited
                    {
                        state.cursors.insert(value, reference);
                    }
                }
                Some(EntityId::Loan(loan)) => {
                    let mut reference = inherited.unwrap_or(Reference {
                        lineage: BTreeSet::new(),
                        origin: loan,
                        root_storage: None,
                        live: true,
                    });
                    reference.root_storage = matches!(
                        instruction.operation,
                        Operation::ExclusiveRootReborrow { .. }
                    )
                    .then_some(reference.origin);
                    reference.lineage.insert(loan);
                    reference.origin = loan;
                    state.loans.insert(loan, reference);
                }
                Some(EntityId::Place(place)) => {
                    if let Some(reference) = inherited {
                        state.places.insert(place, reference);
                    } else {
                        state.places.remove(&place);
                    }
                }
                _ => {}
            }
        }
        let terminator = &block.terminator.as_ref().expect("verified terminator").kind;
        if let TerminatorKind::NullableLoanBranch { source, .. } = terminator
            && state
                .loans
                .get(source)
                .is_none_or(|reference| !reference.live)
        {
            errors.push(VerifyError {
                kind: VerifyErrorKind::LoanInactive { loan: *source },
                location: VerifyLocation::Terminator(block_id),
                origin: block.terminator.as_ref().map(|term| term.origin.clone()),
            });
            return;
        }
        for edge in super::super::edges(terminator) {
            let mut successor = state.clone();
            let target = function.block(edge.target).expect("verified edge");
            for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                match (argument, parameter) {
                    (EntityId::Loan(from), EntityId::Loan(to)) => {
                        successor.loans.remove(to);
                        if let Some(reference) = state.loans.get(from) {
                            successor.loans.insert(*to, reference.clone());
                        }
                    }
                    (EntityId::Place(from), EntityId::Place(to)) => {
                        successor.places.remove(to);
                        if let Some(reference) = state.places.get(from) {
                            successor.places.insert(*to, reference.clone());
                        }
                    }
                    (EntityId::Value(from), EntityId::Value(to)) => {
                        successor.cursors.remove(to);
                        if let Some(reference) = state.cursors.get(from) {
                            successor.cursors.insert(*to, reference.clone());
                        }
                    }
                    _ => {}
                }
            }
            if let TerminatorKind::NullableBranch {
                view,
                when_non_null,
                ..
            } = terminator
                && std::ptr::eq(edge, when_non_null)
            {
                successor.loans.insert(
                    *view,
                    Reference {
                        lineage: BTreeSet::from([*view]),
                        origin: *view,
                        root_storage: None,
                        live: true,
                    },
                );
            }
            if let TerminatorKind::NullableLoanBranch {
                source,
                view,
                when_non_null,
                ..
            } = terminator
                && std::ptr::eq(edge, when_non_null)
                && let Some(mut reference) = state.loans.get(source).cloned()
            {
                reference.lineage.insert(*view);
                reference.origin = *view;
                successor.loans.insert(*view, reference);
            }
            queue.push_back((edge.target, successor));
        }
    }
}

/// A cursor belongs to one live provider lease, not merely the same Map storage root.
fn map_reference(
    state: &State,
    source: LoanId,
    cursor: Option<ValueId>,
) -> Result<Reference, VerifyErrorKind> {
    let reference = state
        .loans
        .get(&source)
        .filter(|reference| reference.live)
        .ok_or(VerifyErrorKind::LoanInactive { loan: source })?;
    if let Some(cursor) = cursor {
        let cursor = state
            .cursors
            .get(&cursor)
            .ok_or(VerifyErrorKind::OperationContract {
                reason: "Map cursor must originate from First/Next and explicit CFG transport",
            })?;
        if !cursor.live || cursor.lineage != reference.lineage {
            return Err(VerifyErrorKind::OperationContract {
                reason: "Map cursor and source must belong to the same live provider lease",
            });
        }
    }
    Ok(reference.clone())
}
