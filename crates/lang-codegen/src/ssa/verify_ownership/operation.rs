//! Ownership effects of one typed-SSA instruction.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_operation(
    module: &Module,
    function: &Function,
    instruction: &crate::ssa::model::Instruction,
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    reborrows: &ReborrowDependencies,
    state: &mut BlockState,
    errors: &mut Vec<VerifyError>,
) {
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    if !element_swap::check_suspended_root_use(function, reborrows, state, instruction, errors) {
        return;
    }
    match &instruction.operation {
        Operation::Constant(_)
        | Operation::PrintLiteral { .. }
        | Operation::StringLiteral { .. }
        | Operation::Binary { .. }
        | Operation::CheckedArithmetic { .. }
        | Operation::IntegerRangeConstruct { .. }
        | Operation::IntegerRangeContains { .. }
        | Operation::IntegerRangeEqual { .. }
        | Operation::CheckedIntegerConvert { .. }
        | Operation::IntegerBitwise { .. }
        | Operation::IntegerNot { .. }
        | Operation::Compare { .. }
        | Operation::BooleanNot { .. } => {}
        Operation::StringConcat { left, right } | Operation::StringEqual { left, right } => {
            require_string_view(
                module,
                function,
                *left,
                aliases,
                state,
                location.clone(),
                origin,
                errors,
            );
            require_string_view(
                module, function, *right, aliases, state, location, origin, errors,
            );
        }
        Operation::StringClone { source }
        | Operation::StringByteLength { source }
        | Operation::StringByteAt { source, .. }
        | Operation::StringCopySlice { source, .. } => {
            require_string_view(
                module,
                function,
                EntityId::Loan(*source),
                aliases,
                state,
                location,
                origin,
                errors,
            );
        }
        Operation::PrintString { value }
        | Operation::Utf8ErrorOffset { source: value }
        | Operation::StringCopyBytes { source: value } => {
            if !state.loans.contains(value) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan: *value },
                    location,
                    origin,
                ));
            }
        }
        Operation::DirectCall {
            receiver,
            arguments,
            ..
        } => borrow_result::verify_call_arguments(
            module,
            function,
            instruction,
            receiver.iter().chain(arguments).copied(),
            aliases,
            closure_loans,
            reborrows,
            state,
            errors,
        ),
        Operation::FunctionAddress { .. } => {}
        Operation::RangeConstruct { .. }
        | Operation::RangeCall { .. }
        | Operation::RangeLength { .. }
        | Operation::RangeElementPlace { .. }
        | Operation::RangeEnd { .. } => range::apply(
            module,
            function,
            instruction,
            aliases,
            closure_loans,
            reborrows,
            state,
            errors,
        ),
        Operation::BorrowCall { .. } => borrow_result::verify_call(
            module,
            function,
            instruction,
            aliases,
            closure_loans,
            reborrows,
            state,
            errors,
        ),
        Operation::ClosureConstruct { captures, .. } => {
            closure::apply_construct(
                module,
                function,
                captures,
                aliases,
                closure_loans,
                state,
                location,
                origin,
                errors,
            );
        }
        Operation::CallableInvokeView { view, arguments } => {
            closure::apply_invoke_view(
                module,
                function,
                *view,
                arguments,
                aliases,
                closure_loans,
                state,
                location,
                origin,
                errors,
            );
        }
        Operation::CallableInvoke {
            callable,
            arguments,
        } => {
            closure::apply_invoke(
                module,
                function,
                *callable,
                arguments,
                aliases,
                closure_loans,
                state,
                location,
                origin,
                errors,
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
        Operation::NullableLoanIsNull { source } => {
            if !state.loans.contains(source) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan: *source },
                    location,
                    origin,
                ));
            }
        }
        Operation::NullableTake { owner, proof } => {
            let owner_available = require_value(
                module,
                function,
                *owner,
                state,
                location.clone(),
                origin,
                errors,
            );
            if !aliases.overlap(EntityId::Value(*owner), EntityId::Loan(*proof)) {
                errors.push(error(
                    VerifyErrorKind::NullableProofMismatch {
                        owner: *owner,
                        proof: *proof,
                    },
                    location,
                    origin,
                ));
            } else if owner_available
                && reborrow::end_loan(
                    function,
                    *proof,
                    aliases,
                    closure_loans,
                    reborrows,
                    state,
                    location.clone(),
                    origin,
                    errors,
                )
            {
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
        }
        Operation::HeapPayloadPlace { owner } => {
            require_value(module, function, *owner, state, location, origin, errors);
        }
        Operation::HeapFieldRead { .. } => receiver_field_exchange::read_field(
            module,
            function,
            instruction,
            aliases,
            reborrows,
            state,
            errors,
        ),
        Operation::HeapFieldReplace {
            receiver, value, ..
        }
        | Operation::InlineFieldReplace {
            receiver, value, ..
        } => {
            if !state.loans.contains(receiver) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan: *receiver },
                    location.clone(),
                    origin,
                ));
            } else if let Some(dependent) = match instruction.operation {
                Operation::HeapFieldReplace { field, .. } => {
                    receiver_field_exchange::dependent_on_field(
                        function, *receiver, field, aliases, reborrows, state,
                    )
                }
                _ => reborrows.active_descendant(*receiver, aliases, state),
            } {
                errors.push(error(
                    VerifyErrorKind::LoanDependencyActive {
                        parent: *receiver,
                        dependent,
                    },
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
        Operation::ExclusiveHeapFieldLoan { .. }
        | Operation::SharedHeapFieldLoan { .. }
        | Operation::HeapReceiverFieldExchange { .. } => receiver_field_exchange::apply(
            module,
            function,
            instruction,
            aliases,
            reborrows,
            state,
            errors,
        ),
        Operation::HeapFieldExchange {
            owner,
            field,
            loan,
            replacement,
        } => {
            field_exchange::apply(
                module,
                function,
                field_exchange::Request {
                    owner: *owner,
                    field: *field,
                    loan: *loan,
                    replacement: *replacement,
                },
                aliases,
                state,
                instruction,
                errors,
            );
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
        Operation::ContainerGenerateBorrowed {
            length,
            initializer,
            ..
        } => {
            require_value(
                module,
                function,
                *length,
                state,
                location.clone(),
                origin,
                errors,
            );
            closure::apply_borrowed_generate(
                *initializer,
                closure_loans,
                state,
                location,
                origin,
                errors,
            );
        }
        Operation::ContainerLength { owner } => match owner {
            EntityId::Value(value) => {
                if require_value(
                    module,
                    function,
                    *value,
                    state,
                    location.clone(),
                    origin,
                    errors,
                ) && has_exclusive_value_loan(function, *value, aliases, state)
                {
                    errors.push(error(
                        VerifyErrorKind::OwnerLoanConflict { value: *value },
                        location,
                        origin,
                    ));
                }
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
            EntityId::Place(_) => unreachable!("operation contract rejects a place owner"),
        },
        Operation::ContainerElementPlace { owner, index } => {
            require_value(
                module,
                function,
                *index,
                state,
                location.clone(),
                origin,
                errors,
            );
            match owner {
                EntityId::Value(owner) => {
                    if require_value(
                        module,
                        function,
                        *owner,
                        state,
                        location.clone(),
                        origin,
                        errors,
                    ) && element_swap::projection_conflicts(
                        function,
                        *owner,
                        instruction,
                        aliases,
                        state,
                    ) {
                        errors.push(error(
                            VerifyErrorKind::OwnerLoanConflict { value: *owner },
                            location,
                            origin,
                        ));
                    }
                }
                EntityId::Loan(loan) => element_swap::check_provider(
                    function,
                    *loan,
                    reborrows,
                    state,
                    instruction,
                    errors,
                ),
                EntityId::Place(_) => unreachable!("operation contract rejects a place owner"),
            }
        }
        Operation::ProjectedSwap { .. }
        | Operation::InoutExchange { .. }
        | Operation::ExclusiveRootReborrow { .. } => element_swap::apply_primitive(
            module,
            function,
            aliases,
            closure_loans,
            reborrows,
            state,
            instruction,
            errors,
        ),
        Operation::ContainerAppend { owner, element } => {
            check_and_consume_container_owner(
                module,
                function,
                *owner,
                aliases,
                state,
                location.clone(),
                origin,
                errors,
            );
            consume_value(
                module,
                function,
                *element,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::ContainerClear { owner }
        | Operation::ContainerRemoveFirst { owner }
        | Operation::ContainerRemoveLast { owner } => {
            check_and_consume_container_owner(
                module, function, *owner, aliases, state, location, origin, errors,
            );
        }
        Operation::ContainerRemoveAt { owner, index } => {
            check_and_consume_container_owner(
                module,
                function,
                *owner,
                aliases,
                state,
                location.clone(),
                origin,
                errors,
            );
            consume_value(
                module,
                function,
                *index,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::ContainerInsertAt {
            owner,
            index,
            element,
        } => {
            check_and_consume_container_owner(
                module,
                function,
                *owner,
                aliases,
                state,
                location.clone(),
                origin,
                errors,
            );
            consume_value(
                module,
                function,
                *index,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location.clone(),
                origin,
                errors,
            );
            consume_value(
                module,
                function,
                *element,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
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
        Operation::MapIterationFirst { .. }
        | Operation::MapIterationHasNext { .. }
        | Operation::MapIterationNext { .. }
        | Operation::MapIterationKeyLoan { .. }
        | Operation::MapIterationValueLoan { .. }
        | Operation::MapConstruct { .. }
        | Operation::MapConsume { .. }
        | Operation::MapSize { .. }
        | Operation::MapContains { .. }
        | Operation::MapGet { .. }
        | Operation::MapRequireValue { .. }
        | Operation::MapWithValue { .. }
        | Operation::MapResultUnwrap { .. }
        | Operation::MapPut { .. }
        | Operation::MapRemove { .. } => {
            if let Operation::MapWithValue { action, .. } = instruction.operation {
                closure::check_borrowed_contents(
                    action,
                    closure_loans,
                    state,
                    instruction.id,
                    origin,
                    errors,
                    "map action requires proved current callable capture contents",
                );
            }
            map::verify_map_ownership(
                module,
                function,
                &instruction.operation,
                aliases,
                reborrows,
                state,
                location,
                origin,
                errors,
            );
        }
        Operation::FieldPlace { base, .. } => {
            require_place(*base, state, location, origin, errors);
        }
        Operation::SharedFieldLoan { base, .. } => {
            if !state.loans.contains(base) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan: *base },
                    location,
                    origin,
                ));
            }
        }
        Operation::SharedReborrow { .. } => receiver_field_exchange::shared_reborrow(
            function,
            instruction,
            reborrows,
            state,
            errors,
        ),
        Operation::SharedReferenceFollow { source }
        | Operation::SharedPayloadLoan { source }
        | Operation::CallableViewOf { source } => {
            if !state.loans.contains(source) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan: *source },
                    location,
                    origin,
                ));
            }
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
        Operation::RootPlaceTake { owner, place } => {
            let place_active = require_place(*place, state, location.clone(), origin, errors);
            let owner_available = require_value(
                module,
                function,
                *owner,
                state,
                location.clone(),
                origin,
                errors,
            );
            let allowed_places = BTreeSet::from([*place]);
            if place_active
                && owner_available
                && closure_loans.root_take_ready(instruction, *owner, state, errors)
                && consume_value(
                    module,
                    function,
                    *owner,
                    aliases,
                    state,
                    &BTreeSet::new(),
                    &allowed_places,
                    location,
                    origin,
                    errors,
                )
            {
                state.places.remove(place);
                state.inout_writebacks.remove(owner);
            }
        }
        Operation::RootReplace {
            owner,
            loan,
            replacement,
        } => {
            root_exchange::apply(
                module,
                function,
                root_exchange::Request {
                    owners: &[*owner],
                    loans: &[*loan],
                    replacement: Some(*replacement),
                },
                aliases,
                state,
                instruction,
                errors,
            );
        }
        Operation::RootSwap { owners, loans } => {
            root_exchange::apply(
                module,
                function,
                root_exchange::Request {
                    owners,
                    loans,
                    replacement: None,
                },
                aliases,
                state,
                instruction,
                errors,
            );
        }
        Operation::BorrowBegin { place, kind } => element_swap::check_borrow(
            function,
            (*place, *kind),
            aliases,
            reborrows,
            state,
            instruction,
            errors,
        ),
        Operation::BorrowEnd { loan } => {
            reborrow::end_loan(
                function,
                *loan,
                aliases,
                closure_loans,
                reborrows,
                state,
                location,
                origin,
                errors,
            );
        }
        Operation::Read { source } => {
            let entity = match source {
                PlaceAccess::Place(place) => {
                    if require_place(*place, state, location.clone(), origin, errors)
                        && has_exclusive_loan(function, *place, aliases, reborrows, state)
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
        Operation::LoanMutate { loan, value }
        | Operation::LoanReplace { loan, value }
        | Operation::ContainerAppendInout {
            loan,
            element: value,
        } => {
            let parameter_requirement = element_swap::loan_parameter_requirement(
                function,
                reborrows,
                *loan,
                &instruction.operation,
            );
            if let Some(reason) = parameter_requirement
                && !inout::is_parameter_loan(function, reborrows, *loan)
            {
                errors.push(error(
                    VerifyErrorKind::OperationContract { reason },
                    location.clone(),
                    origin,
                ));
            }
            if !state.loans.contains(loan) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan: *loan },
                    location.clone(),
                    origin,
                ));
            } else if let Some(dependent) = reborrows.active_descendant(*loan, aliases, state) {
                errors.push(error(
                    VerifyErrorKind::LoanDependencyActive {
                        parent: *loan,
                        dependent,
                    },
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
        Operation::Drop { owner } => closure::apply_drop(
            module,
            function,
            *owner,
            aliases,
            closure_loans,
            reborrows,
            state,
            location,
            origin,
            errors,
        ),
    }
}
