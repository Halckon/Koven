use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{
        CheckedArithmeticOperator, ComparisonOperator, EntityId, Function, Operation, Program,
        ScalarConstant, TerminatorKind,
    },
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_same_source_scalar_expression_roots_through_verified_unit_ssa_and_llvm() {
    use lang_frontend::name_resolution::SymbolKind;

    use crate::{
        llvm::render_verified_program,
        native_tests::boxed_enum_tests::unit_source::UnitSource,
        ssa::{model::Origin, verify::verify_program},
    };

    // Keep the original lower_frontend_tests fixture and its legacy oracle unchanged.
    let text = "fun add(left: Int, right: Int): Int = left + right\n\
         fun negate(input: Int): Int = -input\n\
         fun ordered(left: Int, right: Int): Boolean = !(left >= right)\n\
         fun constantOrder(): Boolean = 1 < 2\n\
         fun invoke(first: Int, second: Int): Int = add(right = second, left = first)";
    let unit = UnitSource::analyze("lowering.ko", text).expect("original scalar Unit source");
    unit.assert_frontend_clean();
    assert_eq!(
        unit.sources().source_text(unit.parsed().source_id()),
        Ok(text)
    );

    let mut root_aborts = Vec::new();
    for name in ["add", "negate", "ordered", "constantOrder", "invoke"] {
        let declaration = unit.declaration(name, SymbolKind::Function);
        let origin = Origin::Source(unit.declaration_span(declaration));
        let (program, entry) = unit
            .lower_entry(declaration)
            .expect("each original root must independently lower through Unit");
        verify_program(&program).expect("original root Unit SSA verifies");
        let module = program.module(entry.module()).expect("returned module");
        let function = module.function(entry).expect("returned entry function");
        assert_eq!(
            function.origin, origin,
            "exact original root identity: {name}"
        );
        assert_eq!(
            module
                .functions
                .iter()
                .filter(|f| f.origin == origin)
                .count(),
            1,
            "one exact source function for {name}"
        );

        let ssa = render_program(&program);
        let checked = function
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::CheckedArithmetic { operator, .. } => Some(operator),
                _ => None,
            })
            .collect::<Vec<_>>();
        let expected_checked = match name {
            "add" => vec![CheckedArithmeticOperator::Add],
            "negate" => vec![CheckedArithmeticOperator::Subtract],
            _ => vec![],
        };
        assert_eq!(checked, expected_checked, "{name}: {ssa}");
        assert_eq!(
            assert_borrowed_root_checked_failure_edges(function),
            expected_checked.len()
        );
        let aborts = function
            .blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .filter(|terminator| matches!(terminator.kind, TerminatorKind::Abort))
            .collect::<Vec<_>>();
        for abort in &aborts {
            let Origin::Source(span) = abort.origin else {
                panic!("{name}: original checked Abort must retain source origin");
            };
            let root_span = unit.declaration_span(declaration);
            assert_eq!(span.source_id(), root_span.source_id());
            assert!(root_span.start() <= span.start() && span.end() <= root_span.end());
            assert!(
                function.instructions.iter().any(|instruction| {
                    matches!(instruction.operation, Operation::CheckedArithmetic { .. })
                        && instruction.origin == abort.origin
                }),
                "Abort retains its original checked operation source"
            );
        }
        let aborts = aborts.len();
        assert_eq!(aborts, expected_checked.len(), "{name}: {ssa}");
        // Count each independently selected root once, not invoke's reachable add again.
        root_aborts.push(aborts);
        match name {
            "add" => assert!(ssa.contains("checked.add"), "{ssa}"),
            "negate" => assert!(ssa.contains("checked.sub"), "{ssa}"),
            "ordered" => {
                assert!(function.instructions.iter().any(|instruction| matches!(
                    instruction.operation,
                    Operation::Compare {
                        operator: ComparisonOperator::GreaterThanOrEqual,
                        ..
                    }
                )));
                assert_eq!(
                    function
                        .instructions
                        .iter()
                        .filter(|instruction| matches!(
                            instruction.operation,
                            Operation::BooleanNot { .. }
                        ))
                        .count(),
                    1
                );
                assert!(ssa.contains("cmp.ge"), "{ssa}");
                assert!(ssa.contains("not %v"), "{ssa}");
            }
            "invoke" => {
                let add_origin = Origin::Source(
                    unit.declaration_span(unit.declaration("add", SymbolKind::Function)),
                );
                let callees = module
                    .functions
                    .iter()
                    .filter(|candidate| candidate.origin == add_origin)
                    .collect::<Vec<_>>();
                let [add] = callees.as_slice() else {
                    panic!("invoke must retain the unique original add declaration");
                };
                let calls = function
                    .instructions
                    .iter()
                    .filter_map(|instruction| {
                        if let Operation::DirectCall {
                            callee,
                            receiver,
                            arguments,
                        } = &instruction.operation
                        {
                            Some((callee, receiver, arguments))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                let [(callee, receiver, arguments)] = calls.as_slice() else {
                    panic!("invoke must contain exactly one real direct call");
                };
                assert_eq!(**callee, add.id);
                assert!(receiver.is_none());
                assert_eq!(
                    arguments.as_slice(),
                    function.blocks[0].parameters.as_slice()
                );
                assert_eq!(assert_borrowed_root_checked_failure_edges(add), 1);
            }
            "constantOrder" => {}
            _ => unreachable!("fixed original root list"),
        }

        let (repeated, repeated_entry) = unit
            .lower_entry(declaration)
            .expect("repeat the same original root lowering");
        verify_program(&repeated).expect("repeated Unit SSA verifies");
        assert_eq!(entry.index(), repeated_entry.index());
        assert_eq!(entry.module().index(), repeated_entry.module().index());
        assert_eq!(ssa, render_program(&repeated), "deterministic SSA: {name}");
        // These source roots take arguments and/or return scalars, not a native main ABI.
        let llvm = render_verified_program(&program).expect("original root Unit LLVM verifies");
        assert_eq!(
            llvm,
            render_verified_program(&repeated).expect("repeated Unit LLVM verifies"),
            "deterministic LLVM: {name}"
        );
    }
    assert_eq!(root_aborts, [1, 1, 0, 0, 0]);
    assert_eq!(root_aborts.iter().sum::<usize>(), 2);
}

#[test]
fn lowers_cross_file_scalar_operations_with_checked_failure_edges_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun compute(own left: Int, own right: Int): Boolean = !(-left + right * 2 <= 9)",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Boolean = p.compute(3, 4)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "q", "entry");

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("cross-file scalar operations lower to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("input permutation preserves scalar lowering");
    assert_eq!(render_program(&forward), render_program(&backward));

    let compute = forward.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("p.compute"))
        .expect("reachable provider is lowered");
    let checked = compute
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::CheckedArithmetic { operator, .. } => Some(operator),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        checked,
        vec![
            CheckedArithmeticOperator::Subtract,
            CheckedArithmeticOperator::Multiply,
            CheckedArithmeticOperator::Add,
        ]
    );
    assert!(compute.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::Compare {
            operator: ComparisonOperator::LessThanOrEqual,
            ..
        }
    )));
    assert_eq!(
        compute
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::BooleanNot { .. }))
            .count(),
        1
    );
    assert_eq!(assert_checked_failure_edges(compute), 3);
}

#[test]
fn maps_every_integer_arithmetic_and_comparison_operator() {
    let arithmetic = [
        ("+", CheckedArithmeticOperator::Add),
        ("-", CheckedArithmeticOperator::Subtract),
        ("*", CheckedArithmeticOperator::Multiply),
        ("/", CheckedArithmeticOperator::Divide),
        ("%", CheckedArithmeticOperator::Remainder),
    ];
    for (source_operator, expected) in arithmetic {
        let program = lower_single(&format!(
            "fun entry(own left: Int, own right: Int): Int = left {source_operator} right"
        ));
        assert!(
            program.modules[0].functions[0]
                .instructions
                .iter()
                .any(|instruction| matches!(
                    instruction.operation,
                    Operation::CheckedArithmetic { operator, .. } if operator == expected
                ))
        );
        assert_eq!(
            assert_checked_failure_edges(&program.modules[0].functions[0]),
            1
        );
    }

    let comparisons = [
        ("==", ComparisonOperator::Equal),
        ("!=", ComparisonOperator::NotEqual),
        ("<", ComparisonOperator::LessThan),
        ("<=", ComparisonOperator::LessThanOrEqual),
        (">", ComparisonOperator::GreaterThan),
        (">=", ComparisonOperator::GreaterThanOrEqual),
    ];
    for (source_operator, expected) in comparisons {
        let program = lower_single(&format!(
            "fun entry(own left: Int, own right: Int): Boolean = left {source_operator} right"
        ));
        assert!(
            program.modules[0].functions[0]
                .instructions
                .iter()
                .any(|instruction| matches!(
                    instruction.operation,
                    Operation::Compare { operator, .. } if operator == expected
                ))
        );
    }
}

#[test]
fn folds_the_signed_minimum_literal_without_a_spurious_overflow_edge() {
    let program = lower_single("fun entry(): Int = -2147483648");
    let function = &program.modules[0].functions[0];
    assert!(function.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::Constant(ScalarConstant::Integer(-2_147_483_648))
    )));
    assert!(
        !function.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::CheckedArithmetic { .. }
        ))
    );
    assert!(!function.blocks.iter().any(|block| {
        block
            .terminator
            .as_ref()
            .is_some_and(|terminator| matches!(&terminator.kind, TerminatorKind::Abort))
    }));
}

fn lower_single(source_text: &str) -> Program {
    let mut sources = SourceMap::new();
    let source_text = format!("package test\n{source_text}");
    let (source, file) = parsed(&mut sources, "test/entry.ko", &source_text);
    let inputs = [SourceUnitInput::new("root", "test/entry.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("scalar source lowers to verified SSA")
    .0
}

fn assert_checked_failure_edges(function: &Function) -> usize {
    let mut count = 0;
    for instruction in &function.instructions {
        if !matches!(instruction.operation, Operation::CheckedArithmetic { .. }) {
            continue;
        }
        count += 1;
        let [_, EntityId::Value(failure)] = instruction.results.as_slice() else {
            panic!("checked arithmetic must expose value plus failure flag");
        };
        let block = function
            .block(instruction.block)
            .expect("checked instruction block exists");
        assert_eq!(
            block.instructions.last(),
            Some(&instruction.id),
            "the checked result is consumed by its immediately following terminator"
        );
        let Some(terminator) = &block.terminator else {
            panic!("checked instruction block has a terminator");
        };
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &terminator.kind
        else {
            panic!("checked instruction must end in a conditional");
        };
        assert_eq!(condition, failure, "the failure result is the condition");
        assert!(when_true.arguments.is_empty());
        assert!(when_false.arguments.is_empty());
        assert!(
            function
                .block(when_true.target)
                .and_then(|block| block.terminator.as_ref())
                .is_some_and(|terminator| matches!(&terminator.kind, TerminatorKind::Abort)),
            "failure=true must enter Abort"
        );
        assert!(
            function
                .block(when_false.target)
                .and_then(|block| block.terminator.as_ref())
                .is_some_and(|terminator| !matches!(&terminator.kind, TerminatorKind::Abort)),
            "failure=false must continue"
        );
    }
    count
}

fn assert_borrowed_root_checked_failure_edges(function: &Function) -> usize {
    let mut count = 0;
    for instruction in &function.instructions {
        if !matches!(instruction.operation, Operation::CheckedArithmetic { .. }) {
            continue;
        }
        count += 1;
        let [_, EntityId::Value(failure)] = instruction.results.as_slice() else {
            panic!("checked arithmetic must expose value plus failure flag");
        };
        let block = function
            .block(instruction.block)
            .expect("checked instruction block exists");
        assert_eq!(
            block.instructions.last(),
            Some(&instruction.id),
            "the checked result is consumed by its immediately following terminator"
        );
        let Some(terminator) = &block.terminator else {
            panic!("checked instruction block has a terminator");
        };
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &terminator.kind
        else {
            panic!("checked instruction must end in a conditional");
        };
        assert_eq!(condition, failure, "the failure result is the condition");
        // The unchanged source uses default Borrow parameters. Unlike the existing
        // explicit-own fixtures, Unit must carry these exact entry loans on both edges.
        let parameters = &function.blocks[0].parameters;
        assert!(
            parameters
                .iter()
                .all(|parameter| matches!(parameter, EntityId::Loan(_)))
        );
        assert_eq!(when_true.arguments, when_false.arguments);
        let mut expected_loans = parameters.clone();
        expected_loans.sort();
        let mut actual_loans = when_true.arguments.clone();
        actual_loans.sort();
        assert_eq!(
            actual_loans, expected_loans,
            "carry each original loan exactly once"
        );
        assert_ne!(when_true.target, when_false.target);
        for edge in [when_true, when_false] {
            let target = function
                .block(edge.target)
                .expect("checked edge target exists");
            assert_eq!(edge.arguments.len(), target.parameters.len());
            for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                let argument_type = function.entity(*argument).expect("original loan exists").ty;
                let parameter_type = function
                    .entity(*parameter)
                    .expect("target parameter exists")
                    .ty;
                assert!(matches!(
                    argument_type,
                    super::model::EntityType::Loan {
                        kind: super::model::LoanKind::Shared,
                        ..
                    }
                ));
                assert_eq!(
                    argument_type, parameter_type,
                    "edge preserves the loan type"
                );
            }
        }
        assert!(
            function
                .block(when_true.target)
                .and_then(|block| block.terminator.as_ref())
                .is_some_and(|terminator| matches!(&terminator.kind, TerminatorKind::Abort)),
            "failure=true must enter Abort"
        );
        assert!(
            function
                .block(when_false.target)
                .and_then(|block| block.terminator.as_ref())
                .is_some_and(|terminator| !matches!(&terminator.kind, TerminatorKind::Abort)),
            "failure=false must continue"
        );
    }
    count
}
