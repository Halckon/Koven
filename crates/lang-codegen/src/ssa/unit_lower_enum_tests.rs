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
fn move_only_enum_when_snapshots_tag_before_subject_drop() {
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
    let (program, entry) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("a tag snapshot must precede the published subject cleanup");
    super::verify::verify_program(&program).unwrap();
    render_verified_program(&program).unwrap();
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    let tags = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| match instruction.operation {
            Operation::TaggedDiscriminant { owner } => Some((index, owner)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tags.len(), 1, "later case tests must reuse the saved tag");
    let (tag_index, owner) = tags[0];
    let drops = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            matches!(instruction.operation, Operation::Drop { owner: dropped } if dropped == owner)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1, "the original owner is retired exactly once");
    let entry_block = function.entry_block().unwrap();
    assert_eq!(function.instructions[tag_index].block, entry_block);
    assert_eq!(function.instructions[drops[0]].block, entry_block);
    let execution_order = &function.blocks[entry_block.index()].instructions;
    let position = |index| {
        execution_order
            .iter()
            .position(|id| id.index() == index)
            .unwrap()
    };
    assert!(
        position(tag_index) < position(drops[0]),
        "tag must be read before owner cleanup in the executed block order"
    );
}

#[test]
fn generic_enum_owned_parameter_reaches_verified_llvm() {
    verified_generic_enum_source(
        "test/generic.ko",
        "package test\n\
         enum class Maybe<T> { Some(item: T), None }\n\
         fun entry(own input: Maybe<Int>): Int = 0",
        "entry",
    );
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

#[test]
fn guide_litmus_04_unit_enum_expression_condition_reaches_verified_llvm() {
    let mut sources = SourceMap::new();
    let text = crate::native_tests::return_control_tests::guide_litmus_04();
    let (source, parsed) = parsed(&mut sources, "demo/enums/litmus4.ko", text);
    let inputs = [SourceUnitInput::new(
        "root",
        "demo/enums/litmus4.ko",
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
        declaration(&names, "demo.enums", "area"),
    )
    .expect("original Guide area accepts its payloadless enum case condition");
    super::verify::verify_program(&program).expect("original Guide area reaches verified SSA");
    render_verified_program(&program).expect("original Guide area reaches verified LLVM");
}

const ORIGINAL_GENERIC_NOMINAL: &str =
    include_str!("../../tests/fixtures/unit-generic-enum-0321/generic_nominal.ko");
const ORIGINAL_GENERIC_FUNCTIONS: &str =
    include_str!("../../tests/fixtures/unit-generic-enum-0321/original_generic_functions.ko");

/// Original source/path/entry, with complete admission before ordinary ownership.
/// Reuse parsed(), the public frontend chain and the existing verifier; no fact injection.
fn with_generic_enum_source<R>(
    path: &str,
    text: &str,
    entry_name: &str,
    check: impl FnOnce(
        &lang_frontend::ownership_checking::OwnedCompilationUnitView<'_, '_>,
        lang_frontend::name_resolution::DeclarationId,
    ) -> R,
) -> R {
    use lang_frontend::{
        name_resolution::{SymbolKind, index_compilation_unit, resolve_compilation_unit_names},
        ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
        type_checking::{check_compilation_unit_types, complete_compilation_unit_types},
    };
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(&mut sources, path, text);
    let inputs = [SourceUnitInput::new("root", path, source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("original unit index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("original unit names")
        .validate()
        .expect("original names validate");
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .expect("original unit types");
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw)
        .expect("original complete type admission");
    let typed = raw.validate().expect("original types validate");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("original ordinary ownership");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let owned = owned.validate().expect("original ownership validates");
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|entry| {
            entry.name() == entry_name
                && entry.kind() == SymbolKind::Function
                && entry.name_span().source_id() == source
                && parsed.roots().contains(&entry.root())
        })
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        panic!("exact original top-level entry {entry_name}: {entries:?}");
    };
    let view = owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
        .expect("ordinary owned view");
    check(&view, entry.id())
}

fn verified_generic_enum_source(path: &str, text: &str, entry: &str) -> super::model::Program {
    with_generic_enum_source(path, text, entry, |view, entry| {
        let (program, _) = super::unit_lower::lower_owned_unit_with_entry(view, entry)
            .expect("complete original generic enum source must lower");
        super::verify::verify_program(&program).expect("generic enum SSA verifies");
        render_verified_program(&program).expect("generic enum LLVM verifies");
        program
    })
}

#[test]
fn original_generic_nominal_source_reaches_verified_llvm() {
    verified_generic_enum_source("nominal-run.ko", ORIGINAL_GENERIC_NOMINAL, "bootstrap");
}

#[test]
fn original_generic_enum_return_reaches_verified_llvm() {
    let program = verified_generic_enum_source("lowering.ko", ORIGINAL_GENERIC_FUNCTIONS, "some");
    let module = &program.modules[0];
    let returned = function(module, ".some");
    let tagged = returned.return_types[0];
    assert!(matches!(
        module.type_kind(tagged),
        Some(SsaTypeKind::TaggedUnion { .. })
    ));
    assert!(returned.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::TaggedConstruct { tagged: actual, variant: 0, .. } if actual == tagged
    )));
}

#[test]
fn original_generic_enum_projection_reaches_verified_llvm() {
    let program =
        verified_generic_enum_source("lowering.ko", ORIGINAL_GENERIC_FUNCTIONS, "enumProjected");
    let projected = function(&program.modules[0], ".enumProjected");
    assert_eq!(
        projected
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::TaggedPayloadPlace { variant: 0, .. }
            ))
            .count(),
        1
    );
}

#[test]
fn original_generic_enum_root_drop_reaches_verified_llvm() {
    use super::model::{EntityType, Ownership};
    let program =
        verified_generic_enum_source("lowering.ko", ORIGINAL_GENERIC_FUNCTIONS, "dropEnum");
    let module = &program.modules[0];
    let dropped = function(module, ".dropEnum");
    let (root, tagged) = dropped
        .instructions
        .iter()
        .find_map(|instruction| {
            if let Operation::TaggedConstruct {
                tagged, variant: 0, ..
            } = instruction.operation
            {
                Some((instruction.results[0], tagged))
            } else {
                None
            }
        })
        .expect("original Owned<Resource> constructs its root");
    assert_eq!(module.type_ownership(tagged), Some(Ownership::MoveOnly));
    assert_eq!(
        dropped.entity(root).map(|entity| entity.ty),
        Some(EntityType::Value(tagged))
    );
    let drops = dropped
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::Drop { owner } => Some(EntityId::Value(owner)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(drops, vec![root], "the original owner has one root Drop");
    let Some(SsaTypeKind::TaggedUnion { variants, .. }) = module.type_kind(tagged) else {
        panic!("Owned<Resource> is tagged storage");
    };
    let Some(SsaTypeKind::Aggregate { fields, .. }) = module.type_kind(variants[0]) else {
        panic!("Some payload is an aggregate");
    };
    assert_eq!(fields.len(), 1);
    assert!(matches!(
        module.type_kind(fields[0]),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    let llvm = render_verified_program(&program).expect("root drop LLVM verifies");
    let drop_glue = |ty: super::model::SsaTypeId| {
        let start = llvm
            .find(&format!(
                "define internal void @koven.drop.t{}(",
                ty.index()
            ))
            .expect("concrete drop glue exists");
        let tail = &llvm[start..];
        let end = tail[1..]
            .find("\ndefine ")
            .map_or(tail.len(), |offset| offset + 1);
        &tail[..end]
    };
    let tagged_drop = drop_glue(tagged);
    assert!(
        tagged_drop.contains("switch i32"),
        "active case drop dispatch"
    );
    assert_eq!(
        tagged_drop
            .matches(&format!("call void @koven.drop.t{}(", variants[0].index()))
            .count(),
        1
    );
    assert!(!tagged_drop.contains(&format!("call void @koven.drop.t{}(", variants[1].index())));
    let payload_drop = drop_glue(variants[0]);
    assert_eq!(
        payload_drop
            .matches(&format!("call void @koven.drop.t{}(", fields[0].index()))
            .count(),
        1
    );
    assert_eq!(
        drop_glue(fields[0]).matches("call void @free(").count(),
        1,
        "the concrete Resource heap owner glue releases its allocation"
    );
}

#[test]
fn generic_enum_instances_keep_layout_and_named_argument_identity() {
    use super::model::ScalarConstant;
    let text = include_str!("../../tests/fixtures/unit-generic-enum-0321/two_instances.ko");
    with_generic_enum_source("test/two-instances.ko", text, "entry", |view, entry| {
        // Descriptor parameter order is first, second; source evaluation order is second, first.
        let constructors = view
            .types()
            .constructions()
            .iter()
            .filter(|descriptor| descriptor.arguments().len() == 2)
            .collect::<Vec<_>>();
        assert_eq!(
            constructors.len(),
            2,
            "both named concrete constructors are present"
        );
        for descriptor in constructors {
            let arguments = descriptor.arguments();
            assert_eq!(arguments[0].parameter_name(), "first");
            assert_eq!(arguments[0].parameter_index(), 0);
            assert_eq!(arguments[0].evaluation_index(), 1);
            assert_eq!(arguments[1].parameter_name(), "second");
            assert_eq!(arguments[1].parameter_index(), 1);
            assert_eq!(arguments[1].evaluation_index(), 0);
        }
        let (program, _) = super::unit_lower::lower_owned_unit_with_entry(view, entry)
            .expect("both concrete instances must lower together");
        super::verify::verify_program(&program).expect("two-instance SSA verifies");
        render_verified_program(&program).expect("two-instance LLVM verifies");
        let module = &program.modules[0];
        let mut tagged_instances = Vec::new();
        for (name, bits, first, second) in [
            ("test.ints", 32, 40, 2),
            ("test.longs", 64, 5_000_000_000, 3),
        ] {
            let constructor = function(module, name);
            let tagged = constructor.return_types[0];
            let empty = function(
                module,
                if bits == 32 {
                    "test.emptyInts"
                } else {
                    "test.emptyLongs"
                },
            );
            assert_eq!(empty.return_types, vec![tagged]);
            assert!(empty.instructions.iter().any(|instruction| matches!(
                instruction.operation,
                Operation::TaggedConstruct { tagged: actual, variant: 1, .. } if actual == tagged
            )));
            tagged_instances.push(tagged);
            let Some(SsaTypeKind::TaggedUnion { variants, .. }) = module.type_kind(tagged) else {
                panic!("concrete return must be tagged");
            };
            assert_eq!(variants.len(), 2, "both cases must have layouts");
            let Some(SsaTypeKind::Aggregate { fields: types, .. }) = module.type_kind(variants[0])
            else {
                panic!("Item payload must have concrete storage");
            };
            assert_eq!(types.len(), 2);
            for ty in types {
                assert_eq!(
                    module.type_kind(*ty),
                    Some(&SsaTypeKind::Integer { bits, signed: true })
                );
            }
            assert!(matches!(module.type_kind(variants[1]),
                Some(SsaTypeKind::Aggregate { fields, .. }) if fields.is_empty()));
            let fields = constructor
                .instructions
                .iter()
                .find_map(|instruction| {
                    if let Operation::AggregateConstruct { aggregate, fields } =
                        &instruction.operation
                    {
                        (*aggregate == variants[0]).then_some(fields)
                    } else {
                        None
                    }
                })
                .expect("Item construction uses the selected concrete payload");
            let constants = constructor
                .instructions
                .iter()
                .filter_map(|instruction| {
                    if let Operation::Constant(ScalarConstant::Integer(value)) =
                        instruction.operation
                    {
                        Some((instruction.results[0], value))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(
                constants,
                vec![
                    (EntityId::Value(fields[1]), second),
                    (EntityId::Value(fields[0]), first),
                ],
                "second is evaluated first, first second; fields retain declaration order"
            );
        }
        assert_ne!(
            tagged_instances[0], tagged_instances[1],
            "Int and Long never share a layout"
        );
    });
}

#[test]
fn original_generic_nominal_source_runs_natively() {
    with_generic_enum_source(
        "nominal-run.ko",
        ORIGINAL_GENERIC_NOMINAL,
        "bootstrap",
        |view, entry| {
            let directory = crate::native_tests::TestDirectory::create();
            let object = directory.join("nominal.o");
            let executable = directory.join("nominal");
            crate::emit_native_owned_unit_object(view, entry, &object)
                .expect("complete original generic source emits an ordinary unit object");
            let linked = std::process::Command::new(crate::test_support::clang())
                .arg(&object)
                .arg("-o")
                .arg(&executable)
                .output()
                .expect("clang launches");
            assert!(linked.status.success(), "{linked:?}");
            let run = std::process::Command::new(&executable)
                .output()
                .expect("native executable launches");
            // The original single exact only asserts success; stdout/stderr are not historical goldens.
            assert!(run.status.success(), "{run:?}");
        },
    );
}

#[test]
fn owned_generic_enum_when_rebinds_subject_and_borrows_string_payload() {
    let source = r#"enum class Reply<T> { Full(value: T), Empty }
fun show(own item: Reply<String>): Unit {
    when (item) {
        is Reply.Full -> println(item.value)
        is Reply.Empty -> println("empty")
    }
    when (item) {
        is Reply.Empty -> println("empty again")
        is Reply.Full -> println(item.value)
    }
}
fun local(): Unit {
    val item: Reply<String> = Reply.Full("local".clone())
    when ((item)) {
        is Reply.Empty -> error("wrong local case")
        is Reply.Full -> println(item.value)
    }
}
fun early(own item: Reply<String>): Int {
    when (item) {
        is Reply.Empty -> return 8
        is Reply.Full -> println(item.value)
    }
    return 7
}
fun main(): Unit {
    val full: Reply<String> = Reply.Full("payload".clone())
    val empty: Reply<String> = Reply.Empty
    show(full)
    show(empty)
    local()
    val earlyFull: Reply<String> = Reply.Full("early".clone())
    val earlyEmpty: Reply<String> = Reply.Empty
    if (early(earlyFull) != 7) { error("wrong full return") }
    if (early(earlyEmpty) != 8) { error("wrong empty return") }
}"#;
    with_generic_enum_source("owned-when.ko", source, "main", |view, entry| {
        let (program, entry) = super::unit_lower::lower_owned_unit_with_entry(view, entry)
            .expect("owned enum subject survives each condition edge and both when joins");
        super::verify::verify_program(&program).expect("no stale or duplicated enum owner");
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .expect("borrowed String payload has the ordinary String ABI");
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 3);
        crate::native_tests::boxed_enum_tests::assert_success(
            &run,
            b"payload\npayload\nempty\nempty again\nlocal\nearly\n",
        );
    });
}

#[test]
fn owned_enum_when_keeps_temporary_subject_cleanup_rejected() {
    for subject in ["make()", "((make()))"] {
        let source = format!(
            "enum class Owned {{ Full(value: String), Empty }}\n\
             fun make(): Owned = Owned.Full(\"value\")\n\
             fun entry(own item: Owned): Int = when ({subject}) {{\n\
                 is Owned.Full -> 1\n\
                 is Owned.Empty -> 0\n\
             }}"
        );
        with_generic_enum_source("rejected-when.ko", &source, "entry", |view, entry| {
            let error = match super::unit_lower::lower_owned_unit_with_entry(view, entry) {
                Ok(_) => panic!("temporary enum owners still need an explicit subject plan"),
                Err(error) => error,
            };
            assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{subject}");
        });
    }
}

#[test]
fn exhaustive_owned_enum_statement_when_returns_without_a_fallthrough_edge() {
    let source = r#"enum class Reply<T> { Full(value: T), Empty }
fun total(own item: Reply<String>): Int {
    when (item) {
        is Reply.Full -> { println(item.value); return 7 }
        is Reply.Empty -> return 8
    }
}
fun partial(own item: Reply<String>): Int {
    when (item) {
        is Reply.Full -> { println(item.value); return 9 }
    }
    println("fallthrough")
    return 10
}
fun main(): Unit {
    val full: Reply<String> = Reply.Full("total".clone())
    val empty: Reply<String> = Reply.Empty
    if (total(full) != 7) { error("total full") }
    if (total(empty) != 8) { error("total empty") }
    val partialFull: Reply<String> = Reply.Full("partial".clone())
    val partialEmpty: Reply<String> = Reply.Empty
    if (partial(partialFull) != 9) { error("partial full") }
    if (partial(partialEmpty) != 10) { error("partial empty") }
}"#;
    with_generic_enum_source("returning-when.ko", source, "main", |view, entry| {
        let (program, entry) = super::unit_lower::lower_owned_unit_with_entry(view, entry)
            .expect("typed non-fallthrough when must not fabricate a normal exit");
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 2);
        crate::native_tests::boxed_enum_tests::assert_success(
            &run,
            b"total\npartial\nfallthrough\n",
        );
    });
}
