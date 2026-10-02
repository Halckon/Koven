use super::*;

#[test]
fn rejects_parameter_growing_recipe_at_stable_back_edge_across_input_order() {
    let mut sources = SourceMap::new();
    let (a_source, a_file) = parsed(
        &mut sources,
        "p/a-grow.ko",
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface DerivedA<X>: Base<String> { fun read(): Int = 7 }\n\
         class Wrapper<T>(val item: T)\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class HostA<T>: DerivedA<Wrapper<GrowA<T>>> {}\n\
         fun badA(host: HostA<Int>): Int = host.throughRequirement()",
    );
    let (z_source, z_file) = parsed(
        &mut sources,
        "p/z-grow.ko",
        "package p\n\
         interface DerivedZ<X>: Base<String> { fun read(): Int = 9 }\n\
         class GrowZ<T>(val next: GrowY<List<T>>)\n\
         class GrowY<U>(val next: GrowZ<U>)\n\
         class HostZ<T>: DerivedZ<Wrapper<GrowZ<T>>> {}\n\
         fun badZ(host: HostZ<Int>): Int = host.throughRequirement()",
    );
    let (entry_source, entry_file) = parsed(
        &mut sources,
        "p/entry.ko",
        "package p\n\
         fun entry(z: HostZ<Int>, a: HostA<Int>): Int = badZ(z) + badA(a)\n\
         fun direct(a: HostA<Int>): Int = a.read()",
    );
    let inputs = [
        // z-grow call 与 input 都在前；稳定 source identity 必须仍选择 a-grow 的 witness。
        SourceUnitInput::new("root", "p/z-grow.ko", z_source, &z_file),
        SourceUnitInput::new("root", "p/a-grow.ko", a_source, &a_file),
        SourceUnitInput::new("root", "p/entry.ko", entry_source, &entry_file),
    ];
    let reversed_inputs = [inputs[2], inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let witness = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "GrowB"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.fields().first())
        .expect("GrowB.next field")
        .span();
    let entry = declaration(&names, "entry");
    let direct = declaration(&names, "direct");

    for unit_inputs in [&inputs[..], &reversed_inputs[..]] {
        let errors = [
            plan_unit_instances(
                &sources,
                unit_inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                entry,
            ),
            plan_unit_instances_with_limit(
                &sources,
                unit_inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                entry,
                0,
            ),
        ];
        for result in errors {
            let error = result
                .expect_err("recipe failure must precede planning and the generic instance limit");
            assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
            assert_eq!(error.span, Some(witness));
        }
        let direct_error = plan_unit_instances(
            &sources,
            unit_inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            direct,
        )
        .expect_err("direct bodyful inherited target must use the same recipe preflight");
        assert_eq!(direct_error.kind, LoweringErrorKind::UnsupportedNode);
        assert_eq!(direct_error.span, Some(witness));
    }
}

#[test]
fn rejects_parameter_growing_recipe_at_delegation_endpoint() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun read(): Int = 7 }\n\
         class Wrapper<T>(val item: T)\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Reader<T>: Base<Wrapper<GrowA<T>>> {}\n\
         class Host<T>(val reader: Reader<T>): Base<Wrapper<GrowA<T>>> by reader {}\n\
         fun entry(host: Host<Int>): Int = host.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let witness = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "GrowB"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.fields().first())
        .expect("GrowB.next field")
        .span();

    let error = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect_err("delegation endpoint must not bypass inherited recipe preflight");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(witness));
}

#[test]
fn rejects_generic_delegation_recipe_before_instance_limit() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun read(): Int = 7 }\n\
         class Wrapper<T>(val item: T)\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Reader<T>: Base<Wrapper<GrowA<T>>> {}\n\
         class Host<T>(val reader: Reader<T>): Base<Wrapper<GrowA<T>>> by reader {}\n\
         fun <T> relay(host: Host<T>): Int = host.read()\n\
         fun entry(host: Host<Int>): Int = relay(host)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let witness = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "GrowB"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.fields().first())
        .expect("GrowB.next field")
        .span();
    let entry = declaration(&names, "entry");

    let results = [
        (
            "normal",
            plan_unit_instances(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                entry,
            ),
        ),
        (
            "limit",
            plan_unit_instances_with_limit(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                entry,
                0,
            ),
        ),
    ];
    for (mode, result) in results {
        let error = result.expect_err(
            "normal and limited generic delegation must select the same recipe witness",
        );
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        assert_eq!(error.span, Some(witness), "{mode}: {error:?}");
    }
}

#[test]
fn keeps_generic_delegation_local_override_out_of_recipe_failures() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun read(): Int = 7 }\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Reader<T>: Base<GrowA<T>> { override fun read(): Int = 9 }\n\
         class Host<T>(val reader: Reader<T>): Base<GrowA<T>> by reader {}\n\
         fun seed(growth: GrowA<Int>): Unit {}\n\
         fun <T> relay(host: Host<T>): Int = host.read()\n\
         fun entry(host: Host<Int>): Int = relay(host)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "entry");

    plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("a delegate local override must not instantiate the inherited owner recipe");

    let error = plan_unit_instances_with_limit(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
        0,
    )
    .expect_err("the generic relay should still respect the concrete instance limit");
    assert_eq!(error.kind, LoweringErrorKind::InstanceLimitExceeded);
}

#[test]
fn rejects_generic_delegation_recipe_across_helper_before_instance_limit() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun read(): Int = 7 }\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Reader<T>: Base<T> {}\n\
         class Host<T>(val reader: Reader<T>): Base<T> by reader {}\n\
         fun <T> inner(host: Host<T>): Int = host.read()\n\
         fun <T> outer(host: Host<T>): Int = inner(host)\n\
         fun entry(host: Host<GrowA<Int>>): Int = outer(host)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let witness = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "GrowB"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.fields().first())
        .expect("GrowB.next field")
        .span();
    let entry = declaration(&names, "entry");

    let results = [
        plan_unit_instances(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        ),
        plan_unit_instances_with_limit(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
            0,
        ),
    ];
    for result in results {
        let error = result.expect_err(
            "normal and limited helper traversal must retain delegated owner recipe facts",
        );
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        assert_eq!(error.span, Some(witness));
    }
}

#[test]
fn rejects_generic_helper_recipe_before_instance_limit() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun read(): Int = 7 }\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Host<T>: Base<T> {}\n\
         fun <T> relay(host: Host<T>): Int = host.read()\n\
         fun entry(host: Host<GrowA<Int>>): Int = relay(host)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let witness = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "GrowB"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.fields().first())
        .expect("GrowB.next field")
        .span();

    let error = plan_unit_instances_with_limit(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
        0,
    )
    .expect_err("generic recipe facts must be propagated before the instance limit");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(witness));
}

#[test]
fn rejects_fixed_argument_recipe_scc_before_instance_limit() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun read(): Int = 7 }\n\
         class Fixed<T>(val next: Fixed<Int>)\n\
         class Left<T>(val next: Right<Int>)\n\
         class Right<U>(val next: Left<String>)\n\
         class Host<T>: Base<T> {}\n\
         fun <T> relay(host: Host<T>): Int = host.read()\n\
         fun entrySelf(host: Host<Fixed<String>>): Int = relay(host)\n\
         fun entryMutual(host: Host<Left<Long>>): Int = relay(host)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    for (entry, witness_owner) in [("entrySelf", "Fixed"), ("entryMutual", "Right")] {
        let witness = typed
            .types()
            .signatures()
            .declaration(declaration(&names, witness_owner))
            .and_then(|signature| signature.nominal())
            .and_then(|nominal| nominal.fields().first())
            .expect("cycle back-edge field")
            .span();
        let error = plan_unit_instances_with_limit(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, entry),
            0,
        )
        .expect_err("fixed-argument SCC must be rejected before the instance limit");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        assert_eq!(error.span, Some(witness));
    }
}

#[test]
fn keeps_closed_descriptor_unsupported_out_of_cycle_failures() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun read(): Int = 7 }\n\
         class Holder<T>(val item: Array<T>)\n\
         class Outer<T>(val passthrough: T, val fixed: Holder<Int>)\n\
         class Host<T>: Base<T> {}\n\
         fun seed(holder: Holder<Int>): Unit {}\n\
         fun <T> relay(host: Host<T>): Int = host.read()\n\
         fun entryDirect(host: Host<Outer<String>>, holder: Holder<Int>): Int {\n\
             seed(holder)\n\
             return host.read()\n\
         }\n\
         fun entryLimit(host: Host<Outer<String>>): Int = relay(host)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let item = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Holder"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.fields().first())
        .expect("Holder.item field")
        .span();

    let direct_error = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entryDirect"),
    )
    .expect_err("closed descriptor shape error must remain an ordinary planner failure");
    assert_eq!(direct_error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(direct_error.span, Some(item));

    let limit_error = plan_unit_instances_with_limit(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entryLimit"),
        0,
    )
    .expect_err("closed descriptor error must not be mislabeled as a preflight cycle");
    assert_eq!(limit_error.kind, LoweringErrorKind::InstanceLimitExceeded);
}
