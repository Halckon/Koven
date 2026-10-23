//! SPEC-0306: inspect unchanged Programs lowered from real mixed-mode source.
use super::{
    borrow_memory_test_support::single, borrow_storage_contract_tests::unit, model::*,
    verify::verify_program,
};
use crate::native_mixed_borrow_cases::{Case, Contract};
use std::collections::BTreeSet;

fn end(function: &Function, loan: LoanId) -> usize {
    let ends = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ends.len(),
        1,
        "{}: exactly one end for {loan:?}",
        function.name
    );
    ends[0]
}

fn producer(function: &Function, entity: EntityId) -> Option<&Instruction> {
    match function.entity(entity).unwrap().definition {
        Definition::InstructionResult { instruction, .. } => function.instruction(instruction),
        Definition::BlockParameter { .. } => None,
    }
}

fn created_by_call(function: &Function, call: &Instruction, loan: LoanId) -> bool {
    let Some(producer) = producer(function, EntityId::Loan(loan)) else {
        return false;
    };
    let created = producer.origin.span();
    let call = call.origin.span();
    created.source_id() == call.source_id()
        && call.start() <= created.start()
        && created.end() <= call.end()
}

fn has_later_read(function: &Function, loan: LoanId, call_index: usize) -> bool {
    function.instructions[call_index + 1..end(function, loan)]
        .iter()
        .any(|instruction| {
            !matches!(instruction.operation, Operation::BorrowEnd { .. })
                && instruction
                    .operation
                    .entities()
                    .contains(&EntityId::Loan(loan))
        })
}

fn loan_parent(function: &Function, loan: LoanId) -> Option<LoanId> {
    match &producer(function, EntityId::Loan(loan))?.operation {
        Operation::SharedReborrow { source } | Operation::BorrowCall { source, .. } => {
            Some(*source)
        }
        Operation::SharedFieldLoan { base, .. } | Operation::SharedHeapFieldLoan { base, .. } => {
            Some(*base)
        }
        _ => None,
    }
}

fn loan_root(function: &Function, mut loan: LoanId) -> LoanId {
    let mut seen = BTreeSet::new();
    while let Some(parent) = loan_parent(function, loan) {
        assert!(seen.insert(loan), "source dependency must be acyclic");
        loan = parent;
    }
    loan
}

fn source_owner(function: &Function, loan: LoanId) -> Option<ValueId> {
    let root = loan_root(function, loan);
    let Operation::BorrowBegin { place, .. } = producer(function, EntityId::Loan(root))?.operation
    else {
        return None;
    };
    match producer(function, EntityId::Place(place))?.operation {
        Operation::RootPlace { owner } => Some(owner),
        _ => None,
    }
}

fn check_contract(module: &Module, function: &Function, expected: &Contract) {
    assert_eq!(function.borrow_return, Some(expected.source_index));
    assert!(function.receiver.is_none());
    assert!(function.carrier_return.is_none());
    assert_eq!(function.blocks.len(), 1, "bounded straight-line borrower");
    let parameters = &function.blocks[0].parameters;
    assert_eq!(parameters.len(), expected.parameter_count);
    for (index, parameter) in parameters.iter().enumerate() {
        let ty = function.entity(*parameter).unwrap().ty;
        if expected.value_indices.contains(&index) {
            assert!(
                matches!((parameter, ty), (EntityId::Value(_), EntityType::Value(_))),
                "{} parameter {index} must retain Value delivery",
                function.name
            );
        } else {
            assert!(
                matches!(
                    (parameter, ty),
                    (
                        EntityId::Loan(_),
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            ..
                        }
                    )
                ),
                "{} parameter {index} must retain shared Loan delivery",
                function.name
            );
        }
    }
    let EntityId::Loan(source) = parameters[expected.source_index] else {
        panic!("the unique source must be an actual Loan parameter");
    };
    let TerminatorKind::BorrowReturn { loan } =
        function.blocks[0].terminator.as_ref().unwrap().kind
    else {
        panic!("mixed callee must return a loan");
    };
    assert_eq!(
        loan_root(function, loan),
        source,
        "return must derive from the exact source parameter"
    );
    let drops = function
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::Drop { owner } = instruction.operation {
                Some(EntityId::Value(owner))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let expected_drops = expected
        .dropped_indices
        .iter()
        .map(|index| parameters[*index])
        .filter(|parameter| {
            let ty = function.entity(*parameter).unwrap().ty.semantic_type();
            module.type_ownership(ty) == Some(Ownership::MoveOnly)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        drops, expected_drops,
        "{}: exact callee-owned cleanup in declared reverse order",
        function.name
    );
    for index in expected.value_indices {
        let parameter = parameters[*index];
        let ty = function.entity(parameter).unwrap().ty.semantic_type();
        if module.type_ownership(ty) == Some(Ownership::MoveOnly) {
            let transfers = function
                .instructions
                .iter()
                .filter(|instruction| {
                    matches!(&instruction.operation,
                Operation::BorrowCall { arguments, .. } | Operation::DirectCall { arguments, .. }
                    if arguments.contains(&parameter))
                })
                .count();
            let cleanups = drops.iter().filter(|owner| **owner == parameter).count();
            assert_eq!(
                transfers + cleanups,
                1,
                "MoveOnly parameter is forwarded or dropped exactly once"
            );
        } else {
            assert!(
                !drops.contains(&parameter),
                "Copyable Value has no owner drop"
            );
        }
    }
    assert!(
        !function.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::StringClone { .. } | Operation::SharedRetain { .. }
        )),
        "borrowing must not hide an owned clone/retain"
    );
}

fn check_function(module: &Module, function: &Function) {
    let is_entry = function.name.contains("entry");
    let parameters = &function.blocks[0].parameters;
    let mut active = parameters
        .iter()
        .filter_map(|entity| {
            if let EntityId::Loan(loan) = entity {
                Some(*loan)
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();
    for (index, instruction) in function.instructions.iter().enumerate() {
        for entity in instruction.operation.entities() {
            if let EntityId::Loan(loan) = entity {
                assert!(
                    active.contains(&loan),
                    "{}: every actual loan operand must be live",
                    function.name
                );
            }
        }
        if let Operation::BorrowCall {
            callee,
            arguments,
            source,
        } = &instruction.operation
        {
            let target = module.function(*callee).unwrap();
            let target_parameters = &target.blocks[0].parameters;
            assert_eq!(arguments.len(), target_parameters.len());
            assert_eq!(
                arguments[target.borrow_return.unwrap()],
                EntityId::Loan(*source)
            );
            for (argument, parameter) in arguments.iter().zip(target_parameters) {
                assert_eq!(
                    function.entity(*argument).unwrap().ty,
                    target.entity(*parameter).unwrap().ty,
                    "caller operand and callee mode/type must match exactly"
                );
                if let EntityId::Value(value) = argument {
                    let ty = function.entity(*argument).unwrap().ty.semantic_type();
                    if module.type_ownership(ty) == Some(Ownership::MoveOnly) {
                        assert!(
                            function.instructions[index + 1..].iter().all(|later| {
                                !later
                                    .operation
                                    .entities()
                                    .contains(&EntityId::Value(*value))
                            }),
                            "transferred Value owner must never be read or dropped by caller again"
                        );
                    }
                }
            }
            let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                panic!("BorrowCall returns exactly one Loan");
            };
            assert_ne!(*result, *source);
            if is_entry {
                assert!(index < end(function, *result));
                assert!(end(function, *result) < end(function, *source));
                for argument in arguments {
                    if let EntityId::Loan(loan) = argument {
                        assert!(index < end(function, *loan));
                        if !created_by_call(function, instruction, *loan) {
                            assert!(
                                has_later_read(function, *loan, index),
                                "reused external loan must remain live for a real later read"
                            );
                        } else if loan != source {
                            assert!(
                                end(function, *loan) < end(function, *result),
                                "only a newly created unrelated Borrow ends before returned loan"
                            );
                        }
                    }
                }
                if let Some(owner) = source_owner(function, *source) {
                    for (later_index, later) in
                        function.instructions.iter().enumerate().skip(index + 1)
                    {
                        let consumes = match &later.operation {
                            Operation::Drop { owner: dropped } => *dropped == owner,
                            Operation::DirectCall { arguments, .. }
                            | Operation::BorrowCall { arguments, .. } => {
                                arguments.contains(&EntityId::Value(owner))
                            }
                            _ => false,
                        };
                        if consumes {
                            assert!(
                                end(function, *source) < later_index,
                                "result and source end before their exact root owner is consumed"
                            );
                        }
                    }
                }
            } else {
                assert!(
                    !function
                        .instructions
                        .iter()
                        .any(|later| matches!(later.operation,
                    Operation::BorrowEnd { loan } if loan == *source || loan == *result)),
                    "wrapper forwards source and result through BorrowReturn"
                );
                for argument in arguments {
                    if let EntityId::Loan(loan) = argument
                        && loan != source
                        && created_by_call(function, instruction, *loan)
                    {
                        assert!(index < end(function, *loan));
                        if let Some(owner) = source_owner(function, *loan) {
                            for (drop_index, later) in function.instructions.iter().enumerate() {
                                if matches!(later.operation, Operation::Drop { owner: dropped } if dropped == owner)
                                {
                                    assert!(
                                        end(function, *loan) < drop_index,
                                        "wrapper auxiliary reborrow ends before its own owner is dropped"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Operation::BorrowEnd { loan } = instruction.operation {
            assert!(active.remove(&loan), "a loan ends once and only while live");
        }
        active.extend(instruction.results.iter().filter_map(|entity| {
            if let EntityId::Loan(loan) = entity {
                Some(*loan)
            } else {
                None
            }
        }));
    }
    if is_entry {
        assert!(
            active.is_empty(),
            "caller must explicitly end all source, result and child loans"
        );
        for instruction in &function.instructions {
            for entity in &instruction.results {
                if let EntityId::Loan(loan) = entity
                    && let Some(parent) = loan_parent(function, *loan)
                {
                    assert!(
                        end(function, *loan) < end(function, parent),
                        "aliases/projections end strictly before their real parent"
                    );
                }
            }
        }
    }
}

fn check(program: &Program, case: &Case) {
    verify_program(program).unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
    crate::llvm::render_verified_program(program).unwrap();
    let module = &program.modules[0];
    let borrowers = module
        .functions
        .iter()
        .filter(|function| function.borrow_return.is_some())
        .collect::<Vec<_>>();
    assert_eq!(
        borrowers.len(),
        case.contracts
            .iter()
            .map(|contract| contract.instances)
            .sum::<usize>()
    );
    for expected in case.contracts {
        let functions = borrowers
            .iter()
            .copied()
            .filter(|function| function.name.contains(expected.name))
            .collect::<Vec<_>>();
        assert_eq!(
            functions.len(),
            expected.instances,
            "{}: exact named callee instantiations",
            case.name
        );
        if expected.instances > 1 {
            let ownerships = functions
                .iter()
                .map(|function| {
                    let parameter = function.blocks[0].parameters[expected.value_indices[0]];
                    let ty = function.entity(parameter).unwrap().ty.semantic_type();
                    module.type_ownership(ty).unwrap()
                })
                .collect::<Vec<_>>();
            assert!(ownerships.contains(&Ownership::Copyable));
            assert!(ownerships.contains(&Ownership::MoveOnly));
        }
        for function in functions {
            check_contract(module, function, expected);
        }
    }
    for function in &module.functions {
        if function.borrow_return.is_some() || function.name.contains("entry") {
            check_function(module, function);
        }
    }
    if case.name == "same_type_different_roots" {
        let entry = module
            .functions
            .iter()
            .find(|function| function.name.contains("entry"))
            .unwrap();
        let call = entry
            .instructions
            .iter()
            .find(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
            .unwrap();
        let Operation::BorrowCall {
            arguments, source, ..
        } = &call.operation
        else {
            unreachable!()
        };
        let EntityId::Loan(other) = arguments[0] else {
            panic!("other is Borrow")
        };
        let EntityId::Value(transferred) = arguments[2] else {
            panic!("transferred is Value")
        };
        let source_owner = source_owner(entry, *source).unwrap();
        let other_owner = self::source_owner(entry, other).unwrap();
        assert_ne!(source_owner, other_owner);
        assert_ne!(source_owner, transferred);
        assert_ne!(other_owner, transferred);
        assert_eq!(
            entry.entity(arguments[0]).unwrap().ty,
            entry.entity(EntityId::Loan(*source)).unwrap().ty,
            "same type is intentionally insufficient to choose the source root"
        );
    }
}

#[test]
fn mixed_value_borrow_single_source_matrix() {
    for case in crate::native_mixed_borrow_cases::cases() {
        let program =
            single(case.source).unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        check(&program, &case);
    }
}

#[test]
fn mixed_value_borrow_unit_source_matrix() {
    for case in crate::native_mixed_borrow_cases::cases() {
        let program = unit(case.source).unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        check(&program, &case);
    }
}

fn value_root(function: &Function, mut value: ValueId) -> ValueId {
    while let Some(Instruction {
        operation: Operation::Copy { source },
        ..
    }) = producer(function, EntityId::Value(value))
    {
        value = *source;
    }
    value
}

fn check_generic_consumers(program: &Program) {
    verify_program(program).unwrap();
    crate::llvm::render_verified_program(program).unwrap();
    let module = &program.modules[0];
    assert!(
        module
            .functions
            .iter()
            .all(|function| function.borrow_return.is_none())
    );
    for function in &module.functions {
        for instruction in &function.instructions {
            if let Operation::Drop { owner } = instruction.operation {
                let ty = function
                    .entity(EntityId::Value(owner))
                    .unwrap()
                    .ty
                    .semantic_type();
                assert_eq!(
                    module.type_ownership(ty),
                    Some(Ownership::MoveOnly),
                    "generic cleanup must never drop a concrete Copyable value"
                );
            }
        }
    }
    for name in ["unused", "forward", "readThenReturn", "readTwice"] {
        let functions = module
            .functions
            .iter()
            .filter(|function| function.name.contains(name))
            .collect::<Vec<_>>();
        assert_eq!(
            functions.len(),
            3,
            "{name}: Int, Resource and direct String instances"
        );
        let mut ownerships = Vec::new();
        let mut string_instances = 0;
        for function in functions {
            let [EntityId::Value(parameter)] = function.blocks[0].parameters.as_slice() else {
                panic!("ordinary own T parameter retains Value delivery");
            };
            let ty = function
                .entity(EntityId::Value(*parameter))
                .unwrap()
                .ty
                .semantic_type();
            let ownership = module.type_ownership(ty).unwrap();
            ownerships.push(ownership);
            string_instances += usize::from(matches!(
                module.type_kind(ty),
                Some(SsaTypeKind::StringOwner)
            ));
            let drops = function.instructions.iter().filter(|instruction| {
                matches!(instruction.operation, Operation::Drop { owner } if owner == *parameter)
            }).count();
            assert_eq!(
                drops,
                usize::from(
                    ownership == Ownership::MoveOnly && matches!(name, "unused" | "readTwice")
                ),
                "{name}: only the final MoveOnly owner is dropped once"
            );
            if matches!(name, "readThenReturn" | "readTwice") {
                let reads = function
                    .instructions
                    .iter()
                    .enumerate()
                    .filter_map(|(index, instruction)| {
                        let Operation::DirectCall {
                            callee, arguments, ..
                        } = &instruction.operation
                        else {
                            return None;
                        };
                        if !module.function(*callee).unwrap().name.contains("observe") {
                            return None;
                        }
                        let [EntityId::Loan(loan)] = arguments.as_slice() else {
                            panic!("observe must borrow the original generic parameter");
                        };
                        assert_eq!(source_owner(function, *loan), Some(*parameter));
                        assert!(index < end(function, *loan));
                        Some(index)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(reads.len(), if name == "readTwice" { 2 } else { 1 });
                for (index, instruction) in function.instructions.iter().enumerate() {
                    if matches!(instruction.operation, Operation::Drop { owner } if owner == *parameter)
                    {
                        assert!(reads.iter().all(|read| *read < index));
                    }
                }
            }
            if name == "readThenReturn" {
                assert!(function.blocks.iter().any(|block| {
                    matches!(&block.terminator.as_ref().unwrap().kind,
                        TerminatorKind::Return { values }
                            if values.iter().any(|value| value_root(function, *value) == *parameter))
                }), "read must leave the same binding available for return");
            }
            if name == "forward" {
                let transfers = function.instructions.iter().filter(|instruction| {
                    matches!(&instruction.operation, Operation::DirectCall { callee, arguments, .. }
                        if module.function(*callee).unwrap().name.contains("unused")
                            && arguments.iter().any(|argument| matches!(argument,
                                EntityId::Value(value) if value_root(function, *value) == *parameter)))
                }).count();
                assert_eq!(transfers, 1, "ordinary generic forwarding happens once");
            }
        }
        assert_eq!(string_instances, 1, "{name}: direct String specialization");
        assert!(ownerships.contains(&Ownership::Copyable));
        assert!(ownerships.contains(&Ownership::MoveOnly));
    }
}

#[test]
fn mixed_value_borrow_generic_ordinary_consumers_single() {
    let case = crate::native_mixed_borrow_cases::generic_consumers();
    check_generic_consumers(&single(case.source).unwrap());
}

#[test]
fn mixed_value_borrow_generic_ordinary_consumers_unit() {
    let case = crate::native_mixed_borrow_cases::generic_consumers();
    check_generic_consumers(&unit(case.source).unwrap());
}

// Group is transparent for provenance, but cleanup belongs to the outer expression ID.
fn check_group_drop(program: &Program, value_origin: lang_frontend::source::Span) {
    verify_program(program).unwrap();
    let drops = program.modules[0]
        .functions
        .iter()
        .filter(|function| function.borrow_return.is_some())
        .flat_map(|function| function.instructions.iter())
        .filter(|instruction| {
            matches!(instruction.operation, Operation::Drop { .. })
                && instruction.origin.span() == value_origin
        })
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        1,
        "the exact grouped-return owner cleanup reaches SSA once"
    );
    crate::llvm::render_verified_program(program).unwrap();
}

#[test]
fn mixed_value_borrow_grouped_return_single_keeps_outer_control_transfer_identity() {
    use lang_frontend::{
        lexer::lex,
        name_resolution::resolve_names,
        ownership_checking::{DropPoint, check_ownership},
        parser::{Expression, parse_file},
        source::SourceMap,
        type_checking::{check_types, standard_environments},
    };
    for case in crate::native_mixed_borrow_cases::cases()
        .into_iter()
        .filter(|case| case.name.contains("grouped"))
    {
        let mut sources = SourceMap::new();
        let source = sources.add_source(case.name, case.source).unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &file, &environment).unwrap();
        let typed = check_types(&sources, &file, &names, &types).unwrap();
        let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let grouped = owned
            .borrow_return_origins()
            .iter()
            .filter(|fact| {
                matches!(
                    file.ast()
                        .expressions()
                        .get(fact.expression())
                        .unwrap()
                        .payload(),
                    Expression::Group { .. }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(grouped.len(), 1);
        let outer = grouped[0].expression();
        let Expression::Group { expression: inner } =
            file.ast().expressions().get(outer).unwrap().payload()
        else {
            unreachable!()
        };
        assert_ne!(outer, *inner);
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == DropPoint::ControlTransfer(outer))
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1, "cleanup uses actual outer Group identity");
        assert!(
            !owned
                .drops()
                .iter()
                .any(|fact| fact.point() == DropPoint::ControlTransfer(*inner))
        );
        let program = super::lower_frontend::orchestrate::lower_scalar_file(
            &sources, &file, &names, &typed, &owned,
        )
        .unwrap();
        check_group_drop(&program, drops[0].value_origin());
        check(&program, &case);
    }
}

#[test]
fn mixed_value_borrow_grouped_return_unit_keeps_source_qualified_outer_identity() {
    use lang_frontend::{
        name_resolution::SourceUnitInput,
        ownership_checking::UnitDropPoint,
        parser::Expression,
        source::SourceMap,
        type_checking::{UnitExpressionId, standard_environments},
    };
    for case in crate::native_mixed_borrow_cases::cases()
        .into_iter()
        .filter(|case| case.name.contains("grouped"))
    {
        let (provider_text, entry_text) = case.source.split_once("fun entry()").unwrap();
        let mut sources = SourceMap::new();
        let (p, provider) = super::unit_lower_test_support::parsed(
            &mut sources,
            "provider.ko",
            &format!("package p\n{provider_text}"),
        );
        let (q, entry) = super::unit_lower_test_support::parsed(
            &mut sources,
            "entry.ko",
            &format!("package q\nimport p.*\nfun entry(){entry_text}"),
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", p, &provider),
            SourceUnitInput::new("root", "q/entry.ko", q, &entry),
        ];
        let (environment, types) = standard_environments();
        let (names, typed, owned) =
            super::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let source_unit = names
            .names()
            .index()
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == p)
            .unwrap()
            .id();
        let grouped = owned
            .ownership()
            .borrow_return_origins()
            .iter()
            .filter(|fact| {
                fact.expression().source_unit() == source_unit
                    && matches!(
                        provider
                            .ast()
                            .expressions()
                            .get(fact.expression().expression())
                            .unwrap()
                            .payload(),
                        Expression::Group { .. }
                    )
            })
            .collect::<Vec<_>>();
        assert_eq!(grouped.len(), 1);
        let outer = grouped[0].expression();
        let Expression::Group { expression: inner } = provider
            .ast()
            .expressions()
            .get(outer.expression())
            .unwrap()
            .payload()
        else {
            unreachable!()
        };
        let inner = UnitExpressionId::new(source_unit, *inner);
        assert_ne!(outer, inner);
        let drops = owned
            .ownership()
            .drops()
            .iter()
            .filter(|fact| fact.point() == UnitDropPoint::ControlTransfer(outer))
            .collect::<Vec<_>>();
        assert_eq!(
            drops.len(),
            1,
            "unit cleanup keeps source-qualified outer Group identity"
        );
        assert!(
            !owned
                .ownership()
                .drops()
                .iter()
                .any(|fact| fact.point() == UnitDropPoint::ControlTransfer(inner))
        );
        let (program, _) = super::unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            super::unit_lower_test_support::declaration(&names, "q", "entry"),
        )
        .unwrap();
        check_group_drop(&program, drops[0].value_origin());
        check(&program, &case);
    }
}
