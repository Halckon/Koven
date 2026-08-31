use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{EntityId, Function, Operation, SsaTypeKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use crate::llvm::render_verified_program;

#[test]
fn lowers_move_only_enum_root_and_finite_recursion_through_a_class_handle() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         enum class Owned { Item(resource: Resource), Empty }\n\
         enum class Event { Next(owner: Owner), End }\n\
         class Owner(val event: Event)\n\
         fun owned(): Owned = Owned.Item(Resource())\n\
         fun discard(own input: Owned) {}\n\
         fun entry(own event: Event): Unit { discard(owned()) }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/provider.ko",
        provider_source,
        &provider,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("MoveOnly enum and enum-class finite recursion lower to verified SSA");

    let module = &program.modules[0];
    assert_eq!(
        module
            .types
            .iter()
            .filter(|ty| matches!(ty, SsaTypeKind::TaggedUnion { .. }))
            .count(),
        2
    );
    let owned = function(module, "p.owned");
    assert!(owned.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::TaggedConstruct { variant: 0, .. }
    )));
    assert!(
        !owned
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );
    let discard = function(module, "p.discard");
    assert_eq!(
        discard
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1
    );
}

#[test]
fn lowers_empty_case_of_a_move_only_enum_as_the_root_tagged_owner() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/empty-owned.ko",
        "package p\n\
         enum class Owned {\n\
             Empty, Full(text: String);\n\
             own fun score(): Int = 9\n\
         }\n\
         fun empty(): Owned = (Owned.Empty)\n\
         fun consume(own input: Owned): Int = 7\n\
         fun entry(): Int {\n\
             val local = (Owned.Empty)\n\
             val receiver = Owned.Empty\n\
             val groupedFull = (Owned.Full(\"payload\"))\n\
             val fullScore = groupedFull.score()\n\
             if (true) { val checked = fullScore } else { val checked = 0 }\n\
             return consume(local) + consume(empty()) + receiver.score() + fullScore\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/empty-owned.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("empty case of a MoveOnly enum must remain a root tagged owner");

    let module = &program.modules[0];
    let empty_variant_count = module
        .functions
        .iter()
        .flat_map(|function| function.instructions.iter())
        .filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::TaggedConstruct { variant: 0, .. }
            )
        })
        .count();
    assert_eq!(
        empty_variant_count, 3,
        "grouped local/return and Value-receiver Empty constructions"
    );
    let score = function(module, ".Owned.score.s");
    assert_eq!(
        score
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "unused Value receiver is dropped exactly once"
    );
    let tagged = module
        .types
        .iter()
        .enumerate()
        .find_map(|(index, ty)| match ty {
            SsaTypeKind::TaggedUnion { variants, .. } => Some((index, variants)),
            _ => None,
        })
        .expect("Owned root tagged identity");
    let [empty_payload, full_payload] = tagged.1.as_slice() else {
        panic!("Owned has exactly Empty and Full payload layouts");
    };
    let llvm = render_verified_program(&program).expect("MoveOnly Empty/Full LLVM must verify");
    let drop_start = llvm
        .find(&format!("define internal void @koven.drop.t{}", tagged.0))
        .expect("tagged drop glue exists");
    let drop_tail = &llvm[drop_start..];
    let drop_end = drop_tail[1..]
        .find("\ndefine ")
        .map_or(drop_tail.len(), |offset| offset + 1);
    let drop_glue = &drop_tail[..drop_end];
    assert!(drop_glue.contains("switch i32"), "{drop_glue}");
    let empty_case = drop_glue
        .split_once("case0:")
        .and_then(|(_, tail)| tail.split_once("case1:"))
        .map(|(body, _)| body)
        .expect("Empty and Full drop branches exist");
    assert!(
        !empty_case.contains("call void @koven.drop"),
        "Empty payload must not run drop glue:\n{empty_case}"
    );
    assert!(
        !drop_glue.contains(&format!("call void @koven.drop.t{}", empty_payload.index())),
        "Empty aggregate is Copyable and has no drop call:\n{drop_glue}"
    );
    assert_eq!(
        drop_glue
            .matches(&format!("call void @koven.drop.t{}", full_payload.index()))
            .count(),
        1,
        "Full payload is dropped exactly once per tagged owner"
    );
}

#[test]
fn move_only_enum_when_waits_for_branch_qualified_subject_drop_facts() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "test/move-only-when.ko",
        "package test\n\
         class Resource {}\n\
         enum class Owned { Item(resource: Resource), Empty }\n\
         fun entry(own input: Owned): Int = when (input) {\n\
             is Owned.Item -> 1\n\
             is Owned.Empty -> 0\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/move-only-when.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let error = match lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    ) {
        Ok(_) => panic!("MoveOnly enum when must not bypass its AfterExpression drop fact"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

#[test]
fn generic_enum_storage_remains_an_explicit_boundary() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "test/generic.ko",
        "package test\n\
         enum class Maybe<T> { Some(item: T), None }\n\
         fun entry(own input: Maybe<Int>): Int = 0",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/generic.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let error = match lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    ) {
        Ok(_) => panic!("generic enum layout remains outside this slice"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

#[test]
fn lowers_cross_file_enum_construction_when_and_projection_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         enum class Choice { Item(count: Int, extra: Int), Empty }\n\
         fun item(): Choice = Choice.Item(extra = 2, count = 40)\n\
         fun empty(): Choice = Choice.Empty\n\
         fun read(own choice: Choice): Int = when (choice) {\n\
             is Choice.Item -> choice.count + choice.extra\n\
             is Choice.Empty -> 0\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.read(p.item()) + p.read(p.empty())",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (backward_names, backward_typed, backward_owned) =
        analyze(&sources, &reversed, &name_environment, &type_environment);
    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("concrete enum facts lower to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &backward_names,
        &type_environment,
        &backward_typed,
        &backward_owned,
        declaration(&backward_names, "q", "entry"),
    )
    .expect("input permutation preserves enum identities");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    assert_eq!(
        module
            .types
            .iter()
            .filter(
                |ty| matches!(ty, SsaTypeKind::TaggedUnion { variants, .. } if variants.len() == 2)
            )
            .count(),
        1
    );
    let item = function(module, "p.item");
    let (count, extra) = item
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::AggregateConstruct { fields, .. } if fields.len() == 2 => {
                Some((fields[0], fields[1]))
            }
            _ => None,
        })
        .expect("enum payload fields use declaration order");
    let constants = item
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::Constant(_) => Some(instruction.results[0]),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(constants.len(), 2);
    assert_eq!(EntityId::Value(extra), constants[0]);
    assert_eq!(EntityId::Value(count), constants[1]);
    assert!(item.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::TaggedConstruct { variant: 0, .. }
    )));

    let empty = function(module, "p.empty");
    assert!(empty.instructions.iter().any(|instruction| matches!(
        &instruction.operation,
        Operation::AggregateConstruct { fields, .. } if fields.is_empty()
    )));
    assert!(empty.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::TaggedConstruct { variant: 1, .. }
    )));

    let read = function(module, "p.read");
    assert_eq!(
        read.instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::TaggedDiscriminant { .. }
            ))
            .count(),
        2
    );
    assert_eq!(
        read.instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::TaggedPayloadPlace { variant: 0, .. }
            ))
            .count(),
        2
    );
}

fn function<'a>(module: &'a super::model::Module, name: &str) -> &'a Function {
    module
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .unwrap_or_else(|| {
            panic!(
                "function {name} exists among {:?}",
                module
                    .functions
                    .iter()
                    .map(|function| function.name.as_str())
                    .collect::<Vec<_>>()
            )
        })
}
