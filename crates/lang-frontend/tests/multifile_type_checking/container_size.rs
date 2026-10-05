use super::*;

#[test]
fn container_size_facts_are_source_qualified_and_input_order_independent() {
    let mut sources = SourceMap::new();
    let (a_source, a) = parsed(
        &mut sources,
        "p/a.ko",
        "package p\nfun arraySize(values: Array<Int>): Int = values.size",
    );
    let (b_source, b) = parsed(
        &mut sources,
        "p/b.ko",
        "package p\n\
         fun sizes(own list: List<Int>, mutable: MutableList<Int>): Unit {\n\
             val first = (list).size\n\
             val second = mutable.size\n\
             val third = listOf(1).size\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a.ko", a_source, &a),
        SourceUnitInput::new("root", "p/b.ko", b_source, &b),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    let reversed_inputs = [inputs[1], inputs[0]];
    let reversed_names = validated_names(&sources, &reversed_inputs, &name_environment);
    let reverse = check_compilation_unit_types(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
    )
    .unwrap();
    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(forward.container_sizes(), reverse.container_sizes());
    assert_eq!(forward.container_sizes().len(), 4);
    for descriptor in forward.container_sizes() {
        assert_eq!(
            forward.container_size(descriptor.expression()),
            Some(*descriptor)
        );
        assert_eq!(
            descriptor.expression().source_unit(),
            descriptor.receiver().source_unit()
        );
        assert_eq!(
            forward.expression_type(descriptor.receiver()),
            Some(descriptor.container_type())
        );
        assert_eq!(
            forward.expression_type(descriptor.expression()),
            Some(descriptor.result_type())
        );
        assert_eq!(
            forward.expression_category(descriptor.expression()),
            Some(ExpressionCategory::Temporary)
        );
        assert_eq!(
            forward.types().get(descriptor.result_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert!(sources.slice(descriptor.span()).unwrap().ends_with(".size"));
        let file = if descriptor.expression().source_unit() == source_unit(&names, a_source) {
            &a
        } else {
            &b
        };
        assert_eq!(
            file.ast()
                .expressions()
                .get(descriptor.expression().expression())
                .unwrap()
                .span(),
            descriptor.span()
        );
    }
}

#[test]
fn container_size_candidate_trial_does_not_duplicate_the_selected_fact() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "size-trial.ko",
        "fun choose(callback: (Int) -> Int): Int = 1\n\
         fun choose(callback: (String) -> Int): Int = 2\n\
         fun use(values: List<Int>): Int = choose({ index -> values.size + index })",
    );
    let inputs = [SourceUnitInput::new("root", "size-trial.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.container_sizes().len(), 1);
}

#[test]
fn same_named_user_field_is_not_a_container_size_intrinsic() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "user-size.ko",
        "value class Counter(val size: Int)\nfun use(counter: Counter): Int = counter.size",
    );
    let inputs = [SourceUnitInput::new("root", "user-size.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.container_sizes().is_empty());
    assert_eq!(typed.aggregate_projections().len(), 1);
}

#[test]
fn generic_container_size_keeps_its_unerased_element_type() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "generic-size.ko",
        "fun <T> size(values: List<T>): Int = values.size\n\
         fun use(values: List<Int>): Int = size(values)",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "generic-size.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.container_sizes().len(), 1);
    assert!(matches!(
        typed.types().get(typed.container_sizes()[0].element_type()),
        Some(UnitTypeKind::TypeParameter(_))
    ));
    assert_eq!(
        typed
            .calls()
            .last()
            .unwrap()
            .instance()
            .type_arguments()
            .len(),
        1
    );
}
