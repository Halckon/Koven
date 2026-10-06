use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{EntityId, Function, Operation, ScalarConstant, SequentialContainerKind, SsaTypeKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[path = "unit_lower_container_tests/runtime_helpers.rs"]
mod runtime_helpers;

#[path = "unit_lower_container_tests/runtime_matrix.rs"]
mod runtime_matrix;

#[test]
fn runtime_generator_unit_source_accepts_pointer_and_shared_initializers() {
    let mut failures = Vec::new();
    for container in ["Array", "List"] {
        for (environment, initializer) in [
            ("pointer", "{ index -> index }"),
            ("shared", "{ index -> index + scale }"),
        ] {
            let mut sources = SourceMap::new();
            let text = format!(
                "package test\nfun entry(): Int {{ val scale = 7\n\
                 val items = {container}<Int>(3, {initializer})\n\
                 return items[2] }}"
            );
            let (source, parsed) = parsed(&mut sources, "test/runtime.ko", &text);
            let inputs = [SourceUnitInput::new(
                "root",
                "test/runtime.ko",
                source,
                &parsed,
            )];
            let (name_environment, type_environment) = standard_environments();
            let (names, typed, owned) =
                analyze(&sources, &inputs, &name_environment, &type_environment);
            if let Err(error) = lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                declaration(&names, "test", "entry"),
            ) {
                failures.push(format!("{container}/{environment}: {error:?}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "existing synchronous Borrow constructors must reach verified source SSA: {failures:?}"
    );
}

#[test]
fn lowers_cross_file_container_construction_and_owner_transfer_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         fun makeResource(): Resource = Resource()\n\
         fun resources(): List<Resource> = listOf(makeResource(), Resource())\n\
         fun wrap(own resource: Resource): List<Resource> = listOf(resource)\n\
         fun duplicate(own number: Int): List<Int> = listOf(number, number)\n\
         fun produce(): Unit {}\n\
         fun units(): List<Unit> = listOf(produce())\n\
         fun nested(): List<List<Int>> = listOf(listOf(1), listOf(2))\n\
         fun strings(): Array<String> = arrayOf(\"first\", \"second\")\n\
         fun empty(): MutableList<Resource> = MutableList<Resource>()\n\
         fun consume(own items: List<Resource>) {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Unit {\n\
             val consumedResources = p.consume(p.resources())\n\
             val consumedWrapped = p.consume(p.wrap(p.makeResource()))\n\
             val duplicated = p.duplicate(7)\n\
             val units = p.units()\n\
             val nested = p.nested()\n\
             val strings = p.strings()\n\
             val empty = p.empty()\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (reverse_names, reverse_typed, reverse_owned) =
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
    .expect("concrete container construction lowers to verified unit SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &type_environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .expect("input permutation preserves container identities");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let containers = module
        .types
        .iter()
        .filter_map(|ty| match ty {
            SsaTypeKind::SequentialContainer { kind, element } => Some((*kind, *element)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(containers.len(), 6);
    assert_eq!(
        containers
            .iter()
            .filter(|(kind, _)| *kind == SequentialContainerKind::List)
            .count(),
        4
    );
    assert!(
        containers
            .iter()
            .any(|(kind, _)| *kind == SequentialContainerKind::Array)
    );
    assert!(
        containers
            .iter()
            .any(|(kind, _)| *kind == SequentialContainerKind::MutableList)
    );

    let resources = function(module, "p.resources");
    assert_eq!(container_element_counts(resources), [2]);
    assert_eq!(drop_count(resources), 0);
    let wrap = function(module, "p.wrap");
    let wrap_elements = container_elements(wrap);
    let [wrapped] = wrap_elements.as_slice() else {
        panic!("wrap constructs one container");
    };
    let parameter = match wrap.blocks[0].parameters[0] {
        EntityId::Value(value) => value,
        EntityId::Place(_) | EntityId::Loan(_) => panic!("Value parameter expected"),
    };
    assert_eq!(wrapped.as_slice(), [parameter]);
    assert_eq!(
        drop_count(wrap),
        0,
        "the container owns the moved parameter"
    );

    let duplicate = function(module, "p.duplicate");
    let duplicate_elements = container_elements(duplicate);
    let [duplicated] = duplicate_elements.as_slice() else {
        panic!("duplicate constructs one container");
    };
    let number = match duplicate.blocks[0].parameters[0] {
        EntityId::Value(value) => value,
        EntityId::Place(_) | EntityId::Loan(_) => panic!("Value parameter expected"),
    };
    assert_eq!(duplicated.as_slice(), [number, number]);
    assert_eq!(
        drop_count(duplicate),
        0,
        "Copy delivery preserves the source"
    );

    let units = function(module, "p.units");
    let call_index = units
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .expect("the Unit-producing call remains observable");
    let (constant_index, unit_value) = units
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match instruction.operation {
            Operation::Constant(ScalarConstant::Unit) => match instruction.results.as_slice() {
                [EntityId::Value(value)] => Some((index, *value)),
                _ => None,
            },
            _ => None,
        })
        .expect("Unit is materialized as a container element value");
    let (construct_index, unit_elements) = units
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match &instruction.operation {
            Operation::ContainerConstruct { elements, .. } => Some((index, elements)),
            _ => None,
        })
        .expect("the Unit list is constructed");
    assert!(call_index < constant_index && constant_index < construct_index);
    assert_eq!(unit_elements.as_slice(), [unit_value]);

    let nested = function(module, "p.nested");
    assert_eq!(container_element_counts(nested), [1, 1, 2]);
    assert_eq!(
        drop_count(nested),
        0,
        "inner containers transfer into the outer owner"
    );
    let strings = function(module, "p.strings");
    assert_eq!(container_element_counts(strings), [2]);
    assert_eq!(
        strings
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
            .count(),
        2
    );
    assert_eq!(
        drop_count(strings),
        0,
        "String elements transfer into the array"
    );
    assert_eq!(container_element_counts(function(module, "p.empty")), [0]);
    assert_eq!(drop_count(function(module, "p.consume")), 1);
    assert_eq!(drop_count(function(module, "q.entry")), 5);
}

#[test]
fn runtime_length_container_and_unsupported_element_remain_explicit_boundaries() {
    for (name, text) in [
        (
            "test/runtime.ko",
            "package test\n\
             fun entry(size: Int, initializer: (Int) -> Int): List<Int> =\n\
                 List<Int>(size, initializer)",
        ),
        (
            "test/float.ko",
            "package test\nfun entry(): List<Double> = listOf(1.0)",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(&mut sources, name, text);
        let inputs = [SourceUnitInput::new("root", name, source, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let error = match lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        ) {
            Ok(_) => panic!("{name} remains outside the concrete container core"),
            Err(error) => error,
        };
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{name}");
    }
}

#[test]
fn lowers_cross_file_mutable_list_add_to_container_append() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun create(): MutableList<Int> = mutableListOf(10)",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.create\n\
         fun entry(): Int {\n\
             var list = create()\n\
             list.add(20)\n\
             list.add(30)\n\
             return list[1]\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("lowering succeeds");
    let func = function(&program.modules[0], "entry");
    let append_count = func
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::ContainerAppend { .. }))
        .count();
    assert_eq!(append_count, 2);
}

#[test]
fn mutable_list_clear_unit_lowers_to_container_clear() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun create(): MutableList<Int> {\n\
             return mutableListOf(10, 20)\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.create\n\
         fun entry(): Int {\n\
             var list = create()\n\
             list.clear()\n\
             return list.size\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("lowering succeeds");
    let func = function(&program.modules[0], "entry");
    let clear_count = func
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::ContainerClear { .. }))
        .count();
    assert_eq!(clear_count, 1);
}

#[test]
fn mutable_list_remove_at_compilation_unit_lowering_creates_container_remove_at() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun create(): MutableList<Int> { return mutableListOf(10, 20) }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.create\n\
         fun entry(): Int {\n\
             var list = create()\n\
             val item = list.removeAt(0)\n\
             return item\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("lowering succeeds");
    let func = function(&program.modules[0], "entry");
    let remove_at_count = func
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::ContainerRemoveAt { .. }))
        .count();
    assert_eq!(remove_at_count, 1);
}

#[test]
fn mutable_list_remove_last_compilation_unit_lowering_creates_container_remove_last() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun create(): MutableList<Int> { return mutableListOf(10, 20) }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.create\n\
         fun entry(): Int {\n\
             var list = create()\n\
             val item = list.removeLast()\n\
             return item\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("lowering succeeds");
    let func = function(&program.modules[0], "entry");
    let remove_last_count = func
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(instruction.operation, Operation::ContainerRemoveLast { .. })
        })
        .count();
    assert_eq!(remove_last_count, 1);
}

fn function<'a>(module: &'a super::model::Module, name: &str) -> &'a Function {
    module
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .unwrap_or_else(|| panic!("reachable function {name} exists"))
}

fn container_elements(function: &Function) -> Vec<Vec<super::model::ValueId>> {
    function
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::ContainerConstruct { elements, .. } => Some(elements.clone()),
            _ => None,
        })
        .collect()
}

fn container_element_counts(function: &Function) -> Vec<usize> {
    container_elements(function).iter().map(Vec::len).collect()
}

fn drop_count(function: &Function) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .count()
}
