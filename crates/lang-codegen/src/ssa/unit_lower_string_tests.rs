use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{EntityId, Operation, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn returns_literal_concat_after_dropping_only_its_operand_owners() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "test/entry.ko",
        "package test\nfun entry(): String = \"left\" + \"right\"",
    );
    let inputs = [SourceUnitInput::new("root", "test/entry.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("literal concat lowers to verified SSA");

    let function = &program.modules[0].functions[0];
    let literals = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
        .map(|instruction| instruction.results[0])
        .collect::<Vec<_>>();
    assert_eq!(literals.len(), 2);
    let (concat_index, concat_result) = function
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match instruction.operation {
            Operation::StringConcat { left, right }
                if [left, right] == [literals[0], literals[1]] =>
            {
                Some((index, instruction.results[0]))
            }
            _ => None,
        })
        .expect("concat reads both literal owners in source order");
    assert_eq!(
        drop_owners(&function.instructions[concat_index + 1..concat_index + 3]),
        vec![literals[1], literals[0]],
        "temporary String operands drop in reverse evaluation order"
    );
    let returned = function
        .blocks
        .iter()
        .find_map(
            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Return { values }) => Some(values.as_slice()),
                _ => None,
            },
        )
        .expect("entry returns the concat owner");
    let EntityId::Value(concat_result) = concat_result else {
        panic!("concat result is a value");
    };
    assert_eq!(returned, &[concat_result]);
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        2,
        "the returned concat owner is not dropped"
    );
}

#[test]
fn lowers_cross_file_concat_equality_and_value_delivery_deterministically() {
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
         fun entry(): Boolean {\n\
             val left = \"K\"\n\
             val joined = left + p.suffix()\n\
             val same = joined == \"K!\"\n\
             val different = joined != \"other\"\n\
             val consumed = p.identity(joined)\n\
             return same == different\n\
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
    .expect("cross-file String binary expressions lower");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry_declaration,
    )
    .expect("input permutation preserves String lowering");
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
    let suffix_result = entry
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } if callee == suffix => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .expect("suffix call result exists");
    let left = string_literal_result(entry, b"K");
    let expected = string_literal_result(entry, b"K!");
    let other = string_literal_result(entry, b"other");
    let (concat_index, joined) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match instruction.operation {
            Operation::StringConcat {
                left: actual_left,
                right: actual_right,
            } if [actual_left, actual_right] == [left, suffix_result] => {
                Some((index, instruction.results[0]))
            }
            _ => None,
        })
        .expect("concat reads local and call result in source order");
    assert_eq!(
        drop_owners(&entry.instructions[concat_index + 1..concat_index + 3]),
        vec![suffix_result, left]
    );

    let equalities = entry
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, instruction)| matches!(instruction.operation, Operation::StringEqual { .. }))
        .collect::<Vec<_>>();
    assert_eq!(equalities.len(), 2);
    assert!(matches!(
        equalities[0].1.operation,
        Operation::StringEqual { left, right } if [left, right] == [joined, expected]
    ));
    assert_eq!(
        drop_owners(&entry.instructions[equalities[0].0 + 1..equalities[0].0 + 2]),
        vec![expected]
    );
    assert!(matches!(
        equalities[1].1.operation,
        Operation::StringEqual { left, right } if [left, right] == [joined, other]
    ));
    assert_eq!(
        drop_owners(&entry.instructions[equalities[1].0 + 1..equalities[1].0 + 2]),
        vec![other]
    );
    let (not_index, not) = entry
        .instructions
        .iter()
        .enumerate()
        .find(|(_, instruction)| matches!(instruction.operation, Operation::BooleanNot { .. }))
        .expect("not-equal negates StringEqual");
    assert_eq!(not_index, equalities[1].0 + 2);
    let EntityId::Value(second_equal) = equalities[1].1.results[0] else {
        panic!("StringEqual result is Boolean");
    };
    assert!(matches!(
        not.operation,
        Operation::BooleanNot { operand } if operand == second_equal
    ));
    let identity_arguments = entry
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::DirectCall { callee, arguments } if *callee == identity => {
                Some(arguments.as_slice())
            }
            _ => None,
        })
        .expect("identity call exists");
    assert_eq!(identity_arguments, &[joined]);
    assert!(
        !entry.instructions[..concat_index]
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );
}

fn string_literal_result(function: &super::model::Function, bytes: &[u8]) -> EntityId {
    function
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::StringLiteral { bytes: actual, .. } if actual == bytes => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .expect("expected String literal exists")
}

fn drop_owners(instructions: &[super::model::Instruction]) -> Vec<EntityId> {
    instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::Drop { owner } => Some(EntityId::Value(owner)),
            _ => None,
        })
        .collect()
}
