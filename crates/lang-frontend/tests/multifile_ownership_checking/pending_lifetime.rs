use super::*;

#[path = "pending_lifetime/temporary_capture.rs"]
mod temporary_capture;

#[test]
fn unit_pending_borrow_owner_survives_branches_and_nested_calls() {
    for operand in [
        "if (flag) { 1 } else { 2 }",
        "when (flag) { true -> 1; false -> 2 }",
        "if (flag) { read(first) } else { read(first) }",
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "class Resource {{}}\nfun read(item: Resource): Int = 1\nfun take(item: Resource, count: Int): Int = count\nfun inspect(own first: Resource, flag: Boolean): Int = take(first, {operand})"
        );
        let (source, file) = parsed(&mut sources, "pending.ko", &text);
        let inputs = [SourceUnitInput::new("root", "pending.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");
        assert!(
            ownership.diagnostics().is_empty(),
            "{operand}: {:?}",
            ownership.diagnostics()
        );
        let unit = source_unit(&names, source);
        let first = symbol_named(&ownership, &names, unit, "first");
        let call = UnitExpressionId::new(
            unit,
            expression_with_text(&sources, &file, &format!("take(first, {operand})")),
        );
        let drops = ownership
            .drops()
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Named(first))
            .map(|fact| fact.point())
            .collect::<Vec<_>>();
        assert_eq!(
            drops,
            [UnitDropPoint::CallReturn(call)],
            "the first argument remains borrowed until the outer call returns: {operand}"
        );
        ownership.validate().expect("valid ownership");
    }
}

#[test]
fn unit_pending_borrow_in_aborting_interpolation_has_no_normal_drop() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "pending-abort.ko",
        r#"
fun take(item: Rc<Int>, second: Int): Int = second
fun inspect(own first: Rc<Int>) {
    "${take(first, error("stop"))}"
}
"#,
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "pending-abort.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("ownership product terminates");
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    let first = symbol_named(&ownership, &names, source_unit(&names, source), "first");
    assert!(
        ownership
            .drops()
            .iter()
            .all(|fact| fact.target() != UnitDropTarget::Named(first)),
        "abort does not unwind the pending argument owner"
    );
    ownership.validate().expect("valid ownership");
}

#[test]
fn conditional_pending_receiver_drop_keeps_its_order_among_argument_and_local_owners() {
    for receiver in ["consume", "this.consume"] {
        let mut sources = SourceMap::new();
        let text = format!(
            "interface Relay {{ own fun consume(text: String, own flag: Boolean): Unit {{}}\nown fun relay(own flag: Boolean): Unit {{ val older = \"older\"\nval done = {receiver}(\"prefix\", if (flag) {{ val newer = \"newer\"\nif (flag) {{ return }} else {{ newer == \"newer\" }} }} else {{ true }})\nval used = println(older) }} }}"
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
        let point = UnitDropPoint::ControlTransfer(UnitExpressionId::new(
            source_unit(&names, source),
            expression_with_text(&sources, &parsed, "return"),
        ));
        let fact = owned
            .ownership()
            .conditional_receiver_drops()
            .iter()
            .find(|fact| fact.point() == point)
            .unwrap();
        let origins = owned
            .ownership()
            .drops()
            .iter()
            .filter(|fact| fact.point() == point)
            .map(|fact| sources.slice(fact.value_origin()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(origins, ["newer", "\"prefix\"", "older"]);
        assert_eq!(
            fact.preceding_drops(),
            2,
            "new local and later operand precede pending receiver; older local follows it"
        );
    }
}

#[test]
fn function_value_callee_stays_owned_until_call_or_argument_exit() {
    for prefix in ["move ", ""] {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(
            &mut sources,
            "main.ko",
            &format!(
                "fun entry(own flag: Boolean): Unit {{ val captured = \"captured\"\nval action: {prefix}(borrow String, own Boolean) -> Unit = {prefix}{{ text, accepted -> println(captured) }}\nval done = action(\"prefix\", if (flag) {{ return }} else {{ true }}) }}"
            ),
        );
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &environment);
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                .unwrap()
                .validate()
                .unwrap();
        let ownership = owned.ownership();
        let unit = source_unit(&names, source);
        let action = names.names().source_units()[unit.index()]
            .resolution()
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "action")
            .unwrap()
            .id();
        let drops = ownership
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.target(), UnitDropTarget::Named(symbol)
        if symbol.source_unit() == unit && symbol.symbol() == action)
            })
            .map(|fact| fact.point())
            .collect::<Vec<_>>();
        assert_eq!(
            drops.len(),
            2,
            "callee must be cleaned on both return and successful call"
        );
        assert!(
            drops.contains(&UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                unit,
                expression_with_text(&sources, &parsed, "return")
            )))
        );
        assert!(
            drops.contains(&UnitDropPoint::CallReturn(UnitExpressionId::new(
                unit,
                expression_with_text(
                    &sources,
                    &parsed,
                    "action(\"prefix\", if (flag) { return } else { true })"
                )
            )))
        );
        if prefix.is_empty() {
            let captured = names.names().source_units()[unit.index()]
                .resolution()
                .symbols()
                .iter()
                .find(|symbol| symbol.name() == "captured")
                .unwrap()
                .id();
            let points = ownership
                .drops()
                .iter()
                .filter(|fact| {
                    matches!(fact.target(), UnitDropTarget::Named(symbol)
            if symbol.source_unit() == unit && symbol.symbol() == captured)
                })
                .map(|fact| fact.point())
                .collect::<Vec<_>>();
            assert_eq!(points.len(), 2);
            assert!(
                points.iter().all(|point| matches!(
                    point,
                    UnitDropPoint::CallReturn(_) | UnitDropPoint::ControlTransfer(_)
                )),
                "shared source must survive every argument branch: {points:?}"
            );
        }
    }
}

#[test]
fn nested_argument_loan_ends_at_its_own_call_before_outer_inout() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "main.ko",
        "class Resource {}\n\
         fun read(item: Resource): Int = 1\n\
         fun accept(own count: Int, inout item: Resource): Unit {}\n\
         fun valid(own input: Resource): Unit {\n\
             var local = input\n\
             val result = accept(read(local), &local)\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership");
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert!(ownership.deferred().is_empty());
    let loans = ownership.loans();
    assert_eq!(loans.len(), 2);
    assert_eq!(loans[0].kind(), LoanKind::Shared);
    assert_eq!(loans[1].kind(), LoanKind::Exclusive);
    assert_ne!(loans[0].call(), loans[1].call());
    assert_eq!(sources.slice(loans[0].end_span()).unwrap(), "read(local)");
    assert_eq!(
        sources.slice(loans[1].end_span()).unwrap(),
        "accept(read(local), &local)"
    );
}
