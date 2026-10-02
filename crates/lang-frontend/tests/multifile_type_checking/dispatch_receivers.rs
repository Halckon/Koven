use super::*;

#[test]
fn concrete_override_publishes_static_abstract_requirement_dispatch() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         class Child: Readable {\n\
             override fun read(): Int = 7\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unit type checking succeeds internally");

    assert!(typed.diagnostics().is_empty());
    let readable = typed
        .signatures()
        .declaration(declaration(&names, "Readable"))
        .and_then(|signature| signature.nominal())
        .expect("Readable signature");
    let child = typed
        .signatures()
        .declaration(declaration(&names, "Child"))
        .and_then(|signature| signature.nominal())
        .expect("Child signature");
    let requirement = readable
        .members()
        .iter()
        .find(|member| member.name() == "read")
        .expect("abstract requirement")
        .target();
    let implementation = child
        .members()
        .iter()
        .find(|member| member.name() == "read")
        .expect("concrete override")
        .target();

    assert_eq!(child.static_dispatch_overrides().len(), 1);
    assert_eq!(
        child.static_dispatch_overrides()[0].requirement(),
        requirement
    );
    assert_eq!(
        child.static_dispatch_overrides()[0].implementation(),
        implementation
    );
    assert_eq!(
        child.static_dispatch_overrides()[0].implementation_owner(),
        child.ty()
    );
    assert!(typed.clone().validate().is_ok());
}

#[test]
fn inherited_replacements_publish_ancestor_requirement_dispatch() {
    for (replacement, expected_owner) in [
        ("fun read(): Int = 7", "Derived"),
        ("fun read(): Int", "Child"),
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "package p\n\
             interface Base {{\n\
                 fun read(): Int\n\
                 fun throughRequirement(): Int = this.read()\n\
             }}\n\
             interface Derived: Base {{ {replacement} }}\n\
             class Child: Derived {{\n\
                 {}\n\
             }}",
            if expected_owner == "Child" {
                "override fun read(): Int = 7"
            } else {
                ""
            }
        );
        let (source, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("unit type checking succeeds internally");

        assert!(typed.diagnostics().is_empty(), "{replacement}");
        let base = typed
            .signatures()
            .declaration(declaration(&names, "Base"))
            .and_then(|signature| signature.nominal())
            .expect("Base signature");
        let expected = typed
            .signatures()
            .declaration(declaration(&names, expected_owner))
            .and_then(|signature| signature.nominal())
            .expect("effective implementation owner");
        let derived = typed
            .signatures()
            .declaration(declaration(&names, "Derived"))
            .and_then(|signature| signature.nominal())
            .expect("Derived signature");
        let child = typed
            .signatures()
            .declaration(declaration(&names, "Child"))
            .and_then(|signature| signature.nominal())
            .expect("Child signature");
        let mut requirements = vec![(
            base.members()
                .iter()
                .find(|member| member.name() == "read")
                .expect("ancestor abstract requirement")
                .target(),
            base.ty(),
        )];
        if expected_owner == "Child" {
            requirements.push((
                derived
                    .members()
                    .iter()
                    .find(|member| member.name() == "read")
                    .expect("intermediate abstract requirement")
                    .target(),
                derived.ty(),
            ));
        }
        let implementation = expected
            .members()
            .iter()
            .find(|member| member.name() == "read")
            .expect("effective implementation")
            .target();

        assert_eq!(child.static_dispatch_overrides().len(), requirements.len());
        for (requirement, requirement_owner) in requirements {
            let dispatch = child
                .static_dispatch_overrides()
                .iter()
                .find(|dispatch| {
                    dispatch.requirement() == requirement
                        && dispatch.implementation() == implementation
                })
                .expect("ancestor requirement dispatch");
            assert_eq!(dispatch.requirement_owner(), requirement_owner);
            assert_eq!(dispatch.implementation_owner(), expected.ty());
        }
        assert!(typed.clone().validate().is_ok());
    }
}

#[test]
fn unique_unrelated_default_publishes_abstract_requirement_dispatch() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Required {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Provided { fun read(): Int = 7 }\n\
         class Child: Required, Provided {}",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unit type checking succeeds internally");

    assert!(typed.diagnostics().is_empty());
    let required = typed
        .signatures()
        .declaration(declaration(&names, "Required"))
        .and_then(|signature| signature.nominal())
        .expect("Required signature");
    let provided = typed
        .signatures()
        .declaration(declaration(&names, "Provided"))
        .and_then(|signature| signature.nominal())
        .expect("Provided signature");
    let child = typed
        .signatures()
        .declaration(declaration(&names, "Child"))
        .and_then(|signature| signature.nominal())
        .expect("Child signature");
    let requirement = required.members()[0].target();
    let implementation = provided.members()[0].target();
    let dispatch = child
        .static_dispatch_overrides()
        .first()
        .expect("unique inherited default dispatch");

    assert_eq!(child.static_dispatch_overrides().len(), 1);
    assert_eq!(dispatch.requirement(), requirement);
    assert_eq!(dispatch.requirement_owner(), required.ty());
    assert_eq!(dispatch.implementation(), implementation);
    assert_eq!(dispatch.implementation_owner(), provided.ty());
    assert!(typed.clone().validate().is_ok());
}

#[test]
fn interface_replacement_checks_every_same_shape_contract() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Matching { fun read(): Int }\n\
         interface Mismatched { fun read(): Long }\n\
         interface Combined: Matching, Mismatched { fun read(): Int }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("replacement diagnostics remain recoverable");

    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0099"]
    );
}

#[test]
fn incompatible_unique_default_does_not_satisfy_abstract_requirement() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Required { fun read(): Int }\n\
         interface Incompatible { fun read(): Long = 1L }\n\
         class Child: Required, Incompatible {}",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("missing implementation diagnostics remain recoverable");

    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0101"]
    );
    let child = typed
        .signatures()
        .declaration(declaration(&names, "Child"))
        .and_then(|signature| signature.nominal())
        .expect("Child signature");
    assert!(child.static_dispatch_overrides().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn instance_receiver_contracts_and_call_origins_are_source_qualified() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "receivers.ko",
        "package p\n\
         class Holder<T>(val item: T) {\n\
             fun echo(input: T): T = input\n\
             borrow fun same(input: T): T = input\n\
             inout fun replace(own input: T): T = input\n\
             fun nested(input: T): T = echo(input)\n\
             fun current(): T = item\n\
         }\n\
         interface Root<T> { fun identity(input: T): T = input }\n\
         interface Parent<T>: Root<T>\n\
         class Child<T>: Parent<T> {\n\
             fun fromDefault(input: T): T = super<Parent<T>>.identity(input)\n\
         }\n\
         object Registry { fun ping(): Int = 1 }\n\
         fun use(holder: Holder<Int>): Int = holder.echo(1)\n\
         fun objectUse(): Int = Registry.ping()",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/receivers.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("receiver contracts type check");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let holder = typed
        .signatures()
        .declaration(declaration(&names, "Holder"))
        .and_then(|signature| signature.nominal())
        .expect("Holder nominal signature");
    let receiver_modes = holder
        .members()
        .iter()
        .map(|member| {
            (
                member.name(),
                member.receiver().expect("instance receiver").mode(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        receiver_modes,
        [
            ("echo", ParameterMode::Borrow),
            ("same", ParameterMode::Borrow),
            ("replace", ParameterMode::Inout),
            ("nested", ParameterMode::Borrow),
            ("current", ParameterMode::Borrow),
        ]
    );

    let unit = source_unit(&names, source);
    let explicit = expression_with_text(&sources, &file, "holder.echo(1)");
    let implicit = expression_with_text(&sources, &file, "echo(input)");
    let default_call = expression_with_text(&sources, &file, "super<Parent<T>>.identity(input)");
    let object_call = expression_with_text(&sources, &file, "Registry.ping()");
    let explicit_receiver = typed
        .call(UnitExpressionId::new(unit, explicit))
        .and_then(|call| call.receiver())
        .expect("explicit receiver fact");
    let implicit_receiver = typed
        .call(UnitExpressionId::new(unit, implicit))
        .and_then(|call| call.receiver())
        .expect("implicit receiver fact");
    assert!(matches!(
        explicit_receiver.origin(),
        UnitCallReceiverOrigin::Expression(_)
    ));
    assert_eq!(explicit_receiver.mode(), ParameterMode::Borrow);
    assert_eq!(explicit_receiver.category(), ExpressionCategory::Place);
    assert!(matches!(
        implicit_receiver.origin(),
        UnitCallReceiverOrigin::ImplicitThis(owner) if owner == declaration(&names, "Holder")
    ));
    let default_receiver = typed
        .call(UnitExpressionId::new(unit, default_call))
        .and_then(|call| call.receiver())
        .expect("super default receiver fact");
    assert!(matches!(
        default_receiver.origin(),
        UnitCallReceiverOrigin::ImplicitThis(owner) if owner == declaration(&names, "Child")
    ));
    assert!(
        typed
            .call(UnitExpressionId::new(unit, object_call))
            .and_then(|call| call.receiver())
            .is_some()
    );
    let bare_field = expression_with_text(&sources, &file, "item");
    assert!(matches!(
        typed
            .aggregate_projection(UnitExpressionId::new(unit, bare_field))
            .expect("bare field projection")
            .receiver(),
        UnitAggregateProjectionReceiver::This(owner) if owner == declaration(&names, "Holder")
    ));
}

#[test]
fn unit_super_uses_the_selected_receiver_mode_and_exact_interface_instance() {
    let mut sources = SourceMap::new();
    let (valid_source, valid) = parsed(
        &mut sources,
        "valid-super.ko",
        "package valid\n\
         interface Mixed {\n\
             inout fun choose(input: Int): Int = 1\n\
             fun choose(input: String): Int = 2\n\
         }\n\
         class Good: Mixed { fun run(): Int = super<Mixed>.choose(\"ok\") }",
    );
    let valid_inputs = [SourceUnitInput::new(
        "root",
        "valid/valid-super.ko",
        valid_source,
        &valid,
    )];
    let (name_environment, type_environment) = standard_environments();
    let valid_names = validated_names(&sources, &valid_inputs, &name_environment);
    let valid_typed =
        check_compilation_unit_types(&sources, &valid_inputs, &valid_names, &type_environment)
            .expect("valid super overload remains recoverable");
    assert!(
        valid_typed.diagnostics().is_empty(),
        "{:?}",
        valid_typed.diagnostics()
    );

    let mut sources = SourceMap::new();
    let (mode_source, mode) = parsed(
        &mut sources,
        "invalid-mode-super.ko",
        "package mode\n\
         interface Mixed {\n\
             inout fun choose(input: Int): Int = 1\n\
             fun choose(input: String): Int = 2\n\
         }\n\
         class Bad: Mixed { fun run(): Int = super<Mixed>.choose(1) }",
    );
    let mode_inputs = [SourceUnitInput::new(
        "root",
        "mode/invalid-mode-super.ko",
        mode_source,
        &mode,
    )];
    let mode_names = validated_names(&sources, &mode_inputs, &name_environment);
    let mode_typed =
        check_compilation_unit_types(&sources, &mode_inputs, &mode_names, &type_environment)
            .expect("invalid selected super receiver remains recoverable");
    assert_eq!(
        mode_typed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0100"]
    );

    let mut sources = SourceMap::new();
    let (invalid_source, invalid) = parsed(
        &mut sources,
        "invalid-super.ko",
        "package invalid\n\
         interface Generic<T> { fun echo(input: T): T = input }\n\
         class Bad: Generic<Int> {\n\
             fun run(): String = super<Generic<String>>.echo(\"wrong\")\n\
         }",
    );
    let invalid_inputs = [SourceUnitInput::new(
        "root",
        "invalid/invalid-super.ko",
        invalid_source,
        &invalid,
    )];
    let invalid_names = validated_names(&sources, &invalid_inputs, &name_environment);
    let invalid_typed =
        check_compilation_unit_types(&sources, &invalid_inputs, &invalid_names, &type_environment)
            .expect("invalid generic super qualifier remains recoverable");
    assert_eq!(
        invalid_typed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0099"]
    );
}

#[test]
fn object_rejects_non_borrow_receiver_at_the_marker() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "object-receiver.ko",
        "package p\nobject Registry { inout fun reset(): Unit {} }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/object-receiver.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("object receiver diagnostic remains recoverable");
    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0099"]
    );
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("object receiver primary"),
        "inout"
    );
}
