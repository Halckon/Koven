use super::*;

/// local 的身份来自名称解析；ownership bindings 只包含参数，不能用于查 local。
fn referenced_local_symbol(
    names: &ValidatedCompilationUnitNames,
    unit: SourceUnitId,
    name: &str,
) -> UnitSymbolId {
    let local = names.names().source_units()[unit.index()]
        .resolution()
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .expect("local symbol exists")
        .id();
    names
        .names()
        .references()
        .iter()
        .find_map(|reference| match reference.target() {
            lang_frontend::name_resolution::UnitReferenceTarget::Symbol(symbol)
                if symbol.source_unit() == unit && symbol.symbol() == local =>
            {
                Some(*symbol)
            }
            _ => None,
        })
        .expect("source-qualified local reference exists")
}

#[test]
fn container_size_preserves_local_value_and_borrow_receivers_without_escaping_loans() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "size-ownership.ko",
        "fun take(own value: List<Int>): Unit {}\n\
         fun sizes(own value: List<Int>, borrowed: Array<Int>, mutable: MutableList<Int>): Unit {\n\
             val local = listOf(1, 2)\n\
             val localSize = local.size\n\
             val repeat = local.size\n\
             val first = value.size\n\
             val second = (value).size\n\
             val arraySize = borrowed.size\n\
             val mutableSize = mutable.size\n\
             val moved = take(value)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "size-ownership.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .unwrap();
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert!(
        ownership.deferred().is_empty(),
        "{:?}",
        ownership.deferred()
    );
    assert_eq!(ownership.loans().len(), 6);
    assert!(
        ownership
            .loans()
            .iter()
            .all(|loan| loan.kind() == LoanKind::Shared)
    );
    for descriptor in typed.types().container_sizes() {
        let loan = ownership
            .loans()
            .iter()
            .find(|loan| loan.call() == descriptor.expression())
            .unwrap();
        assert_eq!(loan.argument(), descriptor.receiver());
        assert!(matches!(loan.target(), UnitLoanTarget::Place(_)));
    }
    let unit = source_unit(&names, source);
    let local = referenced_local_symbol(&names, unit, "local");
    let first_read =
        UnitExpressionId::new(unit, expression_with_text(&sources, &file, "local.size"));
    // The first read cannot destroy an owner required by the second read.
    assert!(
        !ownership
            .drops()
            .iter()
            .any(|drop| drop.target() == UnitDropTarget::Named(local)
                && drop.point() == UnitDropPoint::CallReturn(first_read))
    );
    assert!(ownership.validate().is_ok());
}

#[test]
fn temporary_resource_container_size_drops_its_owner_after_the_read() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "temporary-size.ko",
        "class Resource {}\n\
         fun size(): Unit { val length = (listOf(Resource())).size }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "temporary-size.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .unwrap();
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert!(
        ownership.deferred().is_empty(),
        "{:?}",
        ownership.deferred()
    );
    let descriptor = typed.types().container_sizes()[0];
    let loan = ownership
        .loans()
        .iter()
        .find(|loan| loan.call() == descriptor.expression())
        .unwrap();
    let UnitLoanTarget::Temporary(owner) = loan.target() else {
        panic!("temporary receiver owner")
    };
    let drops = ownership
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Temporary(*owner))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert_eq!(
        drops[0].point(),
        UnitDropPoint::CallReturn(descriptor.expression())
    );
    assert!(ownership.validate().is_ok());
}

#[test]
fn moved_owner_and_active_exclusive_loan_reject_container_size_reads() {
    for (text, expected) in [
        (
            "fun take(own value: List<Int>): Unit {}\nfun invalid(own value: List<Int>): Unit { val moved = take(value); val size = value.size }",
            "L0131",
        ),
        (
            "fun use(inout value: List<Int>, size: Int): Unit {}\nfun invalid(): Unit { var value = listOf(1); val result = use(&value, value.size) }",
            "L0135",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "invalid-size.ko", text);
        let inputs = [SourceUnitInput::new(
            "root",
            "invalid-size.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .unwrap();
        assert_eq!(
            ownership.diagnostics().len(),
            1,
            "{:?}",
            ownership.diagnostics()
        );
        assert_eq!(ownership.diagnostics()[0].code().to_string(), expected);
        assert_eq!(
            sources
                .slice(ownership.diagnostics()[0].primary_span())
                .unwrap(),
            "value"
        );
        assert!(ownership.loans().is_empty());
        assert!(ownership.drops().is_empty());
        assert!(ownership.validate().is_err());
    }
}

#[test]
fn container_size_in_assignment_rhs_keeps_old_owner_until_rhs_completes() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "replacement-size.ko",
        "fun replace(): Unit { var values = listOf(1); values = listOf(values.size) }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "replacement-size.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .unwrap();
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert!(
        ownership.deferred().is_empty(),
        "{:?}",
        ownership.deferred()
    );
    let descriptor = typed.types().container_sizes()[0];
    let loan = ownership
        .loans()
        .iter()
        .find(|loan| loan.call() == descriptor.expression())
        .unwrap();
    let UnitLoanTarget::Place(place) = loan.target() else {
        panic!("named receiver")
    };
    let rhs = UnitExpressionId::new(
        source_unit(&names, source),
        expression_with_text(&sources, &file, "listOf(values.size)"),
    );
    let old_drops = ownership
        .drops()
        .iter()
        .filter(|drop| {
            drop.target() == UnitDropTarget::Named(place.root())
                && sources.slice(drop.value_origin()).unwrap() == "values"
        })
        .collect::<Vec<_>>();
    assert_eq!(old_drops.len(), 1, "old owner must be dropped exactly once");
    assert_eq!(
        old_drops[0].point(),
        UnitDropPoint::AfterExpression(rhs),
        "the replacement container must be complete before destroying the old owner"
    );
    assert!(
        !ownership.drops().iter().any(|drop| {
            drop.target() == UnitDropTarget::Named(place.root())
                && drop.point() == UnitDropPoint::CallReturn(descriptor.expression())
        }),
        "a synchronous header read must not commit replacement cleanup"
    );
}

#[test]
fn container_size_assignment_rhs_return_cleans_up_the_abandoned_old_owner() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "replacement-return.ko",
        "fun replace(flag: Boolean): Unit {\n\
             var values = listOf(1)\n\
             values = if (flag) { listOf(values.size) } else { return }\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "replacement-return.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .unwrap();
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert!(
        ownership.deferred().is_empty(),
        "{:?}",
        ownership.deferred()
    );
    let unit = source_unit(&names, source);
    let root = referenced_local_symbol(&names, unit, "values");
    let rhs = UnitExpressionId::new(
        unit,
        expression_with_text(
            &sources,
            &file,
            "if (flag) { listOf(values.size) } else { return }",
        ),
    );
    let returned = UnitExpressionId::new(unit, expression_with_text(&sources, &file, "return"));
    let old_points = ownership
        .drops()
        .iter()
        .filter(|drop| {
            drop.target() == UnitDropTarget::Named(root)
                && sources.slice(drop.value_origin()).unwrap() == "values"
        })
        .map(|drop| drop.point())
        .collect::<Vec<_>>();
    assert_eq!(
        old_points.len(),
        2,
        "one old-owner drop on each mutually exclusive path"
    );
    assert!(old_points.contains(&UnitDropPoint::AfterExpression(rhs)));
    assert!(
        old_points.contains(&UnitDropPoint::ControlTransfer(returned)),
        "return must discharge the old owner even though replacement was not committed"
    );
}

#[test]
fn container_size_assignment_rhs_loop_jump_cleans_up_the_loop_local_old_owner() {
    for jump in ["break", "continue"] {
        let mut sources = SourceMap::new();
        let rhs_text = format!("if (choose) {{ listOf(values.size) }} else {{ {jump} }}");
        let text = format!(
            "fun replace(running: Boolean, choose: Boolean): Unit {{\n\
                 while (running) {{\n\
                     var values = listOf(1)\n\
                     values = {rhs_text}\n\
                 }}\n\
             }}"
        );
        let (source, file) = parsed(&mut sources, "replacement-jump.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "replacement-jump.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .unwrap();
        assert!(
            ownership.diagnostics().is_empty(),
            "{jump}: {:?}",
            ownership.diagnostics()
        );
        assert!(
            ownership.deferred().is_empty(),
            "{jump}: {:?}",
            ownership.deferred()
        );
        let unit = source_unit(&names, source);
        let root = referenced_local_symbol(&names, unit, "values");
        let rhs = UnitExpressionId::new(unit, expression_with_text(&sources, &file, &rhs_text));
        let jumped = UnitExpressionId::new(unit, expression_with_text(&sources, &file, jump));
        let old_points = ownership
            .drops()
            .iter()
            .filter(|drop| {
                drop.target() == UnitDropTarget::Named(root)
                    && sources.slice(drop.value_origin()).unwrap() == "values"
            })
            .map(|drop| drop.point())
            .collect::<Vec<_>>();
        assert_eq!(
            old_points.len(),
            2,
            "{jump}: one old-owner drop on each path"
        );
        assert!(
            old_points.contains(&UnitDropPoint::AfterExpression(rhs)),
            "{jump}"
        );
        assert!(
            old_points.contains(&UnitDropPoint::ControlTransfer(jumped)),
            "{jump}: abandoning this loop scope must release the old owner"
        );
        let assignment_text = format!("values = {rhs_text}");
        let assignment = UnitExpressionId::new(
            unit,
            expression_with_text(&sources, &file, &assignment_text),
        );
        let new_points = ownership
            .drops()
            .iter()
            .filter(|drop| {
                drop.target() == UnitDropTarget::Named(root)
                    && sources.slice(drop.value_origin()).unwrap() == assignment_text
            })
            .map(|drop| drop.point())
            .collect::<Vec<_>>();
        assert_eq!(
            new_points,
            [UnitDropPoint::AfterExpression(assignment)],
            "{jump}: only the completed replacement creates a new owner"
        );
        ownership
            .validate()
            .expect("complete loop-jump ownership facts");
    }
}

#[test]
fn container_size_assignment_rhs_inner_loop_jump_preserves_the_outer_old_owner() {
    for jump in ["break", "continue"] {
        let mut sources = SourceMap::new();
        let rhs_text = format!(
            "if (choose) {{ while (repeat) {{ val count = values.size; repeat = false; {jump} }}; listOf(2) }} else {{ listOf(3) }}"
        );
        let text = format!(
            "fun replace(running: Boolean, choose: Boolean): Unit {{\n\
                 var repeat = running\n\
                 var values = listOf(1)\n\
                 values = {rhs_text}\n\
             }}"
        );
        let (source, file) = parsed(&mut sources, "replacement-inner-loop.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "replacement-inner-loop.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .unwrap();
        assert!(
            ownership.diagnostics().is_empty(),
            "{jump}: {:?}",
            ownership.diagnostics()
        );
        assert!(
            ownership.deferred().is_empty(),
            "{jump}: {:?}",
            ownership.deferred()
        );
        let unit = source_unit(&names, source);
        let root = referenced_local_symbol(&names, unit, "values");
        let rhs = UnitExpressionId::new(unit, expression_with_text(&sources, &file, &rhs_text));
        let old_points = ownership
            .drops()
            .iter()
            .filter(|drop| {
                drop.target() == UnitDropTarget::Named(root)
                    && sources.slice(drop.value_origin()).unwrap() == "values"
            })
            .map(|drop| drop.point())
            .collect::<Vec<_>>();
        assert_eq!(
            old_points,
            [UnitDropPoint::AfterExpression(rhs)],
            "{jump}: the inner loop jump and loop exit cannot release the outer replacement target"
        );
        let assignment_text = format!("values = {rhs_text}");
        let assignment = UnitExpressionId::new(
            unit,
            expression_with_text(&sources, &file, &assignment_text),
        );
        let all_points = ownership
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(root))
            .map(|drop| drop.point())
            .collect::<Vec<_>>();
        assert_eq!(
            all_points,
            [
                UnitDropPoint::AfterExpression(rhs),
                UnitDropPoint::AfterExpression(assignment)
            ],
            "{jump}: old and replacement owners each retain their complete cleanup fact"
        );
        ownership
            .validate()
            .expect("complete outer replacement ownership facts");
    }
}

#[test]
fn container_size_last_read_preserves_the_outer_pending_shared_borrow() {
    for caller in [
        "fun run(): Unit { val values = listOf(1); use(values, values.size) }",
        "fun run(own values: List<Int>): Unit { use(values, values.size) }",
    ] {
        let mut sources = SourceMap::new();
        let text = format!("fun use(input: List<Int>, size: Int): Unit {{}}\n{caller}");
        let (source, file) = parsed(&mut sources, "pending-size.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "pending-size.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .unwrap();
        assert!(
            ownership.diagnostics().is_empty(),
            "{caller}: {:?}",
            ownership.diagnostics()
        );
        assert!(
            ownership.deferred().is_empty(),
            "{caller}: {:?}",
            ownership.deferred()
        );
        let unit = source_unit(&names, source);
        let root = referenced_local_symbol(&names, unit, "values");
        let outer = UnitExpressionId::new(
            unit,
            expression_with_text(&sources, &file, "use(values, values.size)"),
        );
        let size = typed.types().container_sizes()[0];
        let place_loans = ownership.loans().iter().filter(|loan| {
            matches!(loan.target(), UnitLoanTarget::Place(place) if place.root() == root)
        }).collect::<Vec<_>>();
        assert_eq!(
            place_loans.len(),
            2,
            "{caller}: outer and inner shared reads"
        );
        let inner_loan = place_loans
            .iter()
            .find(|loan| loan.call() == size.expression())
            .unwrap();
        let outer_loan = place_loans
            .iter()
            .find(|loan| loan.call() == outer)
            .unwrap();
        assert_eq!(inner_loan.kind(), LoanKind::Shared);
        assert_eq!(outer_loan.kind(), LoanKind::Shared);
        assert_eq!(inner_loan.argument(), size.receiver());
        assert_eq!(sources.slice(inner_loan.end_span()).unwrap(), "values.size");
        assert_eq!(
            sources.slice(outer_loan.end_span()).unwrap(),
            "use(values, values.size)"
        );
        let points = ownership
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(root))
            .map(|drop| drop.point())
            .collect::<Vec<_>>();
        assert_eq!(
            points,
            [UnitDropPoint::CallReturn(outer)],
            "{caller}: the inner size read cannot destroy the outer call's pending Borrow owner"
        );
        ownership
            .validate()
            .expect("complete pending shared-borrow facts");
    }
}
