use super::*;

#[test]
fn unit_intrinsic_box_and_rc_publish_complete_value_construction_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         value class Point(val x: Int)\n\
         class Resource {}",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun values(): Unit {\n\
             val inferredBox = Box(Point(1))\n\
             val explicitBox = Box<Point>(Point(2))\n\
             val inferredRc = Rc(Resource())\n\
             val explicitRc = Rc<Resource>(Resource())\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("intrinsic constructions type check internally");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed intrinsic constructions type check internally");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.constructions(), reverse.constructions());
    let intrinsic = typed
        .constructions()
        .iter()
        .filter(|descriptor| {
            matches!(
                descriptor.target(),
                UnitConstructionTarget::IntrinsicBox | UnitConstructionTarget::IntrinsicRc
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(intrinsic.len(), 4);
    assert_eq!(
        intrinsic
            .iter()
            .map(|descriptor| descriptor.target())
            .collect::<Vec<_>>(),
        [
            UnitConstructionTarget::IntrinsicBox,
            UnitConstructionTarget::IntrinsicBox,
            UnitConstructionTarget::IntrinsicRc,
            UnitConstructionTarget::IntrinsicRc,
        ]
    );
    for descriptor in intrinsic {
        assert_eq!(descriptor.instance().type_arguments().len(), 1);
        assert_eq!(descriptor.arguments().len(), 1);
        let argument = &descriptor.arguments()[0];
        assert_eq!(argument.parameter_symbol(), None);
        assert_eq!(argument.mode(), ParameterMode::Value);
        assert_eq!(argument.parameter_index(), 0);
        assert_eq!(argument.evaluation_index(), 0);
        assert_eq!(argument.category(), ExpressionCategory::Temporary);
        assert_eq!(
            argument.parameter_type(),
            descriptor.instance().type_arguments()[0]
        );
        assert_eq!(
            argument.parameter_name(),
            if descriptor.target() == UnitConstructionTarget::IntrinsicBox {
                "element"
            } else {
                "value"
            }
        );
        let constructor = if descriptor.target() == UnitConstructionTarget::IntrinsicBox {
            IntrinsicTypeConstructor::Box
        } else {
            IntrinsicTypeConstructor::Rc
        };
        assert_eq!(
            typed.types().get(descriptor.result_type()),
            Some(&UnitTypeKind::Intrinsic {
                constructor,
                arguments: descriptor.instance().type_arguments().to_vec(),
            })
        );
        assert_eq!(
            typed.expression_type(descriptor.expression()),
            Some(descriptor.result_type())
        );
    }
}

#[test]
fn invalid_unit_intrinsic_constructions_reuse_diagnostics_without_partial_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\nvalue class Point(val x: Int)",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun invalidBox(): Unit { Box(1) }\n\
         fun boxTypeArity(): Unit { Box<Int, Long>(1) }\n\
         fun invalidRc(): Unit { Rc<Nothing>(error(\"stop\")) }\n\
         fun rcTypeArity(): Unit { Rc<Int, Long>(1) }\n\
         fun missingOperand(): Unit { Rc() }\n\
         fun wrongMode(): Unit { Box(&1) }\n\
         fun wrongOperand(): Unit { Box<Point>(true) }\n\
         fun wrongResult(): String = Rc(1)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid intrinsic constructions stay recoverable");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed invalid intrinsic constructions stay recoverable");

    assert_eq!(typed.body_diagnostics(), reverse.body_diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0117", "L0091", "L0125", "L0091", "L0121", "L0122", "L0084", "L0084"
        ]
    );
    assert!(typed.constructions().is_empty());
    assert!(reverse.constructions().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn source_box_and_rc_names_never_gain_unit_intrinsic_identity() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         class Box(val item: Int)\n\
         class Rc(val item: Int) { fun share(): Rc = this }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun values(): Unit {\n\
             val boxed = Box(1)\n\
             val counted = Rc(2)\n\
             val retained = counted.share()\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("source names retain nominal construction identity");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(
        typed
            .constructions()
            .iter()
            .map(|descriptor| descriptor.target())
            .collect::<Vec<_>>(),
        [
            UnitConstructionTarget::Nominal(declaration(&names, "Box")),
            UnitConstructionTarget::Nominal(declaration(&names, "Rc")),
        ]
    );
    assert!(typed.constructions().iter().all(|descriptor| {
        descriptor.arguments()[0]
            .parameter_symbol()
            .is_some_and(|symbol| symbol.source_unit() == source_unit(&names, models_source))
    }));
    assert!(typed.rc_operations().is_empty());
    assert_eq!(
        typed
            .calls()
            .iter()
            .filter(|call| matches!(call.target(), UnitCallTarget::Symbol(_)))
            .count(),
        1
    );
}

#[test]
fn deferred_expected_types_do_not_reject_complete_unit_intrinsic_constructions() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         value class Point(val x: Int)\n\
         fun values(): Unit {\n\
             val boxed: Opaque = Box(Point(1))\n\
             val counted: Opaque = Rc(2)\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (mut name_environment, type_environment) = standard_environments();
    name_environment
        .declare_type("Opaque")
        .expect("fresh unbound external type");
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("deferred expected type remains recoverable");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(
        typed
            .constructions()
            .iter()
            .filter(|descriptor| matches!(
                descriptor.target(),
                UnitConstructionTarget::IntrinsicBox | UnitConstructionTarget::IntrinsicRc
            ))
            .count(),
        2
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn intrinsic_construction_overload_lambda_trials_commit_only_the_unique_candidate() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         value class Point(val x: Int)\n\
         fun choose(action: () -> Box<Point>): Int = 1\n\
         fun choose(action: () -> Rc<Point>): Long = 1L\n\
         fun selected(): Int = choose({ Box(Point(1)) })",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("intrinsic construction trial is isolated");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.constructions().len(), 2);
    assert!(matches!(
        typed.constructions()[0].target(),
        UnitConstructionTarget::Nominal(_)
    ));
    assert_eq!(
        typed.constructions()[1].target(),
        UnitConstructionTarget::IntrinsicBox
    );
    assert!(
        typed
            .constructions()
            .iter()
            .all(|descriptor| { descriptor.target() != UnitConstructionTarget::IntrinsicRc })
    );
}

#[test]
fn intrinsic_rc_members_publish_unit_operations_and_trial_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         class Resource\n\
         fun inspect(resource: Resource): Unit {}\n\
         fun choose(callback: (Int) -> Int): Int = 1\n\
         fun choose(callback: (String) -> String): String = \"text\"",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun read(shared: Rc<Int>): Int = shared.value\n\
         fun retain(shared: Rc<Int>): Rc<Int> = shared.share()\n\
         fun borrowPayload(shared: Rc<Resource>): Unit = inspect(shared.value)\n\
         fun selected(shared: Rc<Int>): Int = choose({ ignored -> shared.value })",
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
            .expect("intrinsic Rc member operations are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed intrinsic Rc member operations are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.rc_operations(), reverse.rc_operations());
    assert_eq!(forward.rc_operations().len(), 4);
    assert_eq!(
        forward
            .rc_operations()
            .iter()
            .filter(|operation| operation.kind() == RcOperationKind::Value)
            .count(),
        3
    );
    assert_eq!(
        forward
            .rc_operations()
            .iter()
            .filter(|operation| operation.kind() == RcOperationKind::Share)
            .count(),
        1
    );
    for operation in forward.rc_operations() {
        assert_eq!(
            forward.expression_category(operation.expression()),
            Some(match operation.kind() {
                RcOperationKind::Share => ExpressionCategory::Temporary,
                RcOperationKind::Value => ExpressionCategory::Place,
            })
        );
        assert_eq!(
            operation.expression().source_unit(),
            operation.receiver().source_unit()
        );
    }
    assert!(forward.validate().is_ok());
}

#[test]
fn invalid_intrinsic_rc_share_calls_publish_no_partial_operation() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-rc.ko",
        "fun badType(shared: Rc<Int>): Rc<Int> = shared.share<String>()\n\
         fun badArgument(shared: Rc<Int>): Rc<Int> = shared.share(1)",
    );
    let inputs = [SourceUnitInput::new("root", "invalid-rc.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid Rc calls stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0091", "L0121"]
    );
    assert!(typed.rc_operations().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn invalid_intrinsic_rc_share_traverses_nested_container_operands() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "nested-rc.ko",
        "fun bad(shared: Rc<Int>): Rc<Int> = shared.share(arrayOf<Int>(1))",
    );
    let inputs = [SourceUnitInput::new("root", "nested-rc.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid Rc call still checks the now-supported nested container");
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0121"]
    );
    assert_eq!(typed.container_constructions().len(), 1);
    assert!(typed.rc_operations().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn safe_and_nullable_intrinsic_rc_members_remain_deferred_without_rc_operations() {
    for (name, text, expression_text) in [
        (
            "safe-value.ko",
            "fun bad(shared: Rc<Int>): Int = shared?.value",
            "shared?.value",
        ),
        (
            "nullable-value.ko",
            "fun bad(shared: Rc<Int>?): Int = shared?.value",
            "shared?.value",
        ),
        (
            "safe-share.ko",
            "fun bad(shared: Rc<Int>): Rc<Int> = shared?.share()",
            "shared?.share()",
        ),
        (
            "nullable-share.ko",
            "fun bad(shared: Rc<Int>?): Rc<Int>? = shared?.share()",
            "shared?.share()",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, name, text);
        let inputs = [SourceUnitInput::new("root", name, source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("safe/nullable Rc members preserve the existing deferred boundary");
        let expression = expression_with_text(&sources, &file, expression_text);
        assert!(matches!(
            typed
                .expression_type(UnitExpressionId::new(
                    source_unit(&names, source),
                    expression
                ))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Deferred(DeferredReason::MemberAccess))
        ));
        assert!(typed.rc_operations().is_empty());
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(typed.validate().is_ok());
    }
}

#[test]
fn poisoned_intrinsic_rc_payloads_defer_without_operation_facts() {
    for (name, text, expression_text) in [
        (
            "poisoned-value.ko",
            "fun bad(shared: Rc<List<Opaque>>): List<Opaque> = shared.value",
            "shared.value",
        ),
        (
            "poisoned-share.ko",
            "fun bad(shared: Rc<List<Opaque>>): Rc<List<Opaque>> = shared.share()",
            "shared.share()",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, name, text);
        let inputs = [SourceUnitInput::new("root", name, source, &file)];
        let (mut name_environment, type_environment) = standard_environments();
        name_environment
            .declare_type("Opaque")
            .expect("external type name is unique");
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("poisoned Rc payload preserves a deferred recovery boundary");
        let expression = expression_with_text(&sources, &file, expression_text);
        assert!(matches!(
            typed
                .expression_type(UnitExpressionId::new(
                    source_unit(&names, source),
                    expression
                ))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Deferred(DeferredReason::MemberAccess))
        ));
        assert!(typed.rc_operations().is_empty());
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(typed.validate().is_ok());
    }
}
