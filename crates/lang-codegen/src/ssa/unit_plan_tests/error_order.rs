use super::*;

#[test]
fn keeps_delegation_owner_field_error_before_endpoint_recipe_failure() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun read(): Int = 7 }\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Reader<T>: Base<GrowA<T>> {}\n\
         class Host<T>(val bad: Array<T>, val reader: Reader<T>): Base<GrowA<T>> by reader {}\n\
         fun entry(host: Host<Int>): Int = host.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let bad = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Host"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.fields().first())
        .expect("Host.bad field")
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
    .expect_err("the formal resolver must retain the earlier owner field failure");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(bad));
}

#[test]
fn keeps_earlier_unsupported_constructor_before_later_recipe_cycle() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun read(): Int = 7 }\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Bad<T>(val first: Array<T>, val next: GrowA<T>)\n\
         class Host<T>: Base<T> {}\n\
         fun entry(host: Host<Bad<Int>>): Int = host.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let first_field = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Bad"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.fields().first())
        .expect("Bad.first field")
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
    .expect_err("ordinary unsupported constructor must retain source-order priority");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(first_field));
}

#[test]
fn keeps_earlier_call_error_before_later_recipe_failure() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class First(val second: Second): Readable by second {}\n\
         class Second(val first: First): Readable by first {}\n\
         interface Base<A> {\n\
             fun readGrowth(): Int\n\
             fun throughRequirement(): Int = this.readGrowth()\n\
         }\n\
         interface Derived<X>: Base<String> { fun readGrowth(): Int = 7 }\n\
         class Wrapper<T>(val item: T)\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Host<T>: Derived<Wrapper<GrowA<T>>> {}\n\
         fun badGrowth(host: Host<Int>): Int = host.throughRequirement()\n\
         fun entry(first: First, host: Host<Int>): Int =\n\
             first.read() + badGrowth(host)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let first = declaration(&names, "First");
    let ordinary_call = typed
        .types()
        .calls()
        .iter()
        .find(|call| {
            call.receiver().is_some_and(|receiver| {
                matches!(
                    typed.types().types().get(receiver.ty()),
                    Some(UnitTypeKind::Nominal { declaration, .. }) if *declaration == first
                )
            })
        })
        .expect("First.read call");
    let ordinary_span = parsed
        .ast()
        .expressions()
        .get(ordinary_call.expression().expression())
        .expect("First.read call expression")
        .span();
    let recipe_span = typed
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
    .expect_err("the first ordinary call error must remain observable");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(ordinary_span));
    assert_ne!(error.span, Some(recipe_span));
}

#[test]
fn keeps_instance_limit_before_concrete_error_in_generic_body() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class First(val second: Second): Readable by second {}\n\
         class Second(val first: First): Readable by first {}\n\
         fun <T> relay(first: First): Int = first.read()\n\
         fun entry(first: First): Int = relay<Int>(first)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

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
    .expect_err("a specialized template must pass the instance-limit gate before its body");
    assert_eq!(error.kind, LoweringErrorKind::InstanceLimitExceeded);
}

#[test]
fn keeps_earlier_sibling_instance_limit_before_helper_error() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class First(val second: Second): Readable by second {}\n\
         class Second(val first: First): Readable by first {}\n\
         fun <T> generic(): Int = 0\n\
         fun helper(first: First): Int = first.read()\n\
         fun entry(first: First): Int = helper(first) + generic<Int>()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let generic = declaration(&names, "generic");
    let helper = declaration(&names, "helper");
    assert!(
        generic < helper,
        "fixture must put the specialized key first"
    );

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
    .expect_err("the earlier sibling specialization must retain planner priority");
    assert_eq!(error.kind, LoweringErrorKind::InstanceLimitExceeded);
}

#[test]
fn keeps_earlier_helper_error_before_later_recipe_failure() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class First(val second: Second): Readable by second {}\n\
         class Second(val first: First): Readable by first {}\n\
         fun helper(first: First): Int = first.read()\n\
         interface Base<A> { fun readGrowth(): Int = 7 }\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Host<T>: Base<T> {}\n\
         fun <T> relay(host: Host<T>): Int = host.readGrowth()\n\
         fun entry(first: First, host: Host<GrowA<Int>>): Int =\n\
             helper(first) + relay(host)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let helper = declaration(&names, "helper");
    let relay = declaration(&names, "relay");
    assert!(helper < relay, "fixture must put the ordinary helper first");
    let first = declaration(&names, "First");
    let ordinary_call = typed
        .types()
        .calls()
        .iter()
        .find(|call| {
            call.receiver().is_some_and(|receiver| {
                matches!(
                    typed.types().types().get(receiver.ty()),
                    Some(UnitTypeKind::Nominal { declaration, .. }) if *declaration == first
                )
            })
        })
        .expect("First.read call");
    let ordinary_span = parsed
        .ast()
        .expressions()
        .get(ordinary_call.expression().expression())
        .expect("First.read call expression")
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
    .expect_err("the earlier ordinary helper must stop recipe preflight");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(ordinary_span));
}

#[test]
fn keeps_first_specialization_error_before_later_specialization_recipe() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class First(val second: Second): Readable by second {}\n\
         class Second(val first: First): Readable by first {}\n\
         interface Base<A> { fun readGrowth(): Int = 7 }\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Host<T>: Base<T> {}\n\
         fun <T> relay(first: First, host: Host<T>): Int {\n\
             host.readGrowth()\n\
             return first.read()\n\
         }\n\
         fun entry(first: First, good: Host<Int>, bad: Host<GrowA<Int>>): Int =\n\
             relay(first, good) + relay(first, bad)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let first = declaration(&names, "First");
    let ordinary_call = typed
        .types()
        .calls()
        .iter()
        .find(|call| {
            call.receiver().is_some_and(|receiver| {
                matches!(
                    typed.types().types().get(receiver.ty()),
                    Some(UnitTypeKind::Nominal { declaration, .. }) if *declaration == first
                )
            })
        })
        .expect("First.read call");
    let ordinary_span = parsed
        .ast()
        .expressions()
        .get(ordinary_call.expression().expression())
        .expect("First.read call expression")
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
    .expect_err("the first concrete specialization must retain its ordinary error");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(ordinary_span));
}

#[test]
fn keeps_declaration_frontier_before_earlier_source_symbol_recipe() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class First(val second: Second): Readable by second {}\n\
         class Second(val first: First): Readable by first {}\n\
         interface Base<A> { fun readGrowth(): Int = 7 }\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Host<T>: Base<T> {}\n\
         class Runner {\n\
             fun relay(host: Host<GrowA<Int>>): Int = host.readGrowth()\n\
         }\n\
         fun helper(first: First): Int = first.read()\n\
         fun entry(runner: Runner, first: First, bad: Host<GrowA<Int>>): Int =\n\
             runner.relay(bad) + helper(first)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let first = declaration(&names, "First");
    let ordinary_call = typed
        .types()
        .calls()
        .iter()
        .find(|call| {
            call.receiver().is_some_and(|receiver| {
                matches!(
                    typed.types().types().get(receiver.ty()),
                    Some(UnitTypeKind::Nominal { declaration, .. }) if *declaration == first
                )
            })
        })
        .expect("First.read call");
    let ordinary_span = parsed
        .ast()
        .expressions()
        .get(ordinary_call.expression().expression())
        .expect("First.read call expression")
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
    .expect_err("Declaration keys must remain ahead of Symbol keys");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(ordinary_span));
}

#[test]
fn rejects_later_sibling_recipe_before_current_instance_limit() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun readGrowth(): Int = 7 }\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Host<T>: Base<GrowA<T>> {}\n\
         fun <T> clean(): Int = 0\n\
         fun <T> bad(host: Host<T>): Int = host.readGrowth()\n\
         fun entry(host: Host<Int>): Int = clean<Int>() + bad(host)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let clean = declaration(&names, "clean");
    let bad = declaration(&names, "bad");
    assert!(clean < bad, "fixture must put the clean key first");
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
    .expect_err("a later pending recipe must not be hidden by the current limit");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(witness));
}

#[test]
fn selects_stable_recipe_root_across_limit_hit_siblings() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun readGrowth(): Int = 7 }\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class GrowZ<T>(val next: GrowY<List<T>>)\n\
         class GrowY<U>(val next: GrowZ<U>)\n\
         class HostA<T>: Base<GrowA<T>> {}\n\
         class HostZ<T>: Base<GrowZ<T>> {}\n\
         fun <T> badZ(host: HostZ<T>): Int = host.readGrowth()\n\
         fun <T> badA(host: HostA<T>): Int = host.readGrowth()\n\
         fun entry(z: HostZ<Int>, a: HostA<Int>): Int = badZ(z) + badA(a)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let bad_z = declaration(&names, "badZ");
    let bad_a = declaration(&names, "badA");
    assert!(bad_z < bad_a, "fixture must put the Z key first");
    let a_witness = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "GrowB"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.fields().first())
        .expect("GrowB.next field")
        .span();
    let z_witness = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "GrowY"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.fields().first())
        .expect("GrowY.next field")
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
        let error = result
            .expect_err("normal and limited planning must select the same stable recipe root");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{error:?}");
        assert_eq!(error.span, Some(a_witness));
        assert_ne!(error.span, Some(z_witness));
    }
}
