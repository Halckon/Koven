use super::*;

#[test]
fn intrinsic_container_constructions_publish_stable_unit_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "p/models.ko",
        "package p\nvalue class Resource(val id: Int)",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "p/uses.ko",
        "package p\n\
         fun build(size: Int): Unit {\n\
             val inferred = listOf(Resource(1), Resource(2))\n\
             val expected: List<Int> = listOf()\n\
             val nullable: List<Int?> = listOf(null)\n\
             val explicit = arrayOf<Long>()\n\
             val initializer: (Int) -> Int = { index -> index }\n\
             val array = Array<Int>(size, initializer)\n\
             val list = List<Int>(size, initializer)\n\
             val mutable = MutableList<Int>()\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("core container constructions are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed core container constructions are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(
        forward.container_constructions(),
        reverse.container_constructions()
    );
    assert_eq!(forward.container_constructions().len(), 7);
    assert_eq!(
        forward
            .container_constructions()
            .iter()
            .map(|descriptor| (descriptor.kind(), descriptor.container()))
            .collect::<Vec<_>>(),
        [
            (
                ContainerConstructionKind::ListForm,
                SequentialContainerKind::List,
            ),
            (
                ContainerConstructionKind::ListForm,
                SequentialContainerKind::List,
            ),
            (
                ContainerConstructionKind::ListForm,
                SequentialContainerKind::List,
            ),
            (
                ContainerConstructionKind::ListForm,
                SequentialContainerKind::Array,
            ),
            (
                ContainerConstructionKind::RuntimeLength,
                SequentialContainerKind::Array,
            ),
            (
                ContainerConstructionKind::RuntimeLength,
                SequentialContainerKind::List,
            ),
            (
                ContainerConstructionKind::EmptyMutableList,
                SequentialContainerKind::MutableList,
            ),
        ]
    );
    assert_eq!(
        forward.container_constructions()[0].parameter_modes(),
        [ParameterMode::Value, ParameterMode::Value]
    );
    assert_eq!(
        forward.container_constructions()[4].parameter_modes(),
        [ParameterMode::Borrow, ParameterMode::Borrow]
    );
    for descriptor in forward.container_constructions() {
        assert_eq!(
            forward.expression_type(descriptor.expression()),
            Some(descriptor.container_type())
        );
        assert_eq!(
            forward.expression_category(descriptor.expression()),
            Some(ExpressionCategory::Temporary)
        );
    }
    let resource = declaration(&forward_names, "Resource");
    assert!(matches!(
        forward
            .types()
            .get(forward.container_constructions()[0].element_type()),
        Some(UnitTypeKind::Nominal { declaration, .. }) if *declaration == resource
    ));
    assert!(forward.validate().is_ok());
}

#[test]
fn invalid_intrinsic_container_constructions_keep_diagnostics_and_no_facts() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-containers.ko",
        "fun invalid(size: Boolean): Unit {\n\
             val empty = listOf()\n\
             val absent = listOf(null)\n\
             val mixed = listOf(1, true)\n\
             val initializer: (Int) -> Int = { index -> index }\n\
             val runtime = Array<Int>(size, initializer)\n\
             val wrongMutable = MutableList<Int>(1)\n\
             val tooMany = listOf<Int, Long>()\n\
             val named = listOf(element = 1)\n\
             val marked = listOf(&1)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-containers.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid core container calls stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0126", "L0126", "L0084", "L0084", "L0127", "L0091", "L0120", "L0122"
        ]
    );
    assert!(typed.container_constructions().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn intrinsic_container_overload_trial_commits_only_the_unique_fact() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "container-trial.ko",
        "fun choose(action: () -> List<Int>): Int\n\
         fun choose(action: () -> List<String>): String\n\
         fun selected(): Int = choose({ listOf<Int>() })",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "container-trial.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("one container-returning lambda candidate is valid");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.container_constructions().len(), 1);
    assert_eq!(
        typed.container_constructions()[0].element_type(),
        typed.types().builtin(BuiltinType::Int).expect("Int")
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn source_container_names_never_gain_intrinsic_construction_identity() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "source-containers.ko",
        "class List<T>(val item: T)\n\
         fun listOf(input: Int): Int = input\n\
         fun source(): List<Int> = List(listOf(1))\n\
         fun sourceIndex(items: List<Int>): Unit { val item = items[0] }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "source-containers.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("source container names use ordinary source identities");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.container_constructions().is_empty());
    assert!(typed.element_places().is_empty());
    assert_eq!(typed.constructions().len(), 1);
    assert!(matches!(
        typed.constructions()[0].target(),
        UnitConstructionTarget::Nominal(_)
    ));
    assert_eq!(typed.calls().len(), 1);
    assert!(matches!(
        typed.calls()[0].target(),
        UnitCallTarget::Declaration(_)
    ));
    assert!(typed.validate().is_ok());
}

#[test]
fn intrinsic_container_places_members_and_assignments_publish_stable_unit_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "p/models.ko",
        "package p\nvalue class Resource(val id: Int)",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "p/uses.ko",
        "package p\n\
         fun mutate(inout input: Resource): Unit {}\n\
         fun operate(\n\
             items: List<Resource>, array: Array<Resource>,\n\
             mutable: MutableList<Resource>, replacement: Resource\n\
         ): Unit {\n\
             val first: Resource = items[0]\n\
             val replaced = (array[0] = replacement)\n\
             val changed = (mutable[0] = replacement)\n\
             val count: Int = items.size\n\
             val borrowed = mutate(&(array[0]))\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("intrinsic container places and members are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed intrinsic container places are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(forward.element_places(), reverse.element_places());
    assert_eq!(forward.element_places().len(), 4);
    assert_eq!(
        forward
            .element_places()
            .iter()
            .map(|place| (place.container(), place.is_mutable()))
            .collect::<Vec<_>>(),
        [
            (SequentialContainerKind::List, false),
            (SequentialContainerKind::Array, true),
            (SequentialContainerKind::MutableList, true),
            (SequentialContainerKind::Array, true),
        ]
    );
    let resource = declaration(&forward_names, "Resource");
    for place in forward.element_places() {
        assert_eq!(
            place.expression().source_unit(),
            place.receiver().source_unit()
        );
        assert_eq!(
            place.expression().source_unit(),
            place.index().source_unit()
        );
        assert_eq!(
            forward.expression_type(place.expression()),
            Some(place.element_type())
        );
        assert_eq!(
            forward.expression_category(place.expression()),
            Some(ExpressionCategory::Place)
        );
        assert!(matches!(
            forward.types().get(place.element_type()),
            Some(UnitTypeKind::Nominal { declaration, .. }) if *declaration == resource
        ));
    }
    let size = expression_with_text(&sources, &uses, "items.size");
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(
            source_unit(&forward_names, uses_source),
            size,
        )),
        forward.types().builtin(BuiltinType::Int)
    );
    assert_eq!(
        forward.expression_category(UnitExpressionId::new(
            source_unit(&forward_names, uses_source),
            size,
        )),
        Some(ExpressionCategory::Temporary)
    );
    assert_eq!(forward.calls().len(), 1);
    assert_eq!(
        forward.calls()[0].arguments()[0].category(),
        ExpressionCategory::Place
    );
    assert!(forward.validate().is_ok());
}

#[test]
fn invalid_intrinsic_container_operations_keep_exact_diagnostics_and_no_partial_calls() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-container-operations.ko",
        "fun mutate(inout input: Int): Unit {}\n\
         fun invalid(list: List<Int>, array: Array<Int>, strings: Array<String>): Unit {\n\
             val badIndex = list[true]\n\
             val replacement = (list[0] = 1)\n\
             val groupedReplacement = ((list[0]) = 1)\n\
             val resize = (list.size = 2)\n\
             val get = list.get(0)\n\
             val set = list.set(0, 1)\n\
             val borrowed = mutate(&list[0])\n\
             val groupedBorrowed = mutate(&(list[0]))\n\
             val borrowedSize = mutate(&list.size)\n\
             val changed = mutate(&array[0])\n\
             val compound = (strings[0] += \"x\")\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-container-operations.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid container operations remain in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0128", "L0129", "L0129", "L0129", "L0130", "L0130", "L0122", "L0122", "L0122",
            "L0085"
        ]
    );
    assert_eq!(typed.calls().len(), 1);
    assert_eq!(typed.element_places().len(), 6);
    assert!(typed.validate().is_err());
}

#[test]
fn intrinsic_container_place_overload_trial_commits_only_the_unique_fact() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "container-place-trial.ko",
        "fun choose(action: () -> Int): Int\n\
         fun choose(action: () -> String): String\n\
         fun selected(items: List<Int>): Int = choose({ items[0] })",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "container-place-trial.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("one element-place lambda candidate is valid");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.element_places().len(), 1);
    assert_eq!(
        typed.element_places()[0].element_type(),
        typed.types().builtin(BuiltinType::Int).expect("Int")
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn poisoned_intrinsic_container_element_places_defer_without_place_facts() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "poisoned-container-place.ko",
        "fun bad(items: List<Opaque>): Opaque = items[0]",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "poisoned-container-place.ko",
        source,
        &file,
    )];
    let (mut name_environment, type_environment) = standard_environments();
    name_environment
        .declare_type("Opaque")
        .expect("external type name is unique");
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("poisoned element types preserve a deferred recovery boundary");
    let expression = expression_with_text(&sources, &file, "items[0]");
    assert!(matches!(
        typed
            .expression_type(UnitExpressionId::new(
                source_unit(&names, source),
                expression
            ))
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Deferred(DeferredReason::Index))
    ));
    assert!(typed.element_places().is_empty());
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.validate().is_ok());
}

#[test]
fn mutable_list_add_member_is_typed_in_compilation_unit() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "container-append.ko",
        "fun append(own list: MutableList<Int>): Unit { list.add(42) }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "container-append.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("MutableList.add is valid");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.container_appends().len(), 1);
    let append = typed.container_appends()[0];
    assert_eq!(
        typed.types().get(append.result_type()),
        Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
    );
    assert!(typed.validate().is_ok());
}
