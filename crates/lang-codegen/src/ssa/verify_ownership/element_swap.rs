//! Verify projected storage access and independent mutable swap places.
use super::*;
use crate::ssa::model::{Instruction, SequentialContainerKind};

pub(super) fn projection_provider(function: &Function, place: PlaceId) -> Option<LoanId> {
    let Definition::InstructionResult { instruction, .. } =
        function.entity(EntityId::Place(place))?.definition
    else {
        return None;
    };
    match function.instruction(instruction)?.operation {
        Operation::ContainerElementPlace {
            owner: EntityId::Loan(provider),
            ..
        } => Some(provider),
        _ => None,
    }
}

pub(super) fn check_provider(
    function: &Function,
    loan: LoanId,
    reborrows: &ReborrowDependencies,
    state: &BlockState,
    instruction: &Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let kind = if !state.loans.contains(&loan) {
        VerifyErrorKind::LoanInactive { loan }
    } else if loan_kind(function, loan) == LoanKind::Exclusive
        && !inout::is_parameter_loan(function, reborrows, loan)
    {
        VerifyErrorKind::OperationContract {
            reason: "exclusive element projection requires an Inout parameter",
        }
    } else {
        return;
    };
    errors.push(error(
        kind,
        VerifyLocation::Instruction(instruction.id),
        &instruction.origin,
    ));
}

pub(super) fn check_borrow(
    function: &Function,
    (place, kind): (PlaceId, LoanKind),
    aliases: &AliasRoots,
    reborrows: &ReborrowDependencies,
    state: &BlockState,
    instruction: &Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    if require_place(place, state, location.clone(), origin, errors)
        && borrow_conflicts(function, place, kind, aliases, reborrows, state)
    {
        errors.push(error(
            VerifyErrorKind::BorrowConflict { place },
            location,
            origin,
        ));
    }
}

fn borrow_conflicts(
    function: &Function,
    place: PlaceId,
    requested: LoanKind,
    aliases: &AliasRoots,
    reborrows: &ReborrowDependencies,
    state: &BlockState,
) -> bool {
    let provider = projection_provider(function, place);
    if provider.is_some_and(|loan| {
        !state.loans.contains(&loan)
            || (requested == LoanKind::Exclusive
                && loan_kind(function, loan) != LoanKind::Exclusive)
    }) {
        return true;
    }
    state.loans.iter().any(|loan| {
        !provider.is_some_and(|provider| reborrows.same_origin(provider, *loan))
            && aliases.overlap(EntityId::Place(place), EntityId::Loan(*loan))
            && !aliases
                .direct_fields
                .disjoint(EntityId::Place(place), EntityId::Loan(*loan))
            && (requested == LoanKind::Exclusive
                || loan_kind(function, *loan) == LoanKind::Exclusive)
    })
}

pub(super) fn projection_conflicts(
    function: &Function,
    owner: ValueId,
    instruction: &Instruction,
    aliases: &AliasRoots,
    state: &BlockState,
) -> bool {
    let [place] = instruction.results.as_slice() else {
        unreachable!("element projection contract requires one place")
    };
    state.loans.iter().copied().any(|loan| {
        loan_kind(function, loan) == LoanKind::Exclusive
            && aliases.overlap(EntityId::Value(owner), EntityId::Loan(loan))
            && !aliases.direct_fields.disjoint(*place, EntityId::Loan(loan))
    })
}

/// The parent capability is suspended while its whole storage is reborrowed.
pub(super) fn suspended_root_use(
    function: &Function,
    dependencies: &ReborrowDependencies,
    state: &BlockState,
    parent: LoanId,
) -> Option<VerifyErrorKind> {
    if !dependencies.has_exclusive_root_reborrow {
        return None;
    }
    dependencies
        .active_descendant_among(
            parent,
            state.loans.iter().copied().filter(|loan| {
                dependencies.flows.origins.get(loan).is_some_and(|origins| {
                    origins
                        .iter()
                        .any(|origin| is_root_reborrow(function, *origin))
                })
            }),
        )
        .map(|dependent| VerifyErrorKind::LoanDependencyActive { parent, dependent })
}

pub(super) fn check_suspended_root_use(
    function: &Function,
    dependencies: &ReborrowDependencies,
    state: &BlockState,
    instruction: &Instruction,
    errors: &mut Vec<VerifyError>,
) -> bool {
    if !dependencies.has_exclusive_root_reborrow {
        return true;
    }
    for entity in instruction.operation.entities() {
        if let EntityId::Loan(parent) = entity
            && let Some(kind) = suspended_root_use(function, dependencies, state, parent)
        {
            errors.push(error(
                kind,
                VerifyLocation::Instruction(instruction.id),
                &instruction.origin,
            ));
            return false;
        }
    }
    true
}

fn check_root_reborrow(
    function: &Function,
    parent: LoanId,
    aliases: &AliasRoots,
    dependencies: &ReborrowDependencies,
    state: &BlockState,
    instruction: &Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let kind = if !state.loans.contains(&parent) {
        VerifyErrorKind::LoanInactive { loan: parent }
    } else if !inout::is_parameter_loan(function, dependencies, parent)
        || dependencies
            .flows
            .origins
            .get(&parent)
            .is_none_or(|origins| origins.len() != 1)
    {
        VerifyErrorKind::OperationContract {
            reason: "exclusive root reborrow requires an exact Inout parameter",
        }
    } else if let Some(dependent) = state.loans.iter().copied().find(|loan| {
        !dependencies.same_origin(parent, *loan)
            && aliases.overlap(EntityId::Loan(parent), EntityId::Loan(*loan))
    }) {
        VerifyErrorKind::LoanDependencyActive { parent, dependent }
    } else {
        return;
    };
    errors.push(error(
        kind,
        VerifyLocation::Instruction(instruction.id),
        &instruction.origin,
    ));
}

fn is_root_reborrow(function: &Function, loan: LoanId) -> bool {
    matches!(
        operation_of(function, EntityId::Loan(loan)),
        Some(Operation::ExclusiveRootReborrow { .. })
    )
}

#[allow(clippy::too_many_arguments)]
fn apply_exchange(
    module: &Module,
    function: &Function,
    (loan, replacement): (LoanId, ValueId),
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    dependencies: &ReborrowDependencies,
    state: &mut BlockState,
    instruction: &Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    if !dependencies
        .flows
        .origins
        .get(&loan)
        .is_some_and(|origins| {
            !origins.is_empty()
                && origins.iter().all(|origin| {
                    is_root_reborrow(function, *origin)
                        || receiver_field_exchange::exclusive_field_base(function, *origin)
                            .is_some_and(|parent| {
                                inout::is_parameter_loan(function, dependencies, parent)
                            })
                })
        })
    {
        errors.push(error(VerifyErrorKind::OperationContract {
            reason: "Inout exchange requires a temporary root or field reborrow of an Inout parameter",
        }, location, origin));
        return;
    }
    if aliases.overlap(EntityId::Value(replacement), EntityId::Loan(loan)) {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value: replacement },
            location,
            origin,
        ));
        return;
    }
    if reborrow::end_loan(
        function,
        loan,
        aliases,
        closure_loans,
        dependencies,
        state,
        location.clone(),
        origin,
        errors,
    ) {
        consume_value(
            module,
            function,
            replacement,
            aliases,
            state,
            &BTreeSet::new(),
            &BTreeSet::new(),
            location,
            origin,
            errors,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_primitive(
    module: &Module,
    function: &Function,
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    dependencies: &ReborrowDependencies,
    state: &mut BlockState,
    instruction: &Instruction,
    errors: &mut Vec<VerifyError>,
) {
    match instruction.operation {
        Operation::ProjectedSwap { root, loans } => apply(
            module,
            function,
            (root, loans),
            aliases,
            closure_loans,
            dependencies,
            state,
            instruction,
            errors,
        ),
        Operation::InoutExchange { loan, replacement } => apply_exchange(
            module,
            function,
            (loan, replacement),
            aliases,
            closure_loans,
            dependencies,
            state,
            instruction,
            errors,
        ),
        Operation::ExclusiveRootReborrow { source } => check_root_reborrow(
            function,
            source,
            aliases,
            dependencies,
            state,
            instruction,
            errors,
        ),
        _ => unreachable!("ownership primitive dispatch"),
    }
}

#[allow(clippy::too_many_arguments)]
fn apply(
    module: &Module,
    function: &Function,
    (root, loans): (Option<ValueId>, [LoanId; 2]),
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    reborrows: &ReborrowDependencies,
    state: &mut BlockState,
    instruction: &Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let [left, right] = loans.map(EntityId::Loan);
    let location = VerifyLocation::Instruction(instruction.id);
    if left == right
        || (aliases.overlap(left, right) && !aliases.direct_fields.disjoint(left, right))
        || !loans[usize::from(root.is_some())..].iter().all(|loan| {
            reborrows.flows.origins.get(loan).is_some_and(|origins| {
                !origins.is_empty()
                    && origins
                        .iter()
                        .all(|origin| is_mutable_projection(module, function, *origin))
            })
        })
    {
        errors.push(error(
            VerifyErrorKind::OperationContract {
                reason: "projected swap requires disjoint direct mutable places",
            },
            location,
            &instruction.origin,
        ));
        return;
    }
    for &loan in loans[usize::from(root.is_some())..].iter().rev() {
        if !reborrow::end_loan(
            function,
            loan,
            aliases,
            closure_loans,
            reborrows,
            state,
            location.clone(),
            &instruction.origin,
            errors,
        ) {
            return;
        }
    }
    if let Some(root) = root {
        root_exchange::apply(
            module,
            function,
            root_exchange::Request {
                owners: &[root],
                loans: &loans[..1],
                replacement: None,
            },
            aliases,
            state,
            instruction,
            errors,
        );
    }
}

fn exclusive_borrow_place(function: &Function, loan: LoanId) -> Option<PlaceId> {
    match operation_of(function, EntityId::Loan(loan))? {
        Operation::BorrowBegin {
            place,
            kind: LoanKind::Exclusive,
        } => Some(*place),
        _ => None,
    }
}

fn operation_of(function: &Function, entity: EntityId) -> Option<&Operation> {
    let Definition::InstructionResult { instruction, .. } = function.entity(entity)?.definition
    else {
        return None;
    };
    Some(&function.instruction(instruction)?.operation)
}

fn is_mutable_projection(module: &Module, function: &Function, loan: LoanId) -> bool {
    is_root_reborrow(function, loan)
        || is_mutable_element(module, function, loan)
        || is_owned_field_origin(function, loan)
        || receiver_field_exchange::exclusive_field_base(function, loan).is_some()
}

fn is_owned_field_origin(function: &Function, loan: LoanId) -> bool {
    exclusive_borrow_place(function, loan)
        .and_then(|place| operation_of(function, EntityId::Place(place)))
        .is_some_and(|operation| {
            matches!(operation, Operation::FieldPlace { base, .. }
            if matches!(operation_of(function, EntityId::Place(*base)),
                Some(Operation::HeapPayloadPlace { .. })))
        })
}

pub(super) fn loan_parameter_requirement(
    function: &Function,
    dependencies: &ReborrowDependencies,
    loan: LoanId,
    operation: &Operation,
) -> Option<&'static str> {
    match operation {
        Operation::LoanReplace { .. } if !is_owned_field_loan(function, dependencies, loan) => {
            Some("LoanReplace requires an Inout parameter loan or a direct owned heap field loan")
        }
        Operation::ContainerAppendInout { .. } => {
            Some("ContainerAppendInout requires an Inout parameter loan")
        }
        _ => None,
    }
}

fn is_owned_field_loan(
    function: &Function,
    reborrows: &ReborrowDependencies,
    loan: LoanId,
) -> bool {
    reborrows.flows.origins.get(&loan).is_some_and(|origins| {
        !origins.is_empty()
            && origins
                .iter()
                .all(|origin| is_owned_field_origin(function, *origin))
    })
}

pub(super) fn is_mutable_element(module: &Module, function: &Function, loan: LoanId) -> bool {
    exclusive_borrow_place(function, loan)
        .and_then(|place| operation_of(function, EntityId::Place(place)))
        .is_some_and(|operation| {
            matches!(operation, Operation::ContainerElementPlace { owner, .. }
            if is_mutable_owner(module, function, *owner))
        })
}

pub(super) fn is_mutable_owner(module: &Module, function: &Function, owner: EntityId) -> bool {
    function.entity(owner).is_some_and(|entity| {
        matches!(
            entity.ty,
            EntityType::Value(_)
                | EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    ..
                }
        ) && matches!(
            module.sequential_container(entity.ty.semantic_type()),
            Some((
                SequentialContainerKind::Array | SequentialContainerKind::MutableList,
                _
            ))
        )
    })
}
