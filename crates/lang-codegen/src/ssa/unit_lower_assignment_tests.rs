use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{CheckedArithmeticOperator, EntityId, Operation, ScalarConstant, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn replaces_a_string_owner_only_after_the_rhs_finishes() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun suffix(): String = \"!\"\n\
         fun identity(own input: String): String = input",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): String {\n\
             var current = \"old\"\n\
             { current = current + p.suffix() }\n\
             return p.identity(current)\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry_declaration = declaration(&names, "q", "entry");

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry_declaration,
    )
    .expect("root String assignment lowers to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry_declaration,
    )
    .expect("input permutation preserves assignment lowering");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let entry = module
        .functions
        .iter()
        .find(|function| function.name.contains("q.entry"))
        .expect("entry function exists");
    let suffix = module
        .functions
        .iter()
        .find(|function| function.name.contains("p.suffix"))
        .expect("suffix function exists")
        .id;
    let identity = module
        .functions
        .iter()
        .find(|function| function.name.contains("p.identity"))
        .expect("identity function exists")
        .id;
    let old = entry
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::StringLiteral { bytes, .. } if bytes == b"old" => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .expect("old owner exists");
    let suffix_result = entry
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } if callee == suffix => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .expect("RHS suffix result exists");
    let (concat_index, replacement) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match instruction.operation {
            Operation::StringConcat { left, right } if [left, right] == [old, suffix_result] => {
                Some((index, instruction.results[0]))
            }
            _ => None,
        })
        .expect("RHS reads the old owner before replacement");
    assert!(matches!(
        entry.instructions[concat_index + 1].operation,
        Operation::Drop { owner } if EntityId::Value(owner) == suffix_result
    ));
    assert!(matches!(
        entry.instructions[concat_index + 2].operation,
        Operation::Drop { owner } if EntityId::Value(owner) == old
    ));

    let (identity_index, identity_result) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match &instruction.operation {
            Operation::DirectCall {
                callee, arguments, ..
            } if *callee == identity => {
                Some((index, (arguments.as_slice(), instruction.results[0])))
            }
            _ => None,
        })
        .expect("identity call exists");
    assert_eq!(identity_index, concat_index + 3);
    assert_eq!(identity_result.0, &[replacement]);
    assert!(
        !entry.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::Drop { owner } if EntityId::Value(owner) == replacement
        )),
        "the replacement owner transfers through the updated binding"
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::Drop { owner } => Some(EntityId::Value(owner)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![suffix_result, old]
    );
    let EntityId::Value(identity_result) = identity_result.1 else {
        panic!("identity returns a value");
    };
    let returned = entry
        .blocks
        .iter()
        .find_map(
            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Return { values }) => Some(values.as_slice()),
                _ => None,
            },
        )
        .expect("entry returns identity result");
    assert_eq!(returned, &[identity_result]);
}

#[test]
fn updates_the_binding_after_each_checked_compound_assignment() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun update(own input: Int): Int {\n\
             var total = input\n\
             { total += 2 }\n\
             { total -= 3 }\n\
             { total *= 4 }\n\
             { total /= 5 }\n\
             { total %= 6 }\n\
             return total\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.update(7)",
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
    .expect("compound assignments lower to verified checked CFG");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("input permutation preserves compound assignment lowering");
    assert_eq!(render_program(&forward), render_program(&backward));

    let update = forward.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("p.update"))
        .expect("update function exists");
    let checked = update
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::CheckedArithmetic { .. }))
        .collect::<Vec<_>>();
    assert_eq!(checked.len(), 5);
    assert_eq!(
        checked
            .iter()
            .map(|instruction| match instruction.operation {
                Operation::CheckedArithmetic { operator, .. } => operator,
                _ => unreachable!(),
            })
            .collect::<Vec<_>>(),
        vec![
            CheckedArithmeticOperator::Add,
            CheckedArithmeticOperator::Subtract,
            CheckedArithmeticOperator::Multiply,
            CheckedArithmeticOperator::Divide,
            CheckedArithmeticOperator::Remainder,
        ]
    );
    let EntityId::Value(mut expected_left) = update
        .block(update.entry_block().expect("entry block exists"))
        .expect("entry block exists")
        .parameters[0]
    else {
        panic!("input parameter is a value");
    };
    for (index, instruction) in checked.iter().enumerate() {
        let Operation::CheckedArithmetic { left, right, .. } = instruction.operation else {
            unreachable!();
        };
        assert_eq!(
            left, expected_left,
            "each assignment reads the updated binding"
        );
        let literal = update
            .instructions
            .iter()
            .find_map(|candidate| match candidate.operation {
                Operation::Constant(ScalarConstant::Integer(value))
                    if value == index as i128 + 2 =>
                {
                    Some(candidate.results[0])
                }
                _ => None,
            })
            .expect("compound RHS literal exists");
        assert_eq!(EntityId::Value(right), literal);
        let [_, EntityId::Value(failure)] = instruction.results.as_slice() else {
            panic!("checked assignment exposes value and failure");
        };
        let TerminatorKind::Conditional {
            condition,
            when_true,
            ..
        } = &update
            .block(instruction.block)
            .expect("checked block exists")
            .terminator
            .as_ref()
            .expect("checked block terminates")
            .kind
        else {
            panic!("checked assignment branches on failure");
        };
        assert_eq!(condition, failure);
        assert!(
            update
                .block(when_true.target)
                .and_then(|block| block.terminator.as_ref())
                .is_some_and(|terminator| matches!(terminator.kind, TerminatorKind::Abort))
        );
        let EntityId::Value(result) = instruction.results[0] else {
            panic!("checked assignment result is a value");
        };
        expected_left = result;
    }
    let EntityId::Value(final_result) = checked[4].results[0] else {
        panic!("final compound assignment produces a value");
    };
    let returned = update
        .blocks
        .iter()
        .find_map(
            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Return { values }) if !values.is_empty() => {
                    Some(values.as_slice())
                }
                _ => None,
            },
        )
        .expect("update returns its final binding");
    assert_eq!(returned, &[final_result]);
}

#[test]
fn keeps_undefined_compound_assignment_types_outside_this_slice() {
    let source_text = "package p\n\
         fun entry(): Unit {\n\
             var text = \"left\"\n\
             { text += \"right\" }\n\
         }";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "p/main.ko", source_text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let error = match lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    ) {
        Err(error) => error,
        Ok(_) => panic!("undefined compound assignment remains an explicit boundary"),
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}
