use super::*;

#[test]
fn when_entry_bodies_do_not_observe_later_condition_moves() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun take(own message: String): Boolean = true\n\
         fun read(message: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun use(own message: String, flag: Boolean): Unit {\n\
             when {\n\
                 flag -> { p.read(message) }\n\
                 p.take(message) -> {}\n\
                 else -> {}\n\
             }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    assert_eq!(ownership.loans().len(), 1);
    assert_eq!(ownership.value_deliveries().len(), 1);
    assert_eq!(
        ownership.value_deliveries()[0].kind(),
        UnitValueDeliveryKind::Move
    );
}

#[test]
fn move_only_control_results_transfer_branch_owners_and_drop_only_inputs() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun make(): String = \"made\"\n\
         fun inspect(message: String): Unit {}\n\
         fun wrap(message: String): String = \"wrapped\"",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun selected(flag: Boolean): String {\n\
             val leftOwner = \"left-owner\"\n\
             val rightOwner = \"right-owner\"\n\
             val result = if (flag) { leftOwner } else { rightOwner }\n\
             return result\n\
         }\n\
         fun nested(flag: Boolean): String = when (flag) {\n\
             true -> if (flag) { \"nested-a\" + p.make() } else { \"nested-b\" + \"nested-c\" }\n\
             false -> \"when-a\" + \"when-b\"\n\
         }\n\
         fun initialized(flag: Boolean): String {\n\
             val result = when (flag) {\n\
                 true -> \"init-a\" + \"init-b\"\n\
                 false -> \"init-c\" + \"init-d\"\n\
             }\n\
             return result\n\
         }\n\
         fun diverging(flag: Boolean): String {\n\
             val result = if (flag) { \"normal-a\" + \"normal-b\" } else { return \"early\" }\n\
             return result\n\
         }\n\
         fun aborting(flag: Boolean): String {\n\
             val result = if (flag) { \"survive-a\" + \"survive-b\" } else { error(\"stop\") }\n\
             return result\n\
         }\n\
         fun nestedAborting(flag: Boolean): String {\n\
             val result = if (flag) { \"nested-live-a\" + \"nested-live-b\" } else { p.wrap(error(\"nested-stop\")) }\n\
             return result\n\
         }\n\
         fun borrowed(flag: Boolean): Unit {\n\
             val borrowLeft = \"borrow-left\"\n\
             val borrowRight = \"borrow-right\"\n\
             val seen = p.inspect(if (flag) { borrowLeft } else { borrowRight })\n\
         }\n\
         fun discarded(flag: Boolean): Unit {\n\
             val discardLeft = \"discard-left\"\n\
             val discardRight = \"discard-right\"\n\
             if (flag) { discardLeft } else { discardRight }\n\
         }\n\
         fun entry(): Unit {\n\
             val action: move (borrow Boolean) -> String = move { flag ->\n\
                 when (flag) {\n\
                     true -> \"lambda-a\" + \"lambda-b\"\n\
                     false -> \"lambda-c\" + \"lambda-d\"\n\
                 }\n\
             }\n\
             val sibling: move () -> String = move { \"sibling-a\" + \"sibling-b\" }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed_inputs = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    let consumer_unit = source_unit(&names, consumer_source);
    let expression = |text| {
        UnitExpressionId::new(
            consumer_unit,
            expression_with_text(&sources, &consumer, text),
        )
    };
    let has_operand_drop = |text| {
        let expression = expression(text);
        ownership
            .drops()
            .iter()
            .any(|fact| fact.point() == UnitDropPoint::AfterBinaryOperands(expression))
    };
    for transferred in [
        "\"nested-a\" + p.make()",
        "\"nested-b\" + \"nested-c\"",
        "\"when-a\" + \"when-b\"",
        "\"init-a\" + \"init-b\"",
        "\"init-c\" + \"init-d\"",
        "\"normal-a\" + \"normal-b\"",
        "\"survive-a\" + \"survive-b\"",
        "\"nested-live-a\" + \"nested-live-b\"",
        "\"lambda-a\" + \"lambda-b\"",
        "\"lambda-c\" + \"lambda-d\"",
        "\"sibling-a\" + \"sibling-b\"",
    ] {
        assert!(
            has_operand_drop(transferred),
            "a consumed control tail drops only its owned composite inputs"
        );
        let result = expression(transferred);
        assert!(
            !ownership
                .drops()
                .iter()
                .any(|fact| fact.target() == UnitDropTarget::Temporary(result)),
            "the control tail result {transferred} transfers instead of being dropped: {:?}",
            ownership.drops()
        );
    }
    let nested = expression("\"nested-a\" + p.make()");
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter_map(|fact| {
                (fact.point() == UnitDropPoint::AfterBinaryOperands(nested))
                    .then_some(fact.target())
            })
            .collect::<Vec<_>>(),
        [
            UnitDropTarget::Temporary(expression("p.make()")),
            UnitDropTarget::Temporary(expression("\"nested-a\""))
        ],
        "control-tail composite operands drop in reverse evaluation order"
    );
    let early = expression("\"early\"");
    assert!(
        !ownership
            .drops()
            .iter()
            .any(|fact| fact.target() == UnitDropTarget::Temporary(early)),
        "an explicit return branch transfers its value before control cleanup"
    );
    let stop = expression("error(\"stop\")");
    assert!(
        !ownership.drops().iter().any(|fact| {
            matches!(
                fact.point(),
                UnitDropPoint::BranchExit {
                    control,
                    branch: 1
                } if control == expression("if (flag) { \"survive-a\" + \"survive-b\" } else { error(\"stop\") }")
            ) || fact.target() == UnitDropTarget::Temporary(stop)
        }),
        "a Nothing branch does not publish a normal branch exit or result owner"
    );
    let nested_stop = expression("p.wrap(error(\"nested-stop\"))");
    assert!(
        !ownership.drops().iter().any(|fact| {
            matches!(
                fact.point(),
                UnitDropPoint::BranchExit {
                    control,
                    branch: 1
                } if control == expression(
                    "if (flag) { \"nested-live-a\" + \"nested-live-b\" } else { p.wrap(error(\"nested-stop\")) }"
                )
            ) || fact.target() == UnitDropTarget::Temporary(nested_stop)
        }),
        "a nested Nothing argument prevents its enclosing call from publishing a branch exit"
    );
    let named_drops = |name| {
        ownership
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.target(), UnitDropTarget::Named(_))
                    && sources
                        .slice(fact.value_origin())
                        .is_ok_and(|origin| origin == name)
            })
            .map(|fact| fact.point())
            .collect::<Vec<_>>()
    };
    assert!(matches!(
        named_drops("leftOwner").as_slice(),
        [UnitDropPoint::BranchExit { branch: 1, .. }]
    ));
    assert!(matches!(
        named_drops("rightOwner").as_slice(),
        [UnitDropPoint::BranchExit { branch: 0, .. }]
    ));
    assert!(matches!(
        named_drops("borrowLeft").as_slice(),
        [UnitDropPoint::BranchExit { branch: 1, .. }]
    ));
    assert!(matches!(
        named_drops("borrowRight").as_slice(),
        [UnitDropPoint::BranchExit { branch: 0, .. }]
    ));
    assert!(matches!(
        named_drops("discardLeft").as_slice(),
        [UnitDropPoint::BranchExit { branch: 1, .. }]
    ));
    assert!(matches!(
        named_drops("discardRight").as_slice(),
        [UnitDropPoint::BranchExit { branch: 0, .. }]
    ));
    let borrowed_control = expression("if (flag) { borrowLeft } else { borrowRight }");
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter(|fact| {
                fact.target() == UnitDropTarget::Temporary(borrowed_control)
                    && matches!(fact.point(), UnitDropPoint::CallReturn(_))
            })
            .count(),
        1,
        "Borrow receives one merged control temporary owner and drops it after the call"
    );
    let discarded_control = expression("if (flag) { discardLeft } else { discardRight }");
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter(|fact| {
                fact.target() == UnitDropTarget::Temporary(discarded_control)
                    && fact.point() == UnitDropPoint::AfterExpression(discarded_control)
            })
            .count(),
        1,
        "a Read-context control owns and drops one merged result temporary"
    );
    ownership
        .clone()
        .validate()
        .expect("control result drop facts validate");
    let reversed_names = validated_names(&sources, &reversed_inputs, &name_environment);
    let reversed_typed = validated_types(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
    );
    let reversed = check_compilation_unit_ownership(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
        &reversed_typed,
    )
    .expect("reversed unit ownership product");
    assert_eq!(ownership.drops(), reversed.drops());
}

#[test]
fn consumed_control_result_updates_main_ownership_state() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun inspect(message: String): Unit {}\n\
         fun invalid(flag: Boolean): Unit {\n\
             val left = \"left\"\n\
             val right = \"right\"\n\
             val selected = if (flag) { left } else { right }\n\
             val reused = inspect(left)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "q/consumer.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert_eq!(diagnostic_codes(&ownership), ["L0131"]);
    assert!(
        ownership.drops().is_empty(),
        "diagnostics prevent publishing a contradictory drop plan"
    );
}
