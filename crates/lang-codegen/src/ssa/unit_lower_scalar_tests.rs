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
