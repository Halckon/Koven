use super::*;

#[test]
fn unit_non_null_assertion_consumption_and_source_restrictions() {
    // Source-qualified calls must enforce extraction independently of the result's Borrow mode.
    for (body, expected) in [
        (
            "fun test(own source: Resource?): Int { val first = read(source!!)\nreturn readNullable(source) }",
            vec!["L0131"],
        ),
        (
            "fun test(source: Resource?): Resource = source!!",
            vec!["L0133"],
        ),
        (
            "fun test(inout source: Resource?): Resource = source!!",
            vec!["L0133"],
        ),
        (
            "fun test(own source: Holder): Resource = source.item!!",
            vec!["L0132"],
        ),
        (
            "fun test(own source: Array<Resource?>): Resource = source[0]!!",
            vec!["L0136"],
        ),
        (
            "fun test(own source: Resource?): Int = use(source, source!!)",
            vec!["L0135"],
        ),
        (
            "fun test(source: Int?): Int { val first = source!!\nreturn first + source!! }",
            vec![],
        ),
    ] {
        let mut sources = SourceMap::new();
        let (provider_id, provider) = parsed(
            &mut sources,
            "provider.ko",
            "class Resource {}\nclass Holder(val item: Resource?) {}\nfun read(item: Resource): Int = 0\nfun readNullable(item: Resource?): Int = 0\nfun use(first: Resource?, second: Resource): Int = 0",
        );
        let (consumer_id, consumer) = parsed(&mut sources, "consumer.ko", body);
        let inputs = [
            SourceUnitInput::new("root", "provider.ko", provider_id, &provider),
            SourceUnitInput::new("root", "consumer.ko", consumer_id, &consumer),
        ];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");
        assert_eq!(diagnostic_codes(&ownership), expected, "{body}");
        assert_eq!(
            ownership.non_null_assertions().is_empty(),
            !expected.is_empty(),
            "{body}"
        );
        assert_eq!(ownership.validate().is_ok(), expected.is_empty(), "{body}");
    }
}

#[test]
fn unit_non_null_assertion_plans_bind_transfer_abort_and_result_drop() {
    use lang_frontend::{
        ownership_checking::NonNullAssertionTransferKind as Transfer,
        type_checking::AssertionFailureEffect,
    };
    let mut sources = SourceMap::new();
    let (provider_id, provider) = parsed(
        &mut sources,
        "assert-provider.ko",
        "class Resource {}\nfun create(): Resource? = Resource()\nfun read(item: Resource): Int = 0\nfun copied(source: Int?): Int = source!!",
    );
    let (consumer_id, consumer) = parsed(
        &mut sources,
        "assert-consumer.ko",
        "fun transferred(own source: Resource?): Resource = source!!\nfun temporary(): Int = read(create()!!)\nfun borrowedResult(own source: Resource?): Int = read(source!!)",
    );
    let inputs = [
        SourceUnitInput::new("root", "assert-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "assert-consumer.ko", consumer_id, &consumer),
    ];
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
    assert_eq!(ownership.non_null_assertions().len(), 4);
    for plan in ownership.non_null_assertions() {
        let descriptor = plan.descriptor();
        assert_eq!(
            typed.types().non_null_assertion(descriptor.expression()),
            Some(*descriptor)
        );
        assert_eq!(
            ownership.non_null_assertion(descriptor.expression()),
            Some(plan)
        );
        assert_eq!(plan.null_effect(), AssertionFailureEffect::Abort);
        assert_eq!(
            plan.non_null_transfer(),
            if descriptor.copyability() == Copyability::Copyable {
                Transfer::Copy
            } else {
                Transfer::Consume
            }
        );
    }
    let consumer_unit = source_unit(&names, consumer_id);
    let temporary = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "create()!!"),
    );
    assert!(
        ownership
            .non_null_assertion(temporary)
            .unwrap()
            .source_place()
            .is_none()
    );
    let borrowed_drops = ownership
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), UnitDropTarget::Temporary(_)))
        .collect::<Vec<_>>();
    assert_eq!(borrowed_drops.len(), 2, "{:?}", ownership.drops());
    assert!(
        borrowed_drops
            .iter()
            .all(|fact| matches!(fact.point(), UnitDropPoint::CallReturn(_)))
    );
    assert!(
        ownership
            .drops()
            .iter()
            .all(|fact| sources.slice(fact.value_origin()).unwrap() != "source")
    );
    let again = check_compilation_unit_ownership(
        &sources,
        &[inputs[1], inputs[0]],
        &names,
        &type_environment,
        &typed,
    )
    .unwrap();
    assert_eq!(ownership.non_null_assertions(), again.non_null_assertions());
    assert!(ownership.validate().is_ok());
}

#[test]
fn unit_non_null_assertion_plans_clear_after_later_assignment_failure() {
    // The RHS extraction must not survive a rejected immutable target in a validated product.
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "assert-assignment.ko",
        "class Resource {}\nfun copied(source: Int?): Int = source!!\nfun invalid(own source: Resource?): Unit { val target = Resource()\n{ target = source!! } }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "assert-assignment.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .unwrap();
    assert_eq!(diagnostic_codes(&ownership), ["L0134"]);
    assert!(ownership.non_null_assertions().is_empty());
    assert!(ownership.validate().is_err());
}

#[test]
fn unit_non_null_assertion_plans_preserve_nested_and_assignment_transfers() {
    // A returned conditional owner and a replacement RHS each transfer exactly once.
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "assert-control.ko",
        "class Resource {}\nfun selected(flag: Boolean, own left: Resource?, own right: Resource?): Resource = (if (flag) { left } else { right })!!\nfun replaced(own source: Resource?): Resource { var target = Resource()\n{ target = source!! }\nreturn target }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "assert-control.ko",
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
    assert_eq!(ownership.non_null_assertions().len(), 2);
    assert!(
        ownership
            .drops()
            .iter()
            .all(|fact| !matches!(fact.target(), UnitDropTarget::Temporary(_)))
    );
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter(|fact| sources.slice(fact.value_origin()).unwrap() == "target")
            .count(),
        1
    );
    assert!(
        ownership
            .drops()
            .iter()
            .all(|fact| sources.slice(fact.value_origin()).unwrap() != "source")
    );
    assert!(ownership.validate().is_ok());
}

#[test]
fn unit_non_null_assertion_loop_consumption_checks_only_reachable_backedges() {
    // A repeated extraction requires a restored owner; break and return cannot repeat it.
    for (body, expected) in [
        ("while (flag) { val item = source!! }", vec!["L0131"]),
        (
            "while (flag) { val item = source!!\ncontinue }",
            vec!["L0131"],
        ),
        ("loop { val item = source!! }", vec!["L0131"]),
        (
            "for (index in indices) { val item = source!! }",
            vec!["L0131"],
        ),
        ("while (check(source!!)) {}", vec!["L0131"]),
        ("while (flag) { val item = source!!\nbreak }", vec![]),
        ("loop { val item = source!!\nreturn 0 }", vec![]),
        (
            "while (flag) { val local: Resource? = Resource()\nval item = local!! }",
            vec![],
        ),
        (
            "var local = source\nwhile (flag) { val item = local!!\n{ local = Resource() } }",
            vec![],
        ),
        (
            "var local = source\nwhile (flag) { { local = Resource() }\nval item = local!! }",
            vec![],
        ),
        (
            "var local = source\nloop { { local = Resource() }\nif (flag) { break }\nval item = local!! }\nval result = local!!",
            vec![],
        ),
    ] {
        let mut sources = SourceMap::new();
        let (provider_id, provider) = parsed(
            &mut sources,
            "provider.ko",
            "class Resource {}\nfun check(item: Resource): Boolean = true",
        );
        let text = format!(
            "fun test(flag: Boolean, indices: Array<Int>, own source: Resource?): Int {{ {body}\nreturn 0 }}"
        );
        let (consumer_id, consumer) = parsed(&mut sources, "consumer.ko", &text);
        let inputs = [
            SourceUnitInput::new("root", "provider.ko", provider_id, &provider),
            SourceUnitInput::new("root", "consumer.ko", consumer_id, &consumer),
        ];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");
        assert_eq!(diagnostic_codes(&ownership), expected, "{body}");
        if !expected.is_empty() {
            assert_eq!(
                sources
                    .slice(ownership.diagnostics()[0].primary_span())
                    .unwrap(),
                "source"
            );
            assert!(ownership.non_null_assertions().is_empty());
        }
        assert_eq!(ownership.validate().is_ok(), expected.is_empty(), "{body}");
    }
}
