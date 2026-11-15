#[path = "verify_ownership/operation.rs"]
mod operation;
use operation::apply_operation;

use std::collections::{BTreeMap, BTreeSet};

pub(in crate::ssa) mod aliases;
use aliases::{
    direct_fields,
    map::{self, reborrow},
    provider_lifetime,
};
mod closure;
pub(in crate::ssa) mod field_exchange;
#[cfg(test)]
pub(in crate::ssa) mod field_exchange_tests;
mod range;
use range::borrow_result;
#[cfg(test)]
mod borrow_return_transport_tests;
pub(in crate::ssa) mod root_exchange;

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

pub(super) struct AliasRoots {
    roots: BTreeMap<EntityId, BTreeSet<EntityId>>,
    pub(in crate::ssa) direct_fields: direct_fields::DirectFields,
}

struct ReborrowDependencies {
    parent_by_child: BTreeMap<LoanId, LoanId>,
    must_end_children: BTreeSet<LoanId>,
    flows: LoanFlowAliases,
}

struct LoanFlowAliases {
    origins: BTreeMap<LoanId, BTreeSet<LoanId>>,
}

pub(super) fn verify_ownership(
    module: &Module,
    function: &Function,
    capture_types: &super::verify::TypeCaptures,
    trusted_view_fields: Option<&BTreeSet<usize>>,
    errors: &mut Vec<VerifyError>,
) {
    provider_lifetime::verify(function, errors);
    super::verify_operation::range_verify_signature(module, function, errors);
    let aliases = AliasRoots::compute(function);
    let closure_loans = closure::ClosureLoans::compute(
        module,
        function,
        capture_types,
        &aliases,
        trusted_view_fields,
    );
    let reborrows = ReborrowDependencies::compute(module, function);
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
                &reborrows,
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
                &reborrows,
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
                    &reborrows,
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
                    &reborrows,
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
                    &reborrows,
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
                    &reborrows,
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
                release_borrow_parameters(function, &aliases, &reborrows, &mut state);
                verify_normal_exit(
                    state,
                    VerifyLocation::Terminator(block.id),
                    &terminator.origin,
                    errors,
                );
            }
            TerminatorKind::BorrowReturn { loan } => {
                borrow_result::verify_return(
                    function,
                    *loan,
                    &aliases,
                    &reborrows,
                    &mut state,
                    VerifyLocation::Terminator(block.id),
                    &terminator.origin,
                    errors,
                );
                release_borrow_parameters(function, &aliases, &reborrows, &mut state);
                verify_normal_exit(
                    state,
                    VerifyLocation::Terminator(block.id),
                    &terminator.origin,
                    errors,
                );
            }
            TerminatorKind::RangeReturn { view, source } => {
                range::verify_return(
                    module,
                    function,
                    *view,
                    *source,
                    &aliases,
                    &reborrows,
                    &mut state,
                    VerifyLocation::Terminator(block.id),
                    &terminator.origin,
                    errors,
                );
                release_borrow_parameters(function, &aliases, &reborrows, &mut state);
                verify_normal_exit(
                    state,
                    VerifyLocation::Terminator(block.id),
                    &terminator.origin,
                    errors,
                );
            }
            TerminatorKind::Abort | TerminatorKind::Unreachable => {}
        }
    }
}

fn release_borrow_parameters(
    function: &Function,
    aliases: &AliasRoots,
    reborrows: &ReborrowDependencies,
    state: &mut BlockState,
) {
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
        if reborrows.is_derived(*loan, aliases) {
            return true;
        }
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


#[allow(clippy::too_many_arguments)]
fn require_string_view(
    module: &Module,
    function: &Function,
    operand: EntityId,
    aliases: &AliasRoots,
    state: &BlockState,
    location: VerifyLocation,
    origin: &super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    match operand {
        EntityId::Value(value) => {
            if require_value(
                module,
                function,
                value,
                state,
                location.clone(),
                origin,
                errors,
            ) && has_exclusive_value_loan(function, value, aliases, state)
            {
                errors.push(error(
                    VerifyErrorKind::OwnerLoanConflict { value },
                    location,
                    origin,
                ));
            }
        }
        EntityId::Loan(loan) => {
            if !state.loans.contains(&loan) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan },
                    location,
                    origin,
                ));
            }
        }
        EntityId::Place(_) => unreachable!("operation contract rejects String place views"),
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
    reborrows: &ReborrowDependencies,
    mut state: BlockState,
    origin: &super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    let location = VerifyLocation::Edge { source, successor };
    let transferring_loans = closure_loans.edge_loans(function, edge, &location, origin, errors);
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
    release_borrow_parameters(function, aliases, reborrows, &mut state);
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

#[allow(clippy::too_many_arguments)]
fn check_and_consume_container_owner(
    module: &Module,
    function: &Function,
    owner: ValueId,
    aliases: &AliasRoots,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    if require_value(
        module,
        function,
        owner,
        state,
        location.clone(),
        origin,
        errors,
    ) && has_any_value_loan(owner, aliases, state)
    {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value: owner },
            location.clone(),
            origin,
        ));
    }
    consume_value(
        module,
        function,
        owner,
        aliases,
        state,
        &BTreeSet::new(),
        &BTreeSet::new(),
        location,
        origin,
        errors,
    );
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
            && !aliases
                .direct_fields
                .disjoint(EntityId::Place(place), EntityId::Loan(*loan))
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

impl LoanFlowAliases {
    fn compute(function: &Function) -> Self {
        let mut origins = all_entities(function)
            .into_iter()
            .filter_map(|entity| match entity {
                EntityId::Loan(loan) => Some((loan, BTreeSet::new())),
                EntityId::Value(_) | EntityId::Place(_) => None,
            })
            .collect::<BTreeMap<_, _>>();
        let mut incoming = BTreeMap::<LoanId, usize>::new();
        for block in &function.blocks {
            let terminator = block.terminator.as_ref().expect("terminator must exist");
            for edge in edges(&terminator.kind) {
                let target = function.block(edge.target).expect("target must exist");
                for parameter in &target.parameters {
                    if let EntityId::Loan(loan) = parameter {
                        *incoming.entry(*loan).or_default() += 1;
                    }
                }
            }
        }
        for block in &function.blocks {
            for parameter in &block.parameters {
                let EntityId::Loan(loan) = parameter else {
                    continue;
                };
                if block.id.index() == 0 || incoming.get(loan).copied().unwrap_or(0) == 0 {
                    origins
                        .get_mut(loan)
                        .expect("loan parameter exists")
                        .insert(*loan);
                }
            }
        }
        for instruction in &function.instructions {
            for result in &instruction.results {
                if let EntityId::Loan(loan) = result {
                    origins
                        .get_mut(loan)
                        .expect("loan result exists")
                        .insert(*loan);
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
                        let (EntityId::Loan(argument), EntityId::Loan(parameter)) =
                            (argument, parameter)
                        else {
                            continue;
                        };
                        let source = origins
                            .get(argument)
                            .expect("loan edge argument exists")
                            .clone();
                        let target = origins.get_mut(parameter).expect("loan parameter exists");
                        let before = target.len();
                        target.extend(source);
                        changed |= target.len() != before;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        Self { origins }
    }

    fn equivalent(&self, loan: LoanId, origin: LoanId) -> bool {
        if loan == origin {
            return true;
        }
        let Some(left) = self.origins.get(&loan) else {
            return false;
        };
        let Some(right) = self.origins.get(&origin) else {
            return false;
        };
        left.iter().any(|identity| right.contains(identity))
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

pub(super) fn all_entities(function: &Function) -> Vec<EntityId> {
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

pub(super) fn edges(terminator: &TerminatorKind) -> Vec<&Edge> {
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
        TerminatorKind::Return { .. }
        | TerminatorKind::RangeReturn { .. }
        | TerminatorKind::BorrowReturn { .. }
        | TerminatorKind::Abort
        | TerminatorKind::Unreachable => Vec::new(),
    }
}
