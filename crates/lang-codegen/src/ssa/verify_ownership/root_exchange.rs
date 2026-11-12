//! Atomic owned-root exchanges keep root identity separate from overlap aliasing.

use super::*;

pub(super) struct Request<'a> {
    pub(super) owners: &'a [ValueId],
    pub(super) loans: &'a [LoanId],
    pub(super) replacement: Option<ValueId>,
}

pub(super) fn apply(
    module: &Module,
    function: &Function,
    request: Request<'_>,
    aliases: &AliasRoots,
    state: &mut BlockState,
    instruction: &super::super::model::Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let Request {
        owners,
        loans,
        replacement,
    } = request;
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    let before = errors.len();
    let allowed_loans = loans.iter().copied().collect::<BTreeSet<_>>();
    for (owner, loan) in owners.iter().zip(loans) {
        require_value(
            module,
            function,
            *owner,
            state,
            location.clone(),
            origin,
            errors,
        );
        if !state.loans.contains(loan) {
            errors.push(error(
                VerifyErrorKind::LoanInactive { loan: *loan },
                location.clone(),
                origin,
            ));
        }
        if !exact_root(function, *owner, *loan, aliases) {
            errors.push(error(
                VerifyErrorKind::OperationContract {
                    reason: "root exchange requires an exclusive loan of the exact owned root",
                },
                location.clone(),
                origin,
            ));
        }
        if state.loans.iter().any(|active| {
            !allowed_loans.contains(active)
                && aliases.overlap(EntityId::Value(*owner), EntityId::Loan(*active))
        }) {
            errors.push(error(
                VerifyErrorKind::OwnerLoanConflict { value: *owner },
                location.clone(),
                origin,
            ));
        }
    }
    if allowed_loans.len() != loans.len()
        || owners.iter().enumerate().any(|(index, owner)| {
            owners[..index]
                .iter()
                .any(|other| aliases.overlap(EntityId::Value(*owner), EntityId::Value(*other)))
        })
    {
        errors.push(error(
            VerifyErrorKind::OperationContract {
                reason: "root swap operands must be disjoint",
            },
            location.clone(),
            origin,
        ));
    }
    if let Some(replacement) = replacement {
        require_value(
            module,
            function,
            replacement,
            state,
            location.clone(),
            origin,
            errors,
        );
        if owners
            .iter()
            .any(|owner| aliases.overlap(EntityId::Value(*owner), EntityId::Value(replacement)))
            || has_any_value_loan(replacement, aliases, state)
        {
            errors.push(error(
                VerifyErrorKind::OwnerLoanConflict { value: replacement },
                location.clone(),
                origin,
            ));
        }
    }
    if errors.len() != before {
        return;
    }

    // Commit only once all operands have been checked; no intermediate state escapes.
    for loan in loans {
        state.loans.remove(loan);
    }
    for owner in owners.iter().copied().chain(replacement) {
        consume_value(
            module,
            function,
            owner,
            aliases,
            state,
            &BTreeSet::new(),
            &BTreeSet::new(),
            location.clone(),
            origin,
            errors,
        );
        // Copyable values are independent snapshots, but their old place identity expires too.
        state
            .places
            .retain(|place| !aliases.overlap(EntityId::Value(owner), EntityId::Place(*place)));
    }
}

pub(in crate::ssa) fn exact_place(
    function: &Function,
    owner: ValueId,
    place: PlaceId,
    aliases: &AliasRoots,
) -> bool {
    let owner_roots = &aliases.roots[&EntityId::Value(owner)];
    let place_roots = &aliases.roots[&EntityId::Place(place)];
    !owner_roots.is_empty()
        && owner_roots == place_roots
        && exact_path(
            function,
            owner,
            EntityId::Place(place),
            &mut BTreeSet::new(),
            false,
        )
}

/// Inline fields remain inside the overwritten root storage; allocation projections do not.
pub(in crate::ssa) fn exact_inline_place(
    function: &Function,
    owner: ValueId,
    place: PlaceId,
    aliases: &AliasRoots,
) -> bool {
    let owner_roots = &aliases.roots[&EntityId::Value(owner)];
    let place_roots = &aliases.roots[&EntityId::Place(place)];
    !owner_roots.is_empty()
        && owner_roots == place_roots
        && exact_path(
            function,
            owner,
            EntityId::Place(place),
            &mut BTreeSet::new(),
            true,
        )
}

fn exact_root(function: &Function, owner: ValueId, loan: LoanId, aliases: &AliasRoots) -> bool {
    // Alias equality alone is insufficient: a projected field has the same overlap roots,
    // and CFG joins can have equal origin sets with mismatched owner/loan pairings.
    let owner_roots = &aliases.roots[&EntityId::Value(owner)];
    let loan_roots = &aliases.roots[&EntityId::Loan(loan)];
    !owner_roots.is_empty()
        && owner_roots == loan_roots
        && exact_path(
            function,
            owner,
            EntityId::Loan(loan),
            &mut BTreeSet::new(),
            false,
        )
}

fn exact_path(
    function: &Function,
    owner: ValueId,
    access: EntityId,
    visiting: &mut BTreeSet<(ValueId, EntityId)>,
    allow_inline: bool,
) -> bool {
    if access == EntityId::Value(owner) {
        return true;
    }
    if !visiting.insert((owner, access)) {
        // Loops are checked coinductively; exact_root also requires a real, nonempty origin.
        return true;
    }
    let valid = match function.entity(access).expect("verified entity").definition {
        Definition::InstructionResult {
            instruction,
            index: 0,
        } => {
            match (
                &function
                    .instruction(instruction)
                    .expect("verified instruction")
                    .operation,
                access,
            ) {
                (
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Exclusive,
                    },
                    EntityId::Loan(_),
                ) => exact_path(
                    function,
                    owner,
                    EntityId::Place(*place),
                    visiting,
                    allow_inline,
                ),
                (Operation::RootPlace { owner: source }, EntityId::Place(_)) => exact_path(
                    function,
                    owner,
                    EntityId::Value(*source),
                    visiting,
                    allow_inline,
                ),
                (Operation::FieldPlace { base, .. }, EntityId::Place(_)) if allow_inline => {
                    exact_path(
                        function,
                        owner,
                        EntityId::Place(*base),
                        visiting,
                        allow_inline,
                    )
                }
                (_, EntityId::Value(_)) => {
                    expand_owner(function, owner, access, visiting, allow_inline)
                }
                _ => false,
            }
        }
        Definition::BlockParameter { block, index } => {
            let incoming = incoming_edges(function, block);
            if incoming.is_empty() {
                matches!(access, EntityId::Value(_))
                    && expand_owner(function, owner, access, visiting, allow_inline)
            } else {
                incoming.iter().all(|edge| {
                    exact_path(
                        function,
                        rebound_owner(function, owner, block, edge),
                        edge.arguments[index],
                        visiting,
                        allow_inline,
                    )
                })
            }
        }
        Definition::InstructionResult { .. } => {
            matches!(access, EntityId::Value(_))
                && expand_owner(function, owner, access, visiting, allow_inline)
        }
    };
    visiting.remove(&(owner, access));
    valid
}

fn expand_owner(
    function: &Function,
    owner: ValueId,
    access: EntityId,
    visiting: &mut BTreeSet<(ValueId, EntityId)>,
    allow_inline: bool,
) -> bool {
    let Definition::BlockParameter { block, index } = function
        .entity(EntityId::Value(owner))
        .expect("verified owner")
        .definition
    else {
        return false;
    };
    let incoming = incoming_edges(function, block);
    !incoming.is_empty()
        && incoming.iter().all(|edge| {
            let EntityId::Value(source) = edge.arguments[index] else {
                return false;
            };
            let access = match function.entity(access).expect("verified access").definition {
                Definition::BlockParameter {
                    block: access_block,
                    index,
                } if access_block == block => edge.arguments[index],
                _ => access,
            };
            exact_path(function, source, access, visiting, allow_inline)
        })
}

fn rebound_owner(function: &Function, owner: ValueId, block: BlockId, edge: &Edge) -> ValueId {
    match function
        .entity(EntityId::Value(owner))
        .expect("verified owner")
        .definition
    {
        Definition::BlockParameter {
            block: owner_block,
            index,
        } if owner_block == block => {
            let EntityId::Value(owner) = edge.arguments[index] else {
                unreachable!("verified edge value type")
            };
            owner
        }
        _ => owner,
    }
}

fn incoming_edges(function: &Function, target: BlockId) -> Vec<&Edge> {
    function
        .blocks
        .iter()
        .flat_map(|block| edges(&block.terminator.as_ref().expect("verified terminator").kind))
        .filter(|edge| edge.target == target)
        .collect()
}

pub(in crate::ssa) mod integer {
    //! Integer conversion, scalar constant, byte-array text, numeric Range, and borrow-result contracts.

    use crate::ssa::{
        model::{
            EntityId, EntityType, Function, LoanId, LoanKind, Module, ScalarConstant,
            SequentialContainerKind, SsaTypeId, SsaTypeKind, ValueId,
        },
        verify_operation::{
            entity_type, integer_fits, is_bitwise_integer, is_boolean, is_koven_int,
            single_value_result, value_type,
        },
    };

    pub(in crate::ssa) mod verify_borrow_result {
        use crate::ssa::{
            model::*,
            verify::{VerifyError, VerifyErrorKind, VerifyLocation},
        };

        pub(in crate::ssa) fn parameter(function: &Function) -> Option<(LoanId, SsaTypeId)> {
            if function.receiver.is_some() {
                return None;
            }
            let index = function.borrow_return?;
            if !function.blocks.first()?.parameters.iter().all(|entity| {
                matches!(
                    function.entity(*entity).map(|data| data.ty),
                    Some(
                        EntityType::Value(_)
                            | EntityType::Loan {
                                kind: LoanKind::Shared,
                                ..
                            }
                    )
                )
            }) {
                return None;
            }
            let [target] = function.return_types.as_slice() else {
                return None;
            };
            let entity = *function.blocks.first()?.parameters.get(index)?;
            let EntityId::Loan(loan) = entity else {
                return None;
            };
            matches!(
                function.entity(entity)?.ty,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    ..
                }
            )
            .then_some((loan, *target))
        }

        pub(in crate::ssa) fn call_contract(
            module: &Module,
            function: &Function,
            callee: FunctionId,
            arguments: &[EntityId],
            source: LoanId,
            results: &[EntityType],
        ) -> bool {
            let Some(callee) = module.function(callee) else {
                return false;
            };
            let Some((_, target)) = parameter(callee) else {
                return false;
            };
            let Some(index) = callee.borrow_return else {
                return false;
            };
            let Some(entry) = callee.blocks.first() else {
                return false;
            };
            arguments.len() == entry.parameters.len()
                && arguments.get(index) == Some(&EntityId::Loan(source))
                && arguments.iter().zip(&entry.parameters).all(|(arg, param)| {
                    !matches!(arg, EntityId::Place(_))
                        && callee.entity(*param).is_some_and(|parameter| {
                            crate::ssa::verify_operation::is_first_class(
                                module,
                                parameter.ty.semantic_type(),
                            ) && function.entity(*arg).map(|data| data.ty) == Some(parameter.ty)
                        })
                })
                && crate::ssa::verify_operation::is_first_class(module, target)
                && results
                    == [EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    }]
        }

        pub(in crate::ssa) fn return_contract(
            function: &Function,
            loan: LoanId,
            block: BlockId,
            origin: &Origin,
            errors: &mut Vec<VerifyError>,
        ) {
            let valid = parameter(function).is_some_and(|(_, target)| {
                function.entity(EntityId::Loan(loan)).map(|data| data.ty)
                    == Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    })
            });
            if !valid {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::ReturnType { index: 0 },
                    location: VerifyLocation::Terminator(block),
                    origin: Some(origin.clone()),
                });
            }
        }
    }

    pub(in crate::ssa) fn checked_conversion_contract(
        module: &Module,
        function: &Function,
        operand: ValueId,
        results: &[EntityType],
    ) -> bool {
        let [EntityType::Value(target), EntityType::Value(failed)] = results else {
            return false;
        };
        is_bitwise_integer(module, value_type(function, operand))
            && is_bitwise_integer(module, Some(*target))
            && is_boolean(module, *failed)
    }

    pub(in crate::ssa) fn constant_contract(
        module: &Module,
        constant: &ScalarConstant,
        results: &[EntityType],
    ) -> bool {
        let Some(result) = single_value_result(results) else {
            return false;
        };
        match (constant, module.type_kind(result)) {
            #[cfg(test)]
            (ScalarConstant::SyntheticZero, Some(SsaTypeKind::ZeroSized { .. })) => true,
            (ScalarConstant::Unit, Some(SsaTypeKind::Unit))
            | (ScalarConstant::Boolean(_), Some(SsaTypeKind::Boolean)) => true,
            (ScalarConstant::Char(value), Some(SsaTypeKind::Char)) => {
                char::from_u32(*value).is_some()
            }
            (ScalarConstant::Integer(value), Some(SsaTypeKind::Integer { bits, signed })) => {
                integer_fits(*value, *bits, *signed)
            }
            _ => false,
        }
    }

    pub(in crate::ssa) fn shared_byte_array(
        module: &Module,
        function: &Function,
        source: LoanId,
    ) -> bool {
        let EntityType::Loan {
            target,
            kind: LoanKind::Shared,
        } = entity_type(function, EntityId::Loan(source))
        else {
            return false;
        };
        let Some(SsaTypeKind::SequentialContainer {
            kind: SequentialContainerKind::Array,
            element,
        }) = module.type_kind(target)
        else {
            return false;
        };
        matches!(
            module.type_kind(*element),
            Some(SsaTypeKind::Integer {
                bits: 8,
                signed: false
            })
        )
    }

    pub(in crate::ssa) fn range_construct(
        module: &Module,
        function: &Function,
        first: ValueId,
        bound: ValueId,
        results: &[EntityType],
    ) -> bool {
        let Some(range) = single_value_result(results) else {
            return false;
        };
        let Some(endpoint) = module.integer_range_endpoint(range) else {
            return false;
        };
        module.valid_integer_range_type(range, endpoint)
            && value_type(function, first) == Some(endpoint)
            && value_type(function, bound) == Some(endpoint)
    }

    pub(in crate::ssa) fn range_contains(
        module: &Module,
        function: &Function,
        value: ValueId,
        range: ValueId,
        results: &[EntityType],
    ) -> bool {
        let Some(range) = value_type(function, range) else {
            return false;
        };
        let Some(endpoint) = module.integer_range_endpoint(range) else {
            return false;
        };
        module.valid_integer_range_type(range, endpoint)
            && value_type(function, value) == Some(endpoint)
            && single_value_result(results).is_some_and(|ty| is_boolean(module, ty))
    }

    pub(in crate::ssa) fn range_equal(
        module: &Module,
        function: &Function,
        left: ValueId,
        right: ValueId,
        results: &[EntityType],
    ) -> bool {
        let Some(range) = value_type(function, left) else {
            return false;
        };
        let Some(endpoint) = module.integer_range_endpoint(range) else {
            return false;
        };
        module.valid_integer_range_type(range, endpoint)
            && value_type(function, right) == Some(range)
            && single_value_result(results).is_some_and(|ty| is_boolean(module, ty))
    }

    pub(in crate::ssa) fn borrowed_generate_contract(
        module: &Module,
        function: &Function,
        container: SsaTypeId,
        length: ValueId,
        initializer: LoanId,
        results: &[EntityType],
    ) -> bool {
        let Some((_, element)) = module.sequential_container(container) else {
            return false;
        };
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = entity_type(function, EntityId::Loan(initializer))
        else {
            return false;
        };
        let Some(signature) = module
            .callable_signature(target)
            .or_else(|| module.callable_view_signature(target))
        else {
            return false;
        };
        let [
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: index,
            },
        ] = signature.parameters.as_slice()
        else {
            return false;
        };
        value_type(function, length).is_some_and(|ty| is_koven_int(module, ty))
            && is_koven_int(module, *index)
            && single_value_result(results) == Some(container)
            && !signature.never_returns
            && (signature.returns == [element]
                || (signature.returns.is_empty()
                    && matches!(module.type_kind(element), Some(SsaTypeKind::Unit))))
    }

    pub(in crate::ssa) mod map {
        //! Local type contracts for Map container operations.

        use crate::ssa::{
            model::{
                EntityId, EntityType, Function, LoanId, LoanKind, MapContainerKind, Module,
                Operation, Ownership, SsaTypeId, SsaTypeKind, ValueId,
            },
            verify_operation::{is_koven_int, single_value_result, value_type},
        };

        #[cfg(test)]
        #[path = "../../../../verify_operation/map/require_tests.rs"]
        mod require_tests;
        #[cfg(test)]
        #[path = "../../../../verify_operation/map/tests.rs"]
        mod tests;
        #[cfg(test)]
        #[path = "../../../../verify_operation/map/with_tests.rs"]
        mod with_tests;

        fn read_type(function: &Function, entity: EntityId) -> Option<SsaTypeId> {
            match function.entity(entity)?.ty {
                EntityType::Value(ty)
                | EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                } => Some(ty),
                _ => None,
            }
        }

        pub(in crate::ssa) fn verify_map_operation(
            module: &Module,
            function: &Function,
            operation: &Operation,
            results: &[EntityType],
        ) -> bool {
            match operation {
                Operation::MapIterationFirst { .. }
                | Operation::MapIterationHasNext { .. }
                | Operation::MapIterationNext { .. }
                | Operation::MapIterationKeyLoan { .. }
                | Operation::MapIterationValueLoan { .. } => {
                    verify_map_iteration(module, function, operation, results)
                }
                Operation::MapConsume { owner, map_type } => {
                    let Some(EntityType::Value(source_type)) = function
                        .entity(EntityId::Value(*owner))
                        .map(|entity| entity.ty)
                    else {
                        return false;
                    };
                    let Some((MapContainerKind::MutableMap, key, value)) =
                        module.map_container(source_type)
                    else {
                        return false;
                    };
                    module.map_container(*map_type) == Some((MapContainerKind::Map, key, value))
                        && module.type_ownership(source_type) == Some(Ownership::MoveOnly)
                        && module.type_ownership(*map_type) == Some(Ownership::MoveOnly)
                        && single_value_result(results) == Some(*map_type)
                }
                Operation::MapConstruct { map_type } => {
                    module.map_container(*map_type).is_some()
                        && single_value_result(results) == Some(*map_type)
                }
                Operation::MapSize { owner } => {
                    let owner_type = match function.entity(*owner).map(|e| e.ty) {
                        Some(EntityType::Value(ty))
                        | Some(EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: ty,
                        }) => Some(ty),
                        _ => None,
                    };
                    owner_type.and_then(|ty| module.map_container(ty)).is_some()
                        && single_value_result(results).is_some_and(|ty| is_koven_int(module, ty))
                }
                Operation::MapContains { owner, key } => {
                    let owner_type = match function.entity(*owner).map(|e| e.ty) {
                        Some(EntityType::Value(ty))
                        | Some(EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: ty,
                        }) => Some(ty),
                        _ => None,
                    };
                    let Some((_, key_ty, _)) = owner_type.and_then(|ty| module.map_container(ty))
                    else {
                        return false;
                    };
                    read_type(function, *key) == Some(key_ty)
                        && single_value_result(results).is_some_and(|ty| {
                            matches!(module.type_kind(ty), Some(SsaTypeKind::Boolean))
                        })
                }
                Operation::MapRequireValue { source, key } => {
                    let Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    }) = function.entity(EntityId::Loan(*source)).map(|data| data.ty)
                    else {
                        return false;
                    };
                    let Some((_, key_type, value_type)) = module.map_container(target) else {
                        return false;
                    };
                    read_type(function, *key) == Some(key_type)
                        && results
                            == [EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: value_type,
                            }]
                }
                Operation::MapWithValue {
                    source,
                    key,
                    action,
                } => {
                    let Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    }) = function.entity(EntityId::Loan(*source)).map(|data| data.ty)
                    else {
                        return false;
                    };
                    let Some((_, key_type, value_type)) = module.map_container(target) else {
                        return false;
                    };
                    let Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: action_type,
                    }) = function.entity(EntityId::Loan(*action)).map(|data| data.ty)
                    else {
                        return false;
                    };
                    let Some(signature) = module.callable_signature(action_type) else {
                        return false;
                    };
                    read_type(function, *key) == Some(key_type)
                        && signature.parameters
                            == [EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: value_type,
                            }]
                        && !signature.never_returns
                        && signature.returns.is_empty()
                        && single_value_result(results).is_some_and(|ty| {
                            matches!(module.type_kind(ty), Some(SsaTypeKind::Boolean))
                        })
                }
                Operation::MapGet { owner, key } => {
                    let owner_type = match function.entity(*owner).map(|e| e.ty) {
                        Some(EntityType::Value(ty))
                        | Some(EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: ty,
                        }) => Some(ty),
                        _ => None,
                    };
                    let Some((_, key_ty, val_ty)) =
                        owner_type.and_then(|ty| module.map_container(ty))
                    else {
                        return false;
                    };
                    let Some(res_ty) = single_value_result(results) else {
                        return false;
                    };
                    read_type(function, *key) == Some(key_ty)
                        && module.type_ownership(val_ty) == Some(Ownership::Copyable)
                        && module.map_result_value(res_ty) == Some(val_ty)
                }
                Operation::MapResultUnwrap { result } => {
                    value_type(function, *result).and_then(|ty| module.map_result_value(ty))
                        == single_value_result(results)
                        && single_value_result(results).is_some_and(|ty| {
                            module.type_ownership(ty) == Some(Ownership::Copyable)
                        })
                }
                Operation::MapPut { owner, key, value } => {
                    let Some((kind, key_ty, val_ty)) =
                        value_type(function, *owner).and_then(|ty| module.map_container(ty))
                    else {
                        return false;
                    };
                    kind == MapContainerKind::MutableMap
                        && value_type(function, *key) == Some(key_ty)
                        && value_type(function, *value) == Some(val_ty)
                        && single_value_result(results) == value_type(function, *owner)
                }
                Operation::MapRemove { owner, key } => {
                    let Some((kind, key_ty, val_ty)) =
                        value_type(function, *owner).and_then(|ty| module.map_container(ty))
                    else {
                        return false;
                    };
                    if kind != MapContainerKind::MutableMap
                        || read_type(function, *key) != Some(key_ty)
                    {
                        return false;
                    }
                    let owner_ty = value_type(function, *owner).unwrap();
                    if results.len() == 2 {
                        results[0] == EntityType::Value(owner_ty)
                            && matches!(results[1], EntityType::Value(res_ty)
                                if module.map_result_value(res_ty) == Some(val_ty)
                                    || (module.map_container(val_ty).is_some()
                                        && module.nullable_map_inner(res_ty) == Some(val_ty))
                                    || matches!(module.type_kind(res_ty), Some(SsaTypeKind::NullableHandle { inner }) if *inner == val_ty))
                    } else {
                        false
                    }
                }
                _ => false,
            }
        }

        fn iteration_source_types(
            module: &Module,
            function: &Function,
            source: LoanId,
        ) -> Option<(SsaTypeId, SsaTypeId)> {
            let EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            } = function.entity(EntityId::Loan(source))?.ty
            else {
                return None;
            };
            let (_, key, value) = module.map_container(target)?;
            Some((key, value))
        }

        fn is_cursor(module: &Module, ty: SsaTypeId) -> bool {
            matches!(
                module.type_kind(ty),
                Some(SsaTypeKind::Integer {
                    bits: 64,
                    signed: false
                })
            )
        }

        fn cursor_type(module: &Module, function: &Function, cursor: ValueId) -> Option<SsaTypeId> {
            value_type(function, cursor).filter(|ty| is_cursor(module, *ty))
        }

        fn verify_map_iteration(
            module: &Module,
            function: &Function,
            operation: &Operation,
            results: &[EntityType],
        ) -> bool {
            let (source, cursor) = match *operation {
                Operation::MapIterationFirst { source } => (source, None),
                Operation::MapIterationHasNext { source, cursor }
                | Operation::MapIterationNext { source, cursor }
                | Operation::MapIterationKeyLoan { source, cursor }
                | Operation::MapIterationValueLoan { source, cursor } => (source, Some(cursor)),
                _ => return false,
            };
            let Some((key, value)) = iteration_source_types(module, function, source) else {
                return false;
            };
            if cursor.is_some_and(|cursor| cursor_type(module, function, cursor).is_none()) {
                return false;
            }
            match *operation {
                Operation::MapIterationFirst { .. } => {
                    single_value_result(results).is_some_and(|ty| is_cursor(module, ty))
                }
                Operation::MapIterationNext { cursor, .. } => {
                    single_value_result(results) == cursor_type(module, function, cursor)
                }
                Operation::MapIterationHasNext { .. } => single_value_result(results)
                    .is_some_and(|ty| matches!(module.type_kind(ty), Some(SsaTypeKind::Boolean))),
                Operation::MapIterationKeyLoan { .. } => {
                    results
                        == [EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: key,
                        }]
                }
                Operation::MapIterationValueLoan { .. } => {
                    results
                        == [EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: value,
                        }]
                }
                _ => false,
            }
        }
    }
}
