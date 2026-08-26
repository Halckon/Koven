use std::collections::{BTreeMap, BTreeSet};

mod closure;

use super::{
    model::{
        BlockId, Definition, Edge, EntityId, EntityType, Function, LoanId, LoanKind, Module,
        Operation, PlaceAccess, PlaceId, SsaTypeId, TerminatorKind, ValueId,
    },
    verify::{VerifyError, VerifyErrorKind, VerifyLocation},
};

#[derive(Clone, Default)]
struct BlockState {
    values: BTreeSet<ValueId>,
    places: BTreeSet<PlaceId>,
    loans: BTreeSet<LoanId>,
}

struct AliasRoots {
    roots: BTreeMap<EntityId, BTreeSet<EntityId>>,
}

pub(super) fn verify_ownership(
    module: &Module,
    function: &Function,
    errors: &mut Vec<VerifyError>,
) {
    let aliases = AliasRoots::compute(function);
    let closure_loans = closure::ClosureLoans::compute(function);
    for block in &function.blocks {
        let mut state = entry_state(module, function, block.id, &closure_loans);
        for instruction_id in &block.instructions {
            let instruction = function
                .instruction(*instruction_id)
                .expect("structural verifier proved instruction existence");
            verify_linear_live_ins(
                module,
                function,
                block.id,
                instruction.operation.entities(),
                VerifyLocation::Instruction(instruction.id),
                &instruction.origin,
                errors,
            );
            apply_operation(
                module,
                function,
                instruction,
                &aliases,
                &closure_loans,
                &mut state,
                errors,
            );
            register_results(module, function, instruction, &mut state);
        }

        let terminator = block
            .terminator
            .as_ref()
            .expect("structural verifier proved terminator existence");
        verify_linear_live_ins(
            module,
            function,
            block.id,
            terminator.kind.entities(),
            VerifyLocation::Terminator(block.id),
            &terminator.origin,
            errors,
        );
        match &terminator.kind {
            TerminatorKind::Branch(edge) => verify_edge_state(
                module,
                function,
                block.id,
                0,
                edge,
                &aliases,
                &closure_loans,
                state,
                &terminator.origin,
                errors,
            ),
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => {
                verify_edge_state(
                    module,
                    function,
                    block.id,
                    0,
                    when_true,
                    &aliases,
                    &closure_loans,
                    state.clone(),
                    &terminator.origin,
                    errors,
                );
                verify_edge_state(
                    module,
                    function,
                    block.id,
                    1,
                    when_false,
                    &aliases,
                    &closure_loans,
                    state,
                    &terminator.origin,
                    errors,
                );
            }
            TerminatorKind::NullableBranch {
                owner,
                when_null,
                when_non_null,
                ..
            } => {
                require_value(
                    module,
                    function,
                    *owner,
                    &state,
                    VerifyLocation::Terminator(block.id),
                    &terminator.origin,
                    errors,
                );
                if has_exclusive_value_loan(function, *owner, &aliases, &state) {
                    errors.push(error(
                        VerifyErrorKind::OwnerLoanConflict { value: *owner },
                        VerifyLocation::Terminator(block.id),
                        &terminator.origin,
                    ));
                }
                verify_edge_state(
                    module,
                    function,
                    block.id,
                    0,
                    when_null,
                    &aliases,
                    &closure_loans,
                    state.clone(),
                    &terminator.origin,
                    errors,
                );
                verify_edge_state(
                    module,
                    function,
                    block.id,
                    1,
                    when_non_null,
                    &aliases,
                    &closure_loans,
                    state,
                    &terminator.origin,
                    errors,
                );
            }
            TerminatorKind::Return { values } => {
                for value in values {
                    consume_value(
                        module,
                        function,
                        *value,
                        &aliases,
                        &mut state,
                        &BTreeSet::new(),
                        &BTreeSet::new(),
                        VerifyLocation::Terminator(block.id),
                        &terminator.origin,
                        errors,
                    );
                }
                release_borrow_parameters(function, &aliases, &mut state);
                verify_normal_exit(
                    state,
                    VerifyLocation::Terminator(block.id),
                    &terminator.origin,
                    errors,
                );
            }
            TerminatorKind::Abort => {}
        }
    }
}

fn release_borrow_parameters(function: &Function, aliases: &AliasRoots, state: &mut BlockState) {
    let Some(entry) = function.blocks.first() else {
        return;
    };
    let parameters = entry
        .parameters
        .iter()
        .copied()
        .filter(|entity| matches!(entity, EntityId::Loan(_)))
        .collect::<BTreeSet<_>>();
    state.loans.retain(|loan| {
        !aliases
            .roots
            .get(&EntityId::Loan(*loan))
            .is_some_and(|roots| roots.iter().any(|root| parameters.contains(root)))
    });
}

fn entry_state(
    module: &Module,
    function: &Function,
    block: BlockId,
    closure_loans: &closure::ClosureLoans,
) -> BlockState {
    let mut state = BlockState::default();
    if let Some(entry) = function.blocks.first() {
        state
            .loans
            .extend(entry.parameters.iter().filter_map(|entity| {
                let EntityId::Loan(loan) = entity else {
                    return None;
                };
                Some(*loan)
            }));
    }
    for entity in &function.block(block).expect("block must exist").parameters {
        match entity {
            EntityId::Value(value) if is_move_only(module, function, *value) => {
                state.values.insert(*value);
                closure_loans.activate_entry(*value, &mut state);
            }
            EntityId::Place(place) => {
                state.places.insert(*place);
            }
            EntityId::Loan(loan) => {
                state.loans.insert(*loan);
            }
            EntityId::Value(_) => {}
        }
    }
    state
}

fn register_results(
    module: &Module,
    function: &Function,
    instruction: &super::model::Instruction,
    state: &mut BlockState,
) {
    for entity in &instruction.results {
        match entity {
            EntityId::Value(value) if is_move_only(module, function, *value) => {
                state.values.insert(*value);
            }
            EntityId::Place(place) => {
                state.places.insert(*place);
            }
            EntityId::Loan(loan) => {
                state.loans.insert(*loan);
            }
            EntityId::Value(_) => {}
        }
    }
}

fn apply_operation(
    module: &Module,
    function: &Function,
    instruction: &super::model::Instruction,
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    state: &mut BlockState,
    errors: &mut Vec<VerifyError>,
) {
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    match &instruction.operation {
        Operation::Constant(_)
        | Operation::PrintLiteral { .. }
        | Operation::Binary { .. }
        | Operation::CheckedArithmetic { .. }
        | Operation::Compare { .. }
        | Operation::BooleanNot { .. } => {}
        Operation::DirectCall { arguments, .. } => {
            for argument in arguments {
                match argument {
                    EntityId::Value(value) => {
                        consume_value(
                            module,
                            function,
                            *value,
                            aliases,
                            state,
                            &BTreeSet::new(),
                            &BTreeSet::new(),
                            location.clone(),
                            origin,
                            errors,
                        );
                    }
                    EntityId::Loan(loan) => {
                        if !state.loans.contains(loan) {
                            errors.push(error(
                                VerifyErrorKind::LoanInactive { loan: *loan },
                                location.clone(),
                                origin,
                            ));
                        }
                    }
                    EntityId::Place(place) => {
                        require_place(*place, state, location.clone(), origin, errors);
                    }
                }
            }
        }
        Operation::FunctionAddress { .. } => {}
        Operation::ClosureConstruct { captures, .. } => {
            closure::apply_construct(
                module, function, captures, aliases, state, location, origin, errors,
            );
        }
        Operation::CallableInvoke {
            callable,
            arguments,
        } => {
            closure::apply_invoke(
                module, function, *callable, arguments, aliases, state, location, origin, errors,
            );
        }
        Operation::AggregateConstruct { fields, .. } => {
            for field in fields {
                consume_value(
                    module,
                    function,
                    *field,
                    aliases,
                    state,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    location.clone(),
                    origin,
                    errors,
                );
            }
        }
        Operation::AggregateProject { aggregate, .. } => {
            if require_value(
                module,
                function,
                *aggregate,
                state,
                location.clone(),
                origin,
                errors,
            ) && has_exclusive_value_loan(function, *aggregate, aliases, state)
            {
                errors.push(error(
                    VerifyErrorKind::OwnerLoanConflict { value: *aggregate },
                    location,
                    origin,
                ));
            }
        }
        Operation::AggregateExplode { aggregate } => {
            consume_value(
                module,
                function,
                *aggregate,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::AggregateCopyExplode { aggregate } => {
            require_value(
                module, function, *aggregate, state, location, origin, errors,
            );
        }
        Operation::TaggedConstruct { payload, .. } => {
            consume_value(
                module,
                function,
                *payload,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::TaggedPayloadPlace { owner, .. } => {
            require_value(module, function, *owner, state, location, origin, errors);
        }
        Operation::TaggedDiscriminant { owner } => {
            require_value(module, function, *owner, state, location, origin, errors);
        }
        Operation::HeapAllocate { payload, .. } => {
            consume_value(
                module,
                function,
                *payload,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::SharedAllocate { payload, .. } => {
            consume_value(
                module,
                function,
                *payload,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::SharedRetain { owner } | Operation::SharedPayloadPlace { owner } => {
            match owner {
                EntityId::Value(owner) => {
                    require_value(module, function, *owner, state, location, origin, errors);
                }
                EntityId::Loan(loan) => {
                    if !state.loans.contains(loan) {
                        errors.push(error(
                            VerifyErrorKind::LoanInactive { loan: *loan },
                            location,
                            origin,
                        ));
                    }
                }
                EntityId::Place(_) => unreachable!("operation contract rejects place owners"),
            }
        }
        Operation::NullableWrap { owner, .. } => {
            consume_value(
                module,
                function,
                *owner,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::NullableNull { .. } => {}
        Operation::NullableIsNull { owner } => {
            require_value(module, function, *owner, state, location, origin, errors);
        }
        Operation::NullableTake { owner, proof } => {
            if !aliases.overlap(EntityId::Value(*owner), EntityId::Loan(*proof)) {
                errors.push(error(
                    VerifyErrorKind::NullableProofMismatch {
                        owner: *owner,
                        proof: *proof,
                    },
                    location.clone(),
                    origin,
                ));
            }
            if !state.loans.remove(proof) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan: *proof },
                    location.clone(),
                    origin,
                ));
            }
            consume_value(
                module,
                function,
                *owner,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::HeapPayloadPlace { owner } => {
            require_value(module, function, *owner, state, location, origin, errors);
        }
        Operation::ContainerConstruct { elements, .. } => {
            for element in elements {
                consume_value(
                    module,
                    function,
                    *element,
                    aliases,
                    state,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    location.clone(),
                    origin,
                    errors,
                );
            }
        }
        Operation::ContainerGenerate { length, .. } => {
            require_value(module, function, *length, state, location, origin, errors);
        }
        Operation::ContainerLength { owner } | Operation::ContainerElementPlace { owner, .. } => {
            if require_value(
                module,
                function,
                *owner,
                state,
                location.clone(),
                origin,
                errors,
            ) && has_exclusive_value_loan(function, *owner, aliases, state)
            {
                errors.push(error(
                    VerifyErrorKind::OwnerLoanConflict { value: *owner },
                    location,
                    origin,
                ));
            }
        }
        Operation::ContainerReplace { owner, value, .. } => {
            if require_value(
                module,
                function,
                *owner,
                state,
                location.clone(),
                origin,
                errors,
            ) && has_any_value_loan(*owner, aliases, state)
            {
                errors.push(error(
                    VerifyErrorKind::OwnerLoanConflict { value: *owner },
                    location.clone(),
                    origin,
                ));
            }
            consume_value(
                module,
                function,
                *value,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::FieldPlace { base, .. } => {
            require_place(*base, state, location, origin, errors);
        }
        Operation::Copy { source } => {
            if is_move_only(module, function, *source) {
                errors.push(error(
                    VerifyErrorKind::CopyMoveOnly { value: *source },
                    location,
                    origin,
                ));
            }
        }
        Operation::Consume { owner } => {
            consume_value(
                module,
                function,
                *owner,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::RootPlace { owner } => {
            require_value(module, function, *owner, state, location, origin, errors);
        }
        Operation::BorrowBegin { place, kind } => {
            if require_place(*place, state, location.clone(), origin, errors)
                && has_borrow_conflict(function, *place, *kind, aliases, state)
            {
                errors.push(error(
                    VerifyErrorKind::BorrowConflict { place: *place },
                    location,
                    origin,
                ));
            }
        }
        Operation::BorrowEnd { loan } => {
            if let Some(owner) = closure_loans.live_owner_holding(*loan, state) {
                errors.push(error(
                    VerifyErrorKind::OwnerLoanConflict { value: owner },
                    location,
                    origin,
                ));
            } else if !state.loans.remove(loan) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan: *loan },
                    location,
                    origin,
                ));
            }
        }
        Operation::Read { source } => {
            let entity = match source {
                PlaceAccess::Place(place) => {
                    if require_place(*place, state, location.clone(), origin, errors)
                        && has_exclusive_loan(function, *place, aliases, state)
                    {
                        errors.push(error(
                            VerifyErrorKind::BorrowConflict { place: *place },
                            location.clone(),
                            origin,
                        ));
                    }
                    EntityId::Place(*place)
                }
                PlaceAccess::Loan(loan) => {
                    if !state.loans.contains(loan) {
                        errors.push(error(
                            VerifyErrorKind::LoanInactive { loan: *loan },
                            location.clone(),
                            origin,
                        ));
                    }
                    EntityId::Loan(*loan)
                }
            };
            if type_is_move_only(
                module,
                function
                    .entity(entity)
                    .expect("entity exists")
                    .ty
                    .semantic_type(),
            ) {
                errors.push(error(
                    VerifyErrorKind::MoveOnlyPlaceRead { entity },
                    location,
                    origin,
                ));
            }
        }
        Operation::Mutate { place, value } => {
            if require_place(*place, state, location.clone(), origin, errors)
                && has_any_loan(*place, aliases, state)
            {
                errors.push(error(
                    VerifyErrorKind::MutationConflict { place: *place },
                    location.clone(),
                    origin,
                ));
            }
            consume_value(
                module,
                function,
                *value,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::Drop { owner } => {
            if is_move_only(module, function, *owner) {
                let consumed = consume_value(
                    module,
                    function,
                    *owner,
                    aliases,
                    state,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    location,
                    origin,
                    errors,
                );
                if consumed {
                    closure_loans.release(*owner, state);
                }
            } else {
                errors.push(error(
                    VerifyErrorKind::DropCopyable { value: *owner },
                    location,
                    origin,
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn verify_edge_state(
    module: &Module,
    function: &Function,
    source: BlockId,
    successor: usize,
    edge: &Edge,
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    mut state: BlockState,
    origin: &super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    let location = VerifyLocation::Edge { source, successor };
    let mut transferring_loans = edge
        .arguments
        .iter()
        .filter_map(|entity| match entity {
            EntityId::Loan(loan) => Some(*loan),
            EntityId::Value(_) | EntityId::Place(_) => None,
        })
        .collect::<BTreeSet<_>>();
    for argument in &edge.arguments {
        if let EntityId::Value(value) = argument {
            transferring_loans.extend(closure_loans.dependencies(*value));
        }
    }
    let transferring_places = edge
        .arguments
        .iter()
        .filter_map(|entity| match entity {
            EntityId::Place(place) => Some(*place),
            EntityId::Value(_) | EntityId::Loan(_) => None,
        })
        .collect::<BTreeSet<_>>();
    for loan in edge.arguments.iter().filter_map(|entity| match entity {
        EntityId::Loan(loan) => Some(*loan),
        _ => None,
    }) {
        if !state.loans.remove(&loan) {
            errors.push(error(
                VerifyErrorKind::LoanInactive { loan },
                location.clone(),
                origin,
            ));
        }
    }
    for argument in &edge.arguments {
        match argument {
            EntityId::Value(value) => {
                if consume_value(
                    module,
                    function,
                    *value,
                    aliases,
                    &mut state,
                    &transferring_loans,
                    &transferring_places,
                    location.clone(),
                    origin,
                    errors,
                ) {
                    closure_loans.release(*value, &mut state);
                }
            }
            EntityId::Place(place) => {
                if !state.places.remove(place) {
                    errors.push(error(
                        VerifyErrorKind::PlaceUnavailable { place: *place },
                        location.clone(),
                        origin,
                    ));
                }
            }
            EntityId::Loan(_) => {}
        }
    }
    release_borrow_parameters(function, aliases, &mut state);
    verify_normal_exit(state, location, origin, errors);
}

#[allow(clippy::too_many_arguments)]
fn consume_value(
    module: &Module,
    function: &Function,
    value: ValueId,
    aliases: &AliasRoots,
    state: &mut BlockState,
    allowed_loans: &BTreeSet<LoanId>,
    allowed_places: &BTreeSet<PlaceId>,
    location: VerifyLocation,
    origin: &super::model::Origin,
    errors: &mut Vec<VerifyError>,
) -> bool {
    if !is_move_only(module, function, value) {
        return true;
    }
    if !state.values.contains(&value) {
        errors.push(error(
            VerifyErrorKind::ValueUnavailable { value },
            location,
            origin,
        ));
        return false;
    }
    if state.loans.iter().any(|loan| {
        !allowed_loans.contains(loan)
            && aliases.overlap(EntityId::Value(value), EntityId::Loan(*loan))
    }) {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value },
            location,
            origin,
        ));
        return false;
    }
    state.values.remove(&value);
    let invalidated_places = state
        .places
        .iter()
        .copied()
        .filter(|place| {
            !allowed_places.contains(place)
                && aliases.overlap(EntityId::Value(value), EntityId::Place(*place))
        })
        .collect::<Vec<_>>();
    for place in invalidated_places {
        state.places.remove(&place);
    }
    true
}

fn require_value(
    module: &Module,
    function: &Function,
    value: ValueId,
    state: &BlockState,
    location: VerifyLocation,
    origin: &super::model::Origin,
    errors: &mut Vec<VerifyError>,
) -> bool {
    if !is_move_only(module, function, value) || state.values.contains(&value) {
        true
    } else {
        errors.push(error(
            VerifyErrorKind::ValueUnavailable { value },
            location,
            origin,
        ));
        false
    }
}

fn require_place(
    place: PlaceId,
    state: &BlockState,
    location: VerifyLocation,
    origin: &super::model::Origin,
    errors: &mut Vec<VerifyError>,
) -> bool {
    if state.places.contains(&place) {
        true
    } else {
        errors.push(error(
            VerifyErrorKind::PlaceUnavailable { place },
            location,
            origin,
        ));
        false
    }
}

fn verify_normal_exit(
    state: BlockState,
    location: VerifyLocation,
    origin: &super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    for value in state.values {
        errors.push(error(
            VerifyErrorKind::MissingOwnedExit { value },
            location.clone(),
            origin,
        ));
    }
    for loan in state.loans {
        errors.push(error(
            VerifyErrorKind::ActiveLoanAtExit { loan },
            location.clone(),
            origin,
        ));
    }
}

fn verify_linear_live_ins(
    module: &Module,
    function: &Function,
    use_block: BlockId,
    entities: Vec<EntityId>,
    location: VerifyLocation,
    origin: &super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    for entity in entities {
        let linear = match entity {
            EntityId::Value(value) => is_move_only(module, function, value),
            EntityId::Place(_) | EntityId::Loan(_) => true,
        };
        let entry_loan_parameter = matches!(entity, EntityId::Loan(_))
            && function
                .blocks
                .first()
                .is_some_and(|entry| entry.parameters.contains(&entity));
        if linear && !entry_loan_parameter && definition_block(function, entity) != use_block {
            errors.push(error(
                VerifyErrorKind::HiddenLinearLiveIn { entity },
                location.clone(),
                origin,
            ));
        }
    }
}

fn definition_block(function: &Function, entity: EntityId) -> BlockId {
    match function
        .entity(entity)
        .expect("entity must exist")
        .definition
    {
        Definition::BlockParameter { block, .. } => block,
        Definition::InstructionResult { instruction, .. } => {
            function
                .instruction(instruction)
                .expect("definition instruction must exist")
                .block
        }
    }
}

fn has_borrow_conflict(
    function: &Function,
    place: PlaceId,
    requested: LoanKind,
    aliases: &AliasRoots,
    state: &BlockState,
) -> bool {
    state.loans.iter().any(|loan| {
        aliases.overlap(EntityId::Place(place), EntityId::Loan(*loan))
            && (requested == LoanKind::Exclusive
                || loan_kind(function, *loan) == LoanKind::Exclusive)
    })
}

fn has_exclusive_loan(
    function: &Function,
    place: PlaceId,
    aliases: &AliasRoots,
    state: &BlockState,
) -> bool {
    state.loans.iter().any(|loan| {
        loan_kind(function, *loan) == LoanKind::Exclusive
            && aliases.overlap(EntityId::Place(place), EntityId::Loan(*loan))
    })
}

fn has_exclusive_value_loan(
    function: &Function,
    value: ValueId,
    aliases: &AliasRoots,
    state: &BlockState,
) -> bool {
    state.loans.iter().any(|loan| {
        loan_kind(function, *loan) == LoanKind::Exclusive
            && aliases.overlap(EntityId::Value(value), EntityId::Loan(*loan))
    })
}

fn has_any_loan(place: PlaceId, aliases: &AliasRoots, state: &BlockState) -> bool {
    state
        .loans
        .iter()
        .any(|loan| aliases.overlap(EntityId::Place(place), EntityId::Loan(*loan)))
}

fn has_any_value_loan(value: ValueId, aliases: &AliasRoots, state: &BlockState) -> bool {
    state
        .loans
        .iter()
        .any(|loan| aliases.overlap(EntityId::Value(value), EntityId::Loan(*loan)))
}

fn loan_kind(function: &Function, loan: LoanId) -> LoanKind {
    let EntityType::Loan { kind, .. } = function
        .entity(EntityId::Loan(loan))
        .expect("loan must exist")
        .ty
    else {
        unreachable!("structural verifier proved loan entity type");
    };
    kind
}

fn is_move_only(module: &Module, function: &Function, value: ValueId) -> bool {
    type_is_move_only(
        module,
        function
            .entity(EntityId::Value(value))
            .expect("value must exist")
            .ty
            .semantic_type(),
    )
}

fn type_is_move_only(module: &Module, ty: SsaTypeId) -> bool {
    module.type_ownership(ty) == Some(super::model::Ownership::MoveOnly)
}

fn error(
    kind: VerifyErrorKind,
    location: VerifyLocation,
    origin: &super::model::Origin,
) -> VerifyError {
    VerifyError {
        kind,
        location,
        origin: Some(origin.clone()),
    }
}

impl AliasRoots {
    fn compute(function: &Function) -> Self {
        let mut roots = all_entities(function)
            .into_iter()
            .map(|entity| (entity, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        let mut incoming = BTreeMap::<EntityId, usize>::new();
        for block in &function.blocks {
            let terminator = block.terminator.as_ref().expect("terminator must exist");
            for edge in edges(&terminator.kind) {
                let target = function.block(edge.target).expect("target must exist");
                for parameter in &target.parameters {
                    *incoming.entry(*parameter).or_default() += 1;
                }
            }
        }
        for block in &function.blocks {
            for parameter in &block.parameters {
                if block.id.index() == 0 || incoming.get(parameter).copied().unwrap_or(0) == 0 {
                    roots
                        .get_mut(parameter)
                        .expect("parameter root exists")
                        .insert(*parameter);
                }
            }
        }
        for instruction in &function.instructions {
            for result in &instruction.results {
                if matches!(result, EntityId::Value(_)) {
                    roots
                        .get_mut(result)
                        .expect("result root exists")
                        .insert(*result);
                }
            }
        }

        loop {
            let mut changed = false;
            for block in &function.blocks {
                let terminator = block.terminator.as_ref().expect("terminator must exist");
                for edge in edges(&terminator.kind) {
                    let target = function.block(edge.target).expect("target must exist");
                    for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                        changed |= union_from(&mut roots, *parameter, *argument);
                    }
                }
            }
            for instruction in &function.instructions {
                match &instruction.operation {
                    Operation::RootPlace { owner } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Value(*owner));
                    }
                    Operation::BorrowBegin { place, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Place(*place));
                    }
                    Operation::HeapPayloadPlace { owner } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Value(*owner));
                    }
                    Operation::SharedPayloadPlace { owner } => {
                        changed |= union_from(&mut roots, instruction.results[0], *owner);
                    }
                    Operation::ContainerElementPlace { owner, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Value(*owner));
                    }
                    Operation::FieldPlace { base, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Place(*base));
                    }
                    _ => {}
                }
            }
            for block in &function.blocks {
                let Some(terminator) = &block.terminator else {
                    continue;
                };
                if let TerminatorKind::NullableBranch { owner, view, .. } = terminator.kind {
                    changed |= union_from(&mut roots, EntityId::Loan(view), EntityId::Value(owner));
                }
            }
            if !changed {
                break;
            }
        }

        Self { roots }
    }

    fn overlap(&self, left: EntityId, right: EntityId) -> bool {
        let left = self.roots.get(&left).expect("left alias roots must exist");
        let right = self
            .roots
            .get(&right)
            .expect("right alias roots must exist");
        left.iter().any(|root| right.contains(root))
    }
}

fn union_from(
    roots: &mut BTreeMap<EntityId, BTreeSet<EntityId>>,
    target: EntityId,
    source: EntityId,
) -> bool {
    let source = roots.get(&source).expect("source roots must exist").clone();
    let target = roots.get_mut(&target).expect("target roots must exist");
    let before = target.len();
    target.extend(source);
    target.len() != before
}

fn all_entities(function: &Function) -> Vec<EntityId> {
    let values = (0..function.values.len()).map(|index| {
        EntityId::Value(ValueId {
            function: function.id,
            index,
        })
    });
    let places = (0..function.places.len()).map(|index| {
        EntityId::Place(PlaceId {
            function: function.id,
            index,
        })
    });
    let loans = (0..function.loans.len()).map(|index| {
        EntityId::Loan(LoanId {
            function: function.id,
            index,
        })
    });
    values.chain(places).chain(loans).collect()
}

fn edges(terminator: &TerminatorKind) -> Vec<&Edge> {
    match terminator {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        TerminatorKind::Return { .. } | TerminatorKind::Abort => Vec::new(),
    }
}
