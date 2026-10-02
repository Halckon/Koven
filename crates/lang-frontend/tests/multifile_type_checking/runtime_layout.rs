use super::*;

#[test]
fn runtime_field_layouts_are_owner_qualified_and_recursively_substituted() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package layout\n\
         class Wrapper<T>(val item: T)\n\
         class Holder<T>(val direct: T, val items: List<T>, val wrapped: Wrapper<T>)\n\
         class Grow<T>(val next: Grow<List<T>>)\n\
         value class Inline<T>(val item: T)",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package layout\n\
         fun consumeInt(input: Holder<Int>, nested: Wrapper<Int>): Unit {}\n\
         fun consumeString(input: Holder<String>, nested: Wrapper<String>): Unit {}\n\
         fun unrelated(input: Wrapper<Long>, item: Inline<Int>, grow: Grow<Int>): Unit {}\n\
         fun <U> generic(input: Holder<U>): Unit {}",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "layout/types.ko", types_source, &types),
        SourceUnitInput::new("root", "layout/uses.ko", uses_source, &uses),
    ];
    let reverse_inputs = [
        SourceUnitInput::new("root", "layout/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "layout/types.ko", types_source, &types),
    ];
    let (name_environment, type_environment) = standard_environments();

    let mut ordered_layouts = Vec::new();
    for inputs in [&forward_inputs[..], &reverse_inputs[..]] {
        let names = validated_names(&sources, inputs, &name_environment);
        let typed = check_compilation_unit_types(&sources, inputs, &names, &type_environment)
            .expect("runtime layout typing succeeds")
            .validate()
            .expect("runtime layout fixture has no diagnostics");
        let typed = typed.types();
        let holder = declaration(&names, "Holder");
        let wrapper = declaration(&names, "Wrapper");
        let grow = declaration(&names, "Grow");
        let inline = declaration(&names, "Inline");
        let int = typed
            .types()
            .builtin(BuiltinType::Int)
            .expect("Int is seeded");
        let string = typed
            .types()
            .builtin(BuiltinType::String)
            .expect("String is seeded");
        let holder_int = typed
            .types()
            .find(&UnitTypeKind::Nominal {
                declaration: holder,
                arguments: vec![int],
            })
            .expect("Holder<Int> is canonical");
        let holder_string = typed
            .types()
            .find(&UnitTypeKind::Nominal {
                declaration: holder,
                arguments: vec![string],
            })
            .expect("Holder<String> is canonical");

        for (owner, argument) in [(holder_int, int), (holder_string, string)] {
            let layout = typed
                .runtime_field_layout(owner)
                .expect("concrete Holder owner publishes one layout");
            assert_eq!(layout.owner_type(), owner);
            assert_eq!(layout.declaration(), holder);
            assert_eq!(layout.arguments(), [argument]);
            assert_eq!(layout.fields().len(), 3);
            let holder_signature = typed
                .signatures()
                .declaration(holder)
                .and_then(|signature| signature.nominal())
                .expect("Holder signature exists");
            for (actual, template) in layout.fields().iter().zip(holder_signature.fields()) {
                assert_eq!(actual.symbol(), template.symbol());
                assert_eq!(actual.template_type(), template.ty());
                assert_eq!(actual.span(), template.span());
            }
            assert_eq!(
                sources
                    .slice(layout.fields()[0].span())
                    .expect("direct field span"),
                "direct"
            );
            assert_eq!(
                sources
                    .slice(layout.fields()[1].span())
                    .expect("items field span"),
                "items"
            );
            assert_eq!(
                sources
                    .slice(layout.fields()[2].span())
                    .expect("wrapped field span"),
                "wrapped"
            );
            assert_eq!(layout.fields()[0].concrete_type(), argument);
            assert_eq!(
                typed.types().get(layout.fields()[1].concrete_type()),
                Some(&UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::List,
                    arguments: vec![argument],
                })
            );
            let wrapper_type = typed
                .types()
                .find(&UnitTypeKind::Nominal {
                    declaration: wrapper,
                    arguments: vec![argument],
                })
                .expect("nested Wrapper actual is canonicalized by layout materialization");
            assert_eq!(layout.fields()[2].concrete_type(), wrapper_type);
            assert_eq!(
                typed
                    .runtime_field_layout(wrapper_type)
                    .expect("nested concrete owner receives its own layout")
                    .fields()[0]
                    .concrete_type(),
                argument
            );
        }

        let holder_template = typed
            .signatures()
            .declaration(holder)
            .and_then(|signature| signature.nominal())
            .expect("Holder signature exists")
            .ty();
        let inline_int = typed
            .types()
            .find(&UnitTypeKind::Nominal {
                declaration: inline,
                arguments: vec![int],
            })
            .expect("Inline<Int> is canonical");
        assert!(typed.runtime_field_layout(holder_template).is_none());
        assert!(typed.runtime_field_layout(inline_int).is_none());
        let generic_holder = typed
            .signatures()
            .declaration(declaration(&names, "generic"))
            .and_then(|signature| signature.callable())
            .expect("generic callable signature exists")
            .parameters()[0]
            .ty();
        assert!(typed.runtime_field_layout(generic_holder).is_none());
        assert_eq!(
            typed
                .runtime_field_layouts()
                .iter()
                .filter(|layout| layout.declaration() == holder)
                .count(),
            2
        );

        let grow_int = typed
            .types()
            .find(&UnitTypeKind::Nominal {
                declaration: grow,
                arguments: vec![int],
            })
            .expect("Grow<Int> is canonical");
        let grow_layout = typed
            .runtime_field_layout(grow_int)
            .expect("the concrete recursive owner receives one bounded layout");
        let nested_grow = grow_layout.fields()[0].concrete_type();
        assert!(matches!(
            typed.types().get(nested_grow),
            Some(UnitTypeKind::Nominal { declaration, arguments })
                if *declaration == grow
                    && matches!(
                        arguments.as_slice(),
                        [argument]
                            if matches!(
                                typed.types().get(*argument),
                                Some(UnitTypeKind::Intrinsic {
                                    constructor: IntrinsicTypeConstructor::List,
                                    arguments,
                                }) if arguments == &[int]
                            )
                    )
        ));
        assert!(typed.runtime_field_layout(nested_grow).is_none());

        assert!(
            typed
                .runtime_field_layouts()
                .windows(2)
                .all(|pair| pair[0].owner_type().index() < pair[1].owner_type().index())
        );
        ordered_layouts.push(typed.runtime_field_layouts().to_vec());
    }
    assert_eq!(ordered_layouts[0], ordered_layouts[1]);
}

#[test]
fn runtime_field_layouts_are_atomic_on_typed_recovery() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-layout.ko",
        "package layout\n\
         class Holder<T>(val item: List<T>)\n\
         fun bad(input: Holder<Int>): String = 1",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "layout/invalid-layout.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("typed mismatch remains a recovery product");

    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084"]
    );
    assert!(typed.runtime_field_layouts().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn compilation_unit_class_with_deinit_records_signature_flag() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "deinit-class.ko",
        "class FileHandle(val fd: Int) {\n\
             deinit() {}\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "deinit-class.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let handle_nominal = typed
        .signatures()
        .declarations()
        .iter()
        .find_map(|d| d.nominal())
        .filter(|nominal| nominal.has_deinit());
    assert!(
        handle_nominal.is_some(),
        "FileHandle nominal should have has_deinit = true"
    );
}
