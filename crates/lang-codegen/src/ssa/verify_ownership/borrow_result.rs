//! 返回仅交接指定入口参数的 shared loan 链，其他局部 loan 仍须正常清理。
use super::*;
use crate::ssa::model::Instruction;

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_return(
    function: &Function,
    loan: LoanId,
    aliases: &AliasRoots,
    dependencies: &ReborrowDependencies,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &super::super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    let Some((parameter, _)) = super::super::verify_borrow_result::parameter(function) else {
        return;
    };
    let expected = BTreeSet::from([EntityId::Loan(parameter)]);
    if aliases.roots.get(&EntityId::Loan(loan)) != Some(&expected) {
        errors.push(error(
            VerifyErrorKind::ReturnType { index: 0 },
            location.clone(),
            origin,
        ));
        return;
    }
    if !state.loans.contains(&loan) {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan },
            location.clone(),
            origin,
        ));
        return;
    }
    if let Some(dependent) = dependencies.active_descendant(loan, aliases, state) {
        errors.push(error(
            VerifyErrorKind::LoanDependencyActive {
                parent: loan,
                dependent,
            },
            location.clone(),
            origin,
        ));
        return;
    }
    let mut current = loan;
    let mut visited = BTreeSet::new();
    while visited.insert(current) {
        state.loans.remove(&current);
        if current == parameter {
            return;
        }
        let Some(parent) = dependencies.parent_by_child.get(&current).copied() else {
            break;
        };
        current = parent;
    }
    errors.push(error(
        VerifyErrorKind::ReturnType { index: 0 },
        location,
        origin,
    ));
}

/// 参数交付复用普通调用合同；唯一来源另有结果延续身份。
#[allow(clippy::too_many_arguments)]
pub(super) fn verify_call(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    reborrows: &ReborrowDependencies,
    state: &mut BlockState,
    errors: &mut Vec<VerifyError>,
) {
    let Operation::BorrowCall {
        arguments, source, ..
    } = &instruction.operation
    else {
        return;
    };
    verify_call_arguments(
        module,
        function,
        instruction,
        arguments.iter().copied(),
        aliases,
        closure_loans,
        reborrows,
        state,
        errors,
    );
    // 有效 call 的 source 已作为实参检查；仍独立拒绝缺失实参中的失效 source。
    if !arguments.contains(&EntityId::Loan(*source)) && !state.loans.contains(source) {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan: *source },
            VerifyLocation::Instruction(instruction.id),
            &instruction.origin,
        ));
    }
}

/// DirectCall 与 BorrowCall 使用相同的 Value 消费和同步 Loan 能力检查。
#[allow(clippy::too_many_arguments)]
pub(super) fn verify_call_arguments(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    arguments: impl IntoIterator<Item = EntityId>,
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    reborrows: &ReborrowDependencies,
    state: &mut BlockState,
    errors: &mut Vec<VerifyError>,
) {
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    for argument in arguments {
        match argument {
            EntityId::Value(value) => {
                consume_value(
                    module,
                    function,
                    value,
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
                closure::check_borrowed_contents(
                    loan,
                    closure_loans,
                    state,
                    instruction.id,
                    origin,
                    errors,
                    "helper Borrow requires proved current callable capture contents",
                );
                if !state.loans.contains(&loan) {
                    errors.push(error(
                        VerifyErrorKind::LoanInactive { loan },
                        location.clone(),
                        origin,
                    ));
                } else if loan_kind(function, loan) == LoanKind::Exclusive
                    && let Some(dependent) = reborrows.active_descendant(loan, aliases, state)
                {
                    errors.push(error(
                        VerifyErrorKind::LoanDependencyActive {
                            parent: loan,
                            dependent,
                        },
                        location.clone(),
                        origin,
                    ));
                }
            }
            EntityId::Place(place) => {
                require_place(place, state, location.clone(), origin, errors);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // 这些手写 IR 负例只交 CI；正常 source/native 本地选择不得包含本模块。
    use super::*;
    use crate::ssa::{
        model::{Origin, Program, SsaTypeKind},
        verify::verify_program,
    };
    use lang_frontend::source::SourceMap;

    fn fixture(copyable: bool, drop_parameters: bool) -> Program {
        fixture_with_prefix(copyable, drop_parameters, |_| {})
    }

    fn fixture_with_prefix(
        copyable: bool,
        drop_parameters: bool,
        prefix: impl FnOnce(&mut Function),
    ) -> Program {
        let mut sources = SourceMap::new();
        let source = sources.add_source("call-contract.ko", "view").unwrap();
        let origin = Origin::Source(sources.span(source, 0, 4).unwrap());
        let mut program = Program::default();
        let module_id = program.add_module("call-contract");
        let module = program.module_mut(module_id).unwrap();
        let string = module.intern_type(SsaTypeKind::StringOwner);
        let value = if copyable {
            module.intern_type(SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
        } else {
            string
        };
        let shared = EntityType::Loan {
            kind: LoanKind::Shared,
            target: string,
        };
        let parameters = vec![
            EntityType::Value(value),
            EntityType::Value(value),
            shared,
            shared,
        ];
        let callee = module
            .add_function("view", vec![string], origin.clone())
            .unwrap();
        let function = module.function_mut(callee).unwrap();
        function.borrow_return = Some(2);
        let entry = function
            .add_block(parameters.clone(), origin.clone())
            .unwrap();
        if !copyable && drop_parameters {
            for index in 0..2 {
                let EntityId::Value(owner) = function.blocks[0].parameters[index] else {
                    unreachable!()
                };
                function
                    .append_instruction(entry, Operation::Drop { owner }, vec![], origin.clone())
                    .unwrap();
            }
        }
        let EntityId::Loan(loan) = function.blocks[0].parameters[2] else {
            unreachable!()
        };
        function
            .set_terminator(entry, TerminatorKind::BorrowReturn { loan }, origin.clone())
            .unwrap();
        let caller = module
            .add_function("caller", vec![], origin.clone())
            .unwrap();
        let function = module.function_mut(caller).unwrap();
        let entry = function.add_block(parameters, origin.clone()).unwrap();
        let arguments = function.blocks[0].parameters.clone();
        let EntityId::Loan(source) = arguments[2] else {
            unreachable!()
        };
        let (_, results) = function
            .append_instruction(
                entry,
                Operation::BorrowCall {
                    callee,
                    arguments,
                    source,
                },
                vec![shared],
                origin.clone(),
            )
            .unwrap();
        let EntityId::Loan(loan) = results[0] else {
            unreachable!()
        };
        function
            .append_instruction(entry, Operation::BorrowEnd { loan }, vec![], origin.clone())
            .unwrap();
        // Prefix operations must be assembled before the Builder seals the block.
        prefix(function);
        function
            .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
            .unwrap();
        program
    }

    fn caller(program: &mut Program) -> &mut Function {
        &mut program.modules[0].functions[1]
    }

    fn arguments(function: &mut Function) -> &mut Vec<EntityId> {
        let Operation::BorrowCall { arguments, .. } = &mut function.instructions[0].operation
        else {
            unreachable!()
        };
        arguments
    }

    fn owner(function: &Function) -> ValueId {
        let EntityId::Value(owner) = function.blocks[0].parameters[0] else {
            unreachable!()
        };
        owner
    }

    fn before_call(
        function: &mut Function,
        operation: Operation,
        results: Vec<EntityType>,
    ) -> Vec<EntityId> {
        let block = function.blocks[0].id;
        let call = function.instructions[0].id;
        let (instruction, results) = function
            .append_instruction(block, operation, results, function.origin.clone())
            .unwrap();
        let instructions = &mut function.blocks[0].instructions;
        assert_eq!(instructions.pop(), Some(instruction));
        let index = instructions.iter().position(|id| *id == call).unwrap();
        instructions.insert(index, instruction);
        results
    }

    fn errors(program: &Program) -> Vec<VerifyError> {
        verify_program(program)
            .expect_err("invalid call contract")
            .errors
    }

    #[test]
    fn verify_mixed_call_ci_only_repeated_copyable_value_is_allowed() {
        let mut program = fixture(true, true);
        let arguments = arguments(caller(&mut program));
        arguments[1] = arguments[0];
        verify_program(&program).unwrap();
    }

    #[test]
    fn verify_mixed_call_ci_only_repeated_move_only_value_is_rejected() {
        let mut program = fixture(false, true);
        let function = caller(&mut program);
        let owner = owner(function);
        let arguments = arguments(function);
        arguments[1] = arguments[0];
        assert!(
            errors(&program)
                .iter()
                .any(|error| { error.kind == VerifyErrorKind::ValueUnavailable { value: owner } })
        );
    }

    #[test]
    fn verify_mixed_call_ci_only_consumed_value_is_rejected() {
        let mut program = fixture_with_prefix(false, true, |function| {
            let owner = owner(function);
            before_call(function, Operation::Drop { owner }, vec![]);
        });
        let function = caller(&mut program);
        let owner = owner(function);
        let call = function.instructions[0].id;
        assert!(errors(&program).iter().any(|error| {
            error.kind == VerifyErrorKind::ValueUnavailable { value: owner }
                && error.location == VerifyLocation::Instruction(call)
        }));
    }

    #[test]
    fn verify_mixed_call_ci_only_inactive_auxiliary_loan_is_rejected() {
        let mut program = fixture_with_prefix(false, true, |function| {
            let EntityId::Loan(loan) = function.blocks[0].parameters[3] else {
                unreachable!()
            };
            before_call(function, Operation::BorrowEnd { loan }, vec![]);
        });
        let function = caller(&mut program);
        let EntityId::Loan(loan) = function.blocks[0].parameters[3] else {
            unreachable!()
        };
        let call = function.instructions[0].id;
        assert!(errors(&program).iter().any(|error| {
            error.kind == VerifyErrorKind::LoanInactive { loan }
                && error.location == VerifyLocation::Instruction(call)
        }));
    }

    #[test]
    fn verify_mixed_call_ci_only_borrowed_owner_cannot_be_consumed() {
        let mut program = fixture_with_prefix(false, true, |function| {
            let owner = owner(function);
            let target = function
                .entity(EntityId::Value(owner))
                .unwrap()
                .ty
                .semantic_type();
            let places = before_call(
                function,
                Operation::RootPlace { owner },
                vec![EntityType::Place(target)],
            );
            let EntityId::Place(place) = places[0] else {
                unreachable!()
            };
            before_call(
                function,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
            );
        });
        let function = caller(&mut program);
        let owner = owner(function);
        let call = function.instructions[0].id;
        assert!(errors(&program).iter().any(|error| {
            error.kind == VerifyErrorKind::OwnerLoanConflict { value: owner }
                && error.location == VerifyLocation::Instruction(call)
        }));
    }

    #[test]
    fn verify_mixed_call_ci_only_callee_owned_exit_is_still_required() {
        let program = fixture(false, false);
        let function = &program.modules[0].functions[0];
        let missing = owner(function);
        assert!(
            errors(&program).iter().any(|error| {
                error.kind == VerifyErrorKind::MissingOwnedExit { value: missing }
            })
        );
    }

    #[test]
    fn verify_mixed_call_ci_only_raw_place_argument_is_rejected() {
        let mut program = fixture_with_prefix(false, true, |function| {
            let owner = owner(function);
            let target = function
                .entity(EntityId::Value(owner))
                .unwrap()
                .ty
                .semantic_type();
            let places = before_call(
                function,
                Operation::RootPlace { owner },
                vec![EntityType::Place(target)],
            );
            arguments(function)[0] = places[0];
        });
        let call = caller(&mut program).instructions[0].id;
        assert!(errors(&program).iter().any(|error| {
            matches!(error.kind, VerifyErrorKind::OperationContract { .. })
                && error.location == VerifyLocation::Instruction(call)
        }));
    }

    #[test]
    fn verify_mixed_call_ci_only_exclusive_auxiliary_parameter_is_rejected() {
        let mut program = fixture(false, true);
        for function in &mut program.modules[0].functions {
            let EntityType::Loan { kind, .. } = &mut function.loans[1].ty else {
                unreachable!()
            };
            *kind = LoanKind::Exclusive;
        }
        assert!(
            errors(&program)
                .iter()
                .any(|error| { matches!(error.kind, VerifyErrorKind::OperationContract { .. }) })
        );
    }

    #[test]
    fn verify_mixed_call_ci_only_owned_source_parameter_is_rejected() {
        let mut program = fixture(false, true);
        program.modules[0].functions[0].borrow_return = Some(0);
        assert!(
            errors(&program)
                .iter()
                .any(|error| { matches!(error.kind, VerifyErrorKind::OperationContract { .. }) })
        );
    }

    #[test]
    fn verify_mixed_call_ci_only_non_abi_value_parameter_is_rejected() {
        let mut program = fixture(true, true);
        program.modules[0].types[1] = SsaTypeKind::Unit;
        assert!(
            errors(&program)
                .iter()
                .any(|error| { matches!(error.kind, VerifyErrorKind::OperationContract { .. }) })
        );
    }
}
