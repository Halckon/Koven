use super::*;

#[test]
fn member_receiver_is_checked_as_the_zeroth_call_operand() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Worker(var count: Int) {\n\
             fun read(): Int = count\n\
             inout fun bump(): Unit { count = count + 1 }\n\
             own fun finish(): Unit {}\n\
         }\n\
         fun use(own first: Worker, own second: Worker): Unit {\n\
             val a = first.read()\n\
             val b = first.bump()\n\
             val c = second.finish()\n\
             val d = Worker(0).read()\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    assert_eq!(ownership.call_receiver_contracts().len(), 4);
    assert_eq!(ownership.receiver_facts().len(), 4);

    let unit = source_unit(&names, source);
    let first = symbol_named(&ownership, &names, unit, "first");
    let second = symbol_named(&ownership, &names, unit, "second");
    for (text, expected_kind, expected_root) in [
        (
            "first.read()",
            UnitReceiverOwnershipKind::SharedLoan,
            Some(first),
        ),
        (
            "first.bump()",
            UnitReceiverOwnershipKind::ExclusiveLoan,
            Some(first),
        ),
        (
            "second.finish()",
            UnitReceiverOwnershipKind::Move,
            Some(second),
        ),
        (
            "Worker(0).read()",
            UnitReceiverOwnershipKind::SharedLoan,
            None,
        ),
    ] {
        let call = UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, text));
        let fact = ownership.receiver_fact(call).expect("receiver fact");
        assert_eq!(fact.kind(), expected_kind);
        assert!(fact.declaration_span().is_some());
        assert_eq!(sources.slice(fact.end_span()).expect("call span"), text);
        match (fact.source(), fact.target(), expected_root) {
            (
                UnitCallReceiverOrigin::Expression(origin),
                UnitReceiverOwnershipTarget::Place(place),
                Some(root),
            ) => {
                assert_eq!(origin.source_unit(), unit);
                assert_eq!(place.root(), root);
            }
            (
                UnitCallReceiverOrigin::Expression(origin),
                UnitReceiverOwnershipTarget::Temporary(temporary),
                None,
            ) => {
                assert_eq!(origin, *temporary);
            }
            actual => panic!("unexpected receiver fact: {actual:?}"),
        }
    }
    let temporary_call = UnitExpressionId::new(
        unit,
        expression_with_text(&sources, &parsed, "Worker(0).read()"),
    );
    let UnitReceiverOwnershipTarget::Temporary(temporary) = ownership
        .receiver_fact(temporary_call)
        .expect("temporary receiver fact")
        .target()
    else {
        panic!("Borrow temporary receiver must retain its expression identity");
    };
    assert!(ownership.drops().iter().any(|drop| {
        drop.point() == UnitDropPoint::CallReturn(temporary_call)
            && drop.target() == UnitDropTarget::Temporary(*temporary)
    }));
}

#[test]
fn stateless_object_borrow_receiver_has_no_runtime_drop_fact() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "object Registry { fun ping(): Int = 7 }\n\
         fun entry(): Int = Registry.ping()",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("stateless object ownership product");

    assert!(ownership.diagnostics().is_empty());
    let unit = source_unit(&names, source);
    let call = UnitExpressionId::new(
        unit,
        expression_with_text(&sources, &parsed, "Registry.ping()"),
    );
    let fact = ownership
        .receiver_fact(call)
        .expect("object Borrow receiver fact");
    assert_eq!(fact.kind(), UnitReceiverOwnershipKind::SharedLoan);
    assert!(matches!(
        fact.target(),
        UnitReceiverOwnershipTarget::Temporary(receiver) if *receiver == match fact.source() {
            UnitCallReceiverOrigin::Expression(receiver) => receiver,
            UnitCallReceiverOrigin::ImplicitThis(_) => panic!("object call uses an explicit receiver"),
        }
    ));
    assert!(
        ownership.drops().is_empty(),
        "a stateless object has no runtime owner to drop: {:?}",
        ownership.drops()
    );
}

#[test]
fn member_body_receiver_capability_controls_this_and_implicit_calls() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Worker(var count: Int) {\n\
             fun read(): Int = count\n\
             fun forwardRead(): Int = read()\n\
             inout fun bump(): Unit { count = count + 1 }\n\
             inout fun forwardBump(): Unit { val result = bump() }\n\
             own fun take(): Unit { val local = this }\n\
             own fun keep(): Unit {}\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    let unit = source_unit(&names, source);
    for (text, expected) in [
        ("read()", UnitReceiverOwnershipKind::SharedLoan),
        ("bump()", UnitReceiverOwnershipKind::ExclusiveLoan),
    ] {
        let call = UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, text));
        let fact = ownership
            .receiver_fact(call)
            .expect("implicit receiver fact");
        assert_eq!(fact.kind(), expected);
        assert!(matches!(
            (fact.source(), fact.target()),
            (
                UnitCallReceiverOrigin::ImplicitThis(source_owner),
                UnitReceiverOwnershipTarget::This(target_owner),
            ) if source_owner == *target_owner
        ));
    }
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), UnitDropTarget::This(_)))
            .count(),
        1,
        "only the unconsumed Value receiver is dropped as this",
    );
}

#[test]
fn implicit_value_receiver_remains_pending_until_call_commit() {
    for (exit, expected) in [("return", 1), ("error(\"abort\")", 0), ("true", 0)] {
        let mut sources = SourceMap::new();
        let text = format!(
            "class Host {{ own fun consume(text: String, own flag: Boolean): Unit {{}}\nown fun relay(own flag: Boolean): Unit {{ val done = consume(\"prefix\", if (flag) {{ {exit} }} else {{ true }}) }} }}"
        );
        let (source, parsed) = parsed(&mut sources, "main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &environment);
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                .unwrap()
                .validate()
                .unwrap();
        let call_text = format!("consume(\"prefix\", if (flag) {{ {exit} }} else {{ true }})");
        let call = UnitExpressionId::new(
            source_unit(&names, source),
            expression_with_text(&sources, &parsed, &call_text),
        );
        let UnitCallReceiverOrigin::ImplicitThis(relay) =
            owned.ownership().receiver_fact(call).unwrap().source()
        else {
            panic!("implicit receiver required")
        };
        let drops = owned
            .ownership()
            .drops()
            .iter()
            .filter(|fact| {
                fact.target() == UnitDropTarget::This(relay)
                    && matches!(fact.point(), UnitDropPoint::ControlTransfer(_))
            })
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), expected, "{exit}: {drops:?}");
        if exit == "return" {
            let ordered = owned
                .ownership()
                .drops()
                .iter()
                .filter(|fact| matches!(fact.point(), UnitDropPoint::ControlTransfer(_)))
                .map(|fact| fact.target())
                .collect::<Vec<_>>();
            let prefix = UnitExpressionId::new(
                source_unit(&names, source),
                expression_with_text(&sources, &parsed, "\"prefix\""),
            );
            assert_eq!(
                ordered,
                [
                    UnitDropTarget::Temporary(prefix),
                    UnitDropTarget::This(relay)
                ],
                "later argument owner must drop before the pending receiver"
            );
        }
    }
}

#[test]
fn static_self_value_receiver_publishes_conditional_drop_without_unconditional_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Finishable { own fun finish(): Int = 40 }\n\
         class Resource: Finishable {}\n\
         value class Counter(val item: Int): Finishable {}",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    assert!(
        !ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), UnitDropTarget::This(_))),
        "StaticSelf must not become an unconditional receiver drop",
    );
    let [fact] = ownership.conditional_receiver_drops() else {
        panic!(
            "one StaticSelf receiver-drop obligation expected: {:?}",
            ownership.conditional_receiver_drops()
        );
    };
    let UnitDropPoint::ControlTransfer(body) = fact.point() else {
        panic!("expression-body receiver must drop on its return edge");
    };
    assert_eq!(sources.slice(fact.value_origin()).unwrap(), "finish");
    let Some(UnitTypeKind::StaticSelf(interface)) = typed.types().types().get(fact.receiver_type())
    else {
        panic!("conditional receiver type must be StaticSelf");
    };
    let Some(UnitTypeKind::Nominal { declaration, .. }) = typed.types().types().get(*interface)
    else {
        panic!("StaticSelf must wrap the declaring interface type");
    };
    assert_eq!(fact.owner(), *declaration);
    assert_eq!(
        body,
        UnitExpressionId::new(
            source_unit(&names, source),
            expression_with_text(&sources, &parsed, "40"),
        ),
        "conditional drop point must name the exact default-body expression",
    );
}

#[test]
fn static_self_value_calls_publish_conditional_receiver_deliveries() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Parent { own fun inherited(): Int = 3 }\n\
         interface Chain: Parent {\n\
             fun read(): Int = 1\n\
             inout fun revise(): Unit {}\n\
             inout fun adjust(): Unit { val adjusted = revise() }\n\
             own fun endpoint(): Int = 2\n\
             own fun explicit(): Int {\n\
                 val observed = read()\n\
                 return this.endpoint()\n\
             }\n\
             own fun implicit(): Int = endpoint()\n\
             own fun inheritedExplicit(): Int = this.inherited()\n\
             own fun inheritedSuper(): Int = super<Parent>.inherited()\n\
         }\n\
         class Resource: Chain {}\n\
         value class Counter(val item: Int): Chain {}\n\
         fun exercise(): Unit {\n\
             val resource = Resource()\n\
             val moved = resource.explicit()\n\
             val counter = Counter(1)\n\
             val copied = counter.implicit()\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
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
    let unit = source_unit(&names, source);
    let expected_calls = [
        ("this.endpoint()", Some("this")),
        ("endpoint()", Some("endpoint")),
        ("this.inherited()", Some("this")),
        ("super<Parent>.inherited()", Some("super<Parent>.inherited")),
    ]
    .map(|(text, delivery)| {
        (
            UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, text)),
            delivery,
        )
    });
    assert_eq!(ownership.conditional_receiver_deliveries().len(), 4);
    for (call, delivery) in expected_calls {
        let fact = ownership
            .conditional_receiver_delivery(call)
            .expect("StaticSelf Value receiver delivery fact");
        let typed_call = typed
            .types()
            .calls()
            .iter()
            .find(|descriptor| descriptor.expression() == call)
            .expect("typed call descriptor");
        assert_eq!(fact.call(), call);
        assert_eq!(fact.target(), typed_call.target());
        assert!(matches!(
            typed.types().types().get(fact.receiver_type()),
            Some(UnitTypeKind::StaticSelf(_))
        ));
        assert!(ownership.conditional_receiver_drops().iter().any(|drop| {
            drop.owner() == fact.owner()
                && drop.receiver_type() == fact.receiver_type()
                && drop.value_origin() == fact.receiver_origin()
        }));
        assert!(ownership.receiver_fact(call).is_none());
        match fact.source() {
            UnitCallReceiverOrigin::Expression(receiver) => {
                assert_eq!(
                    sources.slice(fact.delivery_span()).unwrap(),
                    delivery.unwrap()
                );
                assert_eq!(receiver.source_unit(), unit);
            }
            UnitCallReceiverOrigin::ImplicitThis(owner) => {
                assert_eq!(owner, fact.owner());
                assert_eq!(
                    sources.slice(fact.delivery_span()).unwrap(),
                    delivery.unwrap()
                );
            }
        }
    }

    let read_call = UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, "read()"));
    assert_eq!(
        ownership
            .receiver_fact(read_call)
            .expect("Borrow receiver fact")
            .kind(),
        UnitReceiverOwnershipKind::SharedLoan,
    );
    let revise_call =
        UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, "revise()"));
    assert_eq!(
        ownership
            .receiver_fact(revise_call)
            .expect("Inout receiver fact")
            .kind(),
        UnitReceiverOwnershipKind::ExclusiveLoan,
    );
    assert!(
        ownership
            .conditional_receiver_delivery(revise_call)
            .is_none()
    );
    for (text, expected) in [
        ("resource.explicit()", UnitReceiverOwnershipKind::Move),
        ("counter.implicit()", UnitReceiverOwnershipKind::Copy),
    ] {
        let call = UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, text));
        assert_eq!(
            ownership
                .receiver_fact(call)
                .expect("concrete Value receiver fact")
                .kind(),
            expected,
        );
        assert!(ownership.conditional_receiver_delivery(call).is_none());
    }
}

#[test]
fn ownership_error_clears_conditional_receiver_delivery_facts_atomically() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Finishable {\n\
             own fun finish(): Unit {}\n\
             own fun forward(): Unit { val result = finish() }\n\
         }\n\
         class Resource {}\n\
         fun invalid(own resource: Resource): Unit {\n\
             val first = resource\n\
             val second = resource\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership recovery product");

    assert_eq!(diagnostic_codes(&ownership), ["L0131"]);
    assert!(ownership.conditional_receiver_deliveries().is_empty());
    assert!(ownership.conditional_receiver_drops().is_empty());
}

#[test]
fn conditional_receiver_drop_excludes_non_static_self_and_bodyless_receivers() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Actions {\n\
             fun inspect(): Unit {}\n\
             inout fun update(): Unit {}\n\
             own fun missing(): Unit\n\
             own fun finish(): Unit {}\n\
         }\n\
         class Resource { own fun consume(): Unit {} }\n\
         value class Counter(val item: Int) { own fun copy(): Unit {} }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    let [conditional] = ownership.conditional_receiver_drops() else {
        panic!("only the bodyful StaticSelf Value receiver is conditional");
    };
    assert_eq!(sources.slice(conditional.value_origin()).unwrap(), "finish");
    let unconditional_origins = ownership
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), UnitDropTarget::This(_)))
        .map(|fact| sources.slice(fact.value_origin()).expect("receiver origin"))
        .collect::<Vec<_>>();
    assert_eq!(unconditional_origins, ["consume"]);
}

#[test]
fn ownership_error_clears_conditional_receiver_drop_facts_atomically() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Finishable { own fun finish(): Unit {} }\n\
         class Resource {}\n\
         fun invalid(own resource: Resource): Unit {\n\
             val first = resource\n\
             val second = resource\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership recovery product");

    assert_eq!(diagnostic_codes(&ownership), ["L0131"]);
    assert!(ownership.conditional_receiver_drops().is_empty());
}

#[test]
fn receiver_loan_precedes_arguments_and_conflicts_with_overlapping_places() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Worker {\n\
             inout fun update(other: Worker): Unit {}\n\
             fun borrowThenTake(own other: Worker): Unit {}\n\
         }\n\
         fun first(own worker: Worker): Unit { val result = worker.update(worker) }\n\
         fun second(own worker: Worker): Unit { val result = worker.borrowThenTake(worker) }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert_eq!(diagnostic_codes(&ownership), ["L0135", "L0135"]);
    assert!(ownership.receiver_facts().is_empty());
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
}

#[test]
fn borrow_only_delegation_publishes_outer_and_field_ownership_plan() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Readable { fun read(): Int }\n\
         class Reader: Readable { override fun read(): Int = 1 }\n\
         class Host(val reader: Reader): Readable by reader",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    assert!(ownership.deferred().is_empty());
    assert_eq!(ownership.delegations().len(), 1);
    let plan = &ownership.delegations()[0];
    assert_eq!(plan.target().source_unit(), source_unit(&names, source));
    assert_eq!(plan.forwarders().len(), 1);
    assert_eq!(
        sources
            .slice(plan.delegation_span())
            .expect("delegation span"),
        "by reader",
    );
}

#[test]
fn member_body_rejects_receiver_capability_escalation_and_use_after_move() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Worker(var count: Int) {\n\
             inout fun bump(): Unit { count = count + 1 }\n\
             own fun consume(): Unit {}\n\
             fun badWrite(): Unit { count = 1 }\n\
             fun badInout(): Unit { val result = bump() }\n\
             fun badValue(): Unit { val result = consume() }\n\
             own fun badAfterMove(): Int {\n\
                 val moved = this\n\
                 return count\n\
             }\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert_eq!(
        diagnostic_codes(&ownership),
        ["L0134", "L0134", "L0133", "L0131"]
    );
    assert!(ownership.receiver_facts().is_empty());
}

#[test]
fn explicit_this_field_cannot_supply_inout_from_borrow_or_value_receiver() {
    for (mode, expected) in [
        ("borrow", vec!["L0134"]),
        ("own", vec!["L0134"]),
        ("inout", vec![]),
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "class Cell(var n: Int) {{ inout fun set(): Unit {{ n = 1 }} }}\nclass Holder(val cell: Cell) {{ {mode} fun test(): Unit {{ this.cell.set() }} }}"
        );
        let (source, parsed) = parsed(&mut sources, "main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");
        assert_eq!(diagnostic_codes(&ownership), expected, "{mode}");
    }
}
