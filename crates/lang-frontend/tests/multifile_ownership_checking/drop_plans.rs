use super::*;

#[test]
fn unit_asap_drop_facts_cover_return_temporary_replacement_and_control_edges() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         class Holder(var item: Resource) {\n\
         inout fun replace(own replacement: Resource): Unit {\n\
                 val result = (this.item = replacement)\n\
             }\n\
             inout fun stop(): Unit {\n\
                 val result = (item = error(\"stop\"))\n\
             }\n\
         }\n\
         class CopyHolder(var item: Int) {\n\
             inout fun replace(own replacement: Int): Unit {\n\
                 val result = (item = replacement)\n\
             }\n\
         }\n\
         fun create(): Resource\n\
         fun inspect(item: Resource): Unit {}\n\
         fun replaceOther(inout holder: Holder, own replacement: Resource): Unit {\n\
             val result = (holder.item = replacement)\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         import p.create\n\
         import p.inspect\n\
         fun drops(flag: Boolean, own unusedParameter: Resource, own branchOwner: Resource): Unit {\n\
             val unused = create()\n\
             val used = create()\n\
             val first = inspect(used)\n\
             var replaced = create()\n\
             { replaced = create() }\n\
             val temporary = inspect(create())\n\
             if (flag) {\n\
                 val branchRead = inspect(branchOwner)\n\
                 val early = create()\n\
                 if (flag) { return }\n\
                 val after = inspect(early)\n\
             } else {\n\
                 val branch = create()\n\
             }\n\
             while (flag) {\n\
                 val loopRead = inspect(replaced)\n\
                 break\n\
             }\n\
         }\n\
         fun returned(own result: Resource, own spare: Resource): Resource {\n\
             return result\n\
         }\n\
         fun captured(own item: Resource): Unit {\n\
             val closure: move () -> Unit = move { val read = inspect(item) }\n\
         }\n\
         fun stringDrops(own left: String, own right: String): Boolean {\n\
             val joined = left + \"!\"\n\
             return joined == right\n\
         }\n\
         fun elementDrop(own items: MutableList<Resource>, own replacement: Resource): Unit {\n\
             val result = (items[0] = replacement)\n\
         }\n\
         fun divergentAssignment(): Unit {\n\
             var target = create()\n\
             val result = (target = error(\"root-stop\"))\n\
             val unreachable = create()\n\
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

    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert!(ownership.deferred().is_empty());
    let provider_unit = source_unit(&names, provider_source);
    let consumer_unit = source_unit(&names, consumer_source);
    let field_assignments = [
        UnitExpressionId::new(
            provider_unit,
            expression_with_text(&sources, &provider, "this.item = replacement"),
        ),
        UnitExpressionId::new(
            provider_unit,
            expression_with_text(&sources, &provider, "holder.item = replacement"),
        ),
    ];
    let mut replaced_field = None;
    for field_assignment in field_assignments {
        let field_descriptor = typed
            .types()
            .assignment(field_assignment)
            .expect("MoveOnly field assignment descriptor");
        let projection = typed
            .types()
            .aggregate_projection(field_descriptor.target())
            .expect("MoveOnly field projection");
        if let Some(expected) = replaced_field {
            assert_eq!(projection.field(), expected, "same Holder field identity");
        } else {
            replaced_field = Some(projection.field());
        }
        let facts = ownership
            .drops()
            .iter()
            .filter(|fact| {
                matches!(
                    fact.point(),
                    UnitDropPoint::BeforeReplacement(expression)
                        if expression == field_assignment
                )
            })
            .collect::<Vec<_>>();
        assert!(matches!(
            facts.as_slice(),
            [fact]
                if matches!(
                    fact.target(),
                    UnitDropTarget::ReplacedField {
                        assignment,
                        field,
                    } if assignment == field_assignment && field == projection.field()
                ) && sources.slice(fact.value_origin()).unwrap().ends_with("item")
        ));
    }
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter(|fact| matches!(fact.point(), UnitDropPoint::BeforeReplacement(_)))
            .count(),
        2,
        "Copyable and non-fallthrough field assignments must not publish old-field drop facts"
    );
    let unreachable_drops = ownership
        .drops()
        .iter()
        .filter(|fact| {
            matches!(fact.target(), UnitDropTarget::Named(_))
                && sources.slice(fact.value_origin()).unwrap() == "unreachable"
        })
        .collect::<Vec<_>>();
    assert!(
        unreachable_drops.is_empty(),
        "an aborting replacement RHS must stop planning unreachable statements: {unreachable_drops:?}"
    );
    let target_drops = ownership
        .drops()
        .iter()
        .filter(|fact| {
            matches!(fact.target(), UnitDropTarget::Named(_))
                && sources.slice(fact.value_origin()).unwrap() == "target"
        })
        .collect::<Vec<_>>();
    assert_eq!(
        target_drops.len(),
        1,
        "ASAP may drop the unread target before abort, but must not add a second function-exit drop: {target_drops:?}"
    );
    let named_origins = ownership
        .drops()
        .iter()
        .filter_map(|fact| match fact.target() {
            UnitDropTarget::Named(_) => Some(sources.slice(fact.value_origin()).unwrap()),
            UnitDropTarget::This(_)
            | UnitDropTarget::Temporary(_)
            | UnitDropTarget::ReplacedElement(_)
            | UnitDropTarget::ReplacedField { .. }
            | UnitDropTarget::Captured { .. } => None,
        })
        .collect::<Vec<_>>();
    for expected in [
        "unusedParameter",
        "unused",
        "used",
        "replaced",
        "early",
        "branch",
        "spare",
        "closure",
    ] {
        assert!(
            named_origins.contains(&expected),
            "missing {expected}: {named_origins:?}"
        );
    }
    assert!(
        !named_origins.contains(&"result"),
        "returned owner must transfer instead of drop: {named_origins:?}"
    );
    for predicate in [
        ownership.drops().iter().any(|fact| {
            matches!(
                fact.point(),
                UnitDropPoint::FunctionEntry(item) if item.source_unit() == consumer_unit
            )
        }),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::AfterStatement(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::AfterBinaryOperands(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::CallReturn(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::ControlTransfer(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::BranchExit { .. })),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::LoopExit(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::AfterReplacement(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), UnitDropTarget::Temporary(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), UnitDropTarget::ReplacedElement(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), UnitDropTarget::Captured { .. })),
    ] {
        assert!(predicate, "{:?}", ownership.drops());
    }
    let validated = ownership
        .clone()
        .validate()
        .expect("complete ownership product validates");
    assert!(validated.ownership().is_compatible_with(&typed));

    let reversed_inputs = [inputs[1], inputs[0]];
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
    assert!(reversed.validate().is_ok());
}

#[test]
fn validated_unit_ownership_rejects_deferred_element_field_drop_plans() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         interface Finishable {\n\
             own fun finish(): Unit {}\n\
             own fun forward(): Unit { val result = finish() }\n\
         }\n\
         class Resource {}\n\
         class Holder(var payload: Resource)",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Holder\n\
         fun deferred(holders: List<Holder>): Unit {\n\
             val projected = holders[0].payload\n\
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
            .expect("recovery ownership product");

    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert_eq!(ownership.deferred().len(), 1);
    assert!(
        ownership.conditional_receiver_deliveries().is_empty(),
        "a deferred ownership boundary must not expose executable conditional deliveries"
    );
    assert_eq!(
        ownership.deferred()[0].reason(),
        OwnershipDeferredReason::IndexPlace
    );
    assert_eq!(
        sources
            .slice(
                consumer
                    .ast()
                    .expressions()
                    .get(ownership.deferred()[0].expression().expression())
                    .expect("deferred expression")
                    .span()
            )
            .expect("deferred source"),
        "holders[0].payload"
    );
    assert!(ownership.clone().validate().is_err());
}
