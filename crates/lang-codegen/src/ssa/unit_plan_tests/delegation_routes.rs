use super::*;

#[test]
fn plans_concrete_override_instead_of_abstract_requirement() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         class Child: Readable { override fun read(): Int = 7 }\n\
         fun entry(): Int = Child().throughRequirement()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let readable = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Readable"))
        .and_then(|signature| signature.nominal())
        .expect("Readable signature");
    let child = typed
        .types()
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

    let instances = plan(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    );
    assert!(
        instances
            .iter()
            .any(|instance| instance.key().target() == implementation)
    );
    assert!(
        instances
            .iter()
            .all(|instance| instance.key().target() != requirement),
        "abstract declarations must never become lowering templates"
    );
}

#[test]
fn plans_delegate_implementation_from_validated_forwarder_route() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class Reader: Readable { override fun read(): Int = 7 }\n\
         class Host(val delegate: Reader): Readable by delegate {}\n\
         fun entry(host: Host): Int = host.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let readable = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Readable"))
        .and_then(|signature| signature.nominal())
        .expect("Readable signature");
    let reader = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Reader"))
        .and_then(|signature| signature.nominal())
        .expect("Reader signature");
    let requirement = readable.members()[0].target();
    let implementation = reader.members()[0].target();

    let instances = plan(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    );
    assert!(
        instances
            .iter()
            .any(|instance| instance.key().target() == implementation)
    );
    assert!(
        instances
            .iter()
            .all(|instance| instance.key().target() != requirement),
        "delegated abstract requirement must not become a lowering template"
    );
}

#[test]
fn plans_same_requirement_delegation_chain_to_the_direct_implementation() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class Reader: Readable { override fun read(): Int = 7 }\n\
         class Middle(val reader: Reader): Readable by reader {}\n\
         class Host(val middle: Middle): Readable by middle {}\n\
         fun entry(host: Host): Int = host.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let implementation = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Reader"))
        .and_then(|signature| signature.nominal())
        .expect("Reader signature")
        .members()[0]
        .target();
    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("same-requirement delegation chain must resolve to its direct endpoint");
    assert!(
        instances
            .iter()
            .any(|instance| instance.key().target() == implementation)
    );
}

#[test]
fn bodyful_requirement_chain_prefers_the_nested_route_over_an_inherited_default() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int = 1 }\n\
         class Reader: Readable { override fun read(): Int = 7 }\n\
         class Middle(val reader: Reader): Readable by reader {}\n\
         class Host(val middle: Middle): Readable by middle {}\n\
         fun entry(host: Host): Int = host.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let readable = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Readable"))
        .and_then(|signature| signature.nominal())
        .expect("Readable signature");
    let reader = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Reader"))
        .and_then(|signature| signature.nominal())
        .expect("Reader signature");
    let default = readable.members()[0].target();
    let endpoint = reader.members()[0].target();
    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("bodyful same-requirement chain must follow the nested route");
    assert!(
        instances
            .iter()
            .any(|instance| instance.key().target() == endpoint)
    );
    assert!(
        instances
            .iter()
            .all(|instance| instance.key().target() != default),
        "the inherited outer default must not truncate the delegation chain"
    );
}

#[test]
fn plans_identity_changing_delegation_chain_to_the_direct_endpoint() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base { fun read(): Int = 1 }\n\
         interface Derived: Base { fun read(): Int = 2 }\n\
         class Reader: Derived { override fun read(): Int = 7 }\n\
         class Middle(val reader: Reader): Derived by reader {}\n\
         class Host(val middle: Middle): Base by middle {}\n\
         fun entry(host: Host): Int = host.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let signatures = typed.types().signatures();
    let base_default = signatures
        .declaration(declaration(&names, "Base"))
        .and_then(|signature| signature.nominal())
        .expect("Base signature")
        .members()[0]
        .target();
    let derived_default = signatures
        .declaration(declaration(&names, "Derived"))
        .and_then(|signature| signature.nominal())
        .expect("Derived signature")
        .members()[0]
        .target();
    let endpoint = signatures
        .declaration(declaration(&names, "Reader"))
        .and_then(|signature| signature.nominal())
        .expect("Reader signature")
        .members()[0]
        .target();
    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("identity-changing chain must consume the exact frontend next-hop fact");
    assert!(
        instances
            .iter()
            .any(|instance| instance.key().target() == endpoint)
    );
    assert!(instances.iter().all(|instance| !matches!(
        instance.key().target(),
        target if target == base_default || target == derived_default
    )));
}

#[test]
fn remaps_identity_changing_next_hop_owner_prefix_and_callable_suffix() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> { fun <R> map(own input: R): R = input }\n\
         interface Derived<X, Y>: Base<Y> { fun <R> map(own input: R): R = input }\n\
         class Reader: Derived<String, Long> {}\n\
         class Middle(val reader: Reader): Derived<String, Long> by reader {}\n\
         class Host(val middle: Middle): Base<Long> by middle {}\n\
         fun entry(host: Host): Int = host.map<Int>(7)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let derived = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Derived"))
        .and_then(|signature| signature.nominal())
        .expect("Derived signature");
    let endpoint = derived.members()[0].target();

    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("generic identity-changing next hop must remap its slots");
    let endpoint = instances
        .iter()
        .find(|instance| instance.key().target() == endpoint)
        .expect("Derived default endpoint");
    assert!(matches!(
        endpoint
            .key()
            .type_arguments()
            .iter()
            .map(|argument| typed.types().types().get(*argument))
            .collect::<Vec<_>>()
            .as_slice(),
        [
            Some(UnitTypeKind::Builtin(BuiltinType::String)),
            Some(UnitTypeKind::Builtin(BuiltinType::Long)),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ]
    ));
}

#[test]
fn nested_delegate_local_override_terminates_the_outer_route() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int = 1 }\n\
         class Reader: Readable { override fun read(): Int = 7 }\n\
         class Middle(val reader: Reader): Readable by reader {\n\
             override fun read(): Int = 9\n\
         }\n\
         class Host(val middle: Middle): Readable by middle {}\n\
         fun entry(host: Host): Int = host.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let middle = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Middle"))
        .and_then(|signature| signature.nominal())
        .expect("Middle signature");
    let override_target = middle.members()[0].target();
    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("nested delegate local override must terminate the outer route");
    assert!(
        instances
            .iter()
            .any(|instance| instance.key().target() == override_target)
    );
}

#[test]
fn rejects_delegation_cycle_before_planning_a_partial_instance() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class First(val second: Second): Readable by second {}\n\
         class Second(val first: First): Readable by first {}\n\
         fun entry(first: First): Int = first.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let error = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect_err("delegation cycle must fail before publishing a partial instance");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}

#[test]
fn plans_parameter_independent_generic_delegate_field() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class Reader<T>: Readable { override fun read(): Int = 7 }\n\
         class Host(val delegate: Reader<Int>): Readable by delegate {}\n\
         fun entry(host: Host): Int = host.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let reader = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Reader"))
        .and_then(|signature| signature.nominal())
        .expect("Reader signature");
    let implementation = reader.members()[0].target();
    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("parameter-independent generic delegate layout must be routable");
    assert!(
        instances
            .iter()
            .any(|instance| instance.key().target() == implementation)
    );
    let resolved = only_member_call_route(&parsed, &typed, &owned);
    let [route] = resolved.delegation() else {
        panic!("one generic delegate route");
    };
    assert!(matches!(
        typed.types().types().get(route.outer_receiver()),
        Some(UnitTypeKind::Nominal { declaration: owner, arguments })
            if *owner == declaration(&names, "Host") && arguments.is_empty()
    ));
    assert!(matches!(
        typed.types().types().get(route.delegate_receiver()),
        Some(UnitTypeKind::Nominal { declaration: delegate, arguments })
            if *delegate == declaration(&names, "Reader")
                && arguments.as_slice()
                    == [typed.types().types().builtin(BuiltinType::Int).expect("Int type")]
    ));
}

#[test]
fn plans_parameter_independent_generic_outer_receiver() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class Reader: Readable { override fun read(): Int = 7 }\n\
         class Host<T>(val delegate: Reader): Readable by delegate {}\n\
         fun entry(host: Host<String>): Int = host.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let reader = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Reader"))
        .and_then(|signature| signature.nominal())
        .expect("Reader signature");
    let implementation = reader.members()[0].target();
    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("parameter-independent generic outer layout must be routable");
    assert!(
        instances
            .iter()
            .any(|instance| instance.key().target() == implementation)
    );
    let resolved = only_member_call_route(&parsed, &typed, &owned);
    let [route] = resolved.delegation() else {
        panic!("one generic outer route");
    };
    assert!(matches!(
        typed.types().types().get(route.outer_receiver()),
        Some(UnitTypeKind::Nominal { declaration: owner, arguments })
            if *owner == declaration(&names, "Host")
                && arguments.as_slice()
                    == [typed.types().types().builtin(BuiltinType::String).expect("String type")]
    ));
    assert!(matches!(
        typed.types().types().get(route.delegate_receiver()),
        Some(UnitTypeKind::Nominal { declaration: delegate, arguments })
            if *delegate == declaration(&names, "Reader") && arguments.is_empty()
    ));
}

#[test]
fn plans_frontend_authorized_nested_generic_delegate_layout() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class Reader<T>(val item: T): Readable { override fun read(): Int = 7 }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<T>(val delegate: Reader<Wrapper<T>>): Readable by delegate {}\n\
         fun entry(host: Host<Int>): Int = host.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let reader = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Reader"))
        .and_then(|signature| signature.nominal())
        .expect("Reader signature");
    let implementation = reader.members()[0].target();
    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("SPEC-0219 exact Host<Int> field layout must route to Reader<Int>");
    assert!(
        instances
            .iter()
            .any(|instance| instance.key().target() == implementation)
    );
    let resolved = only_member_call_route(&parsed, &typed, &owned);
    let [route] = resolved.delegation() else {
        panic!("one nested generic delegate route");
    };
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");
    assert!(matches!(
        typed.types().types().get(route.outer_receiver()),
        Some(UnitTypeKind::Nominal { declaration: owner, arguments })
            if *owner == declaration(&names, "Host") && arguments == &[int]
    ));
    assert!(matches!(
        typed.types().types().get(route.delegate_receiver()),
        Some(UnitTypeKind::Nominal { declaration: delegate, arguments })
            if *delegate == declaration(&names, "Reader")
                && matches!(
                    arguments.as_slice(),
                    [argument]
                        if matches!(
                            typed.types().types().get(*argument),
                            Some(UnitTypeKind::Nominal {
                                declaration: wrapper,
                                arguments: wrapper_arguments,
                            }) if *wrapper == declaration(&names, "Wrapper")
                                && wrapper_arguments == &[int]
                        )
                )
    ));
}

#[test]
fn remaps_generic_delegation_owner_prefix_and_callable_suffix() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Mapper<T> { fun <R> map(own input: R): R = input }\n\
         class DefaultMapper: Mapper<String> {}\n\
         class OverrideMapper: Mapper<String> {\n\
             override fun <R> map(own input: R): R = input\n\
         }\n\
         class DefaultHost(val delegate: DefaultMapper): Mapper<String> by delegate {}\n\
         class OverrideHost(val delegate: OverrideMapper): Mapper<String> by delegate {}\n\
         fun entry(first: DefaultHost, second: OverrideHost): Long =\n\
             first.map<Long>(7L) + second.map<Long>(9L)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let mapper = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Mapper"))
        .and_then(|signature| signature.nominal())
        .expect("Mapper signature");
    let default_mapper = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "DefaultMapper"))
        .and_then(|signature| signature.nominal())
        .expect("DefaultMapper signature");
    let override_mapper = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "OverrideMapper"))
        .and_then(|signature| signature.nominal())
        .expect("OverrideMapper signature");
    let requirement = mapper.members()[0].target();
    let override_target = override_mapper.members()[0].target();

    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("generic interface owner/callable delegation must plan");
    let default = instances
        .iter()
        .find(|instance| instance.key().target() == requirement)
        .expect("generic interface default instance");
    assert_eq!(default.key().static_self(), Some(default_mapper.ty()));
    assert!(matches!(
        default
            .key()
            .type_arguments()
            .iter()
            .map(|argument| typed.types().types().get(*argument))
            .collect::<Vec<_>>()
            .as_slice(),
        [
            Some(UnitTypeKind::Builtin(BuiltinType::String)),
            Some(UnitTypeKind::Builtin(BuiltinType::Long))
        ]
    ));
    let overridden = instances
        .iter()
        .find(|instance| instance.key().target() == override_target)
        .expect("generic concrete override instance");
    assert_eq!(overridden.key().static_self(), None);
    assert!(matches!(
        overridden
            .key()
            .type_arguments()
            .iter()
            .map(|argument| typed.types().types().get(*argument))
            .collect::<Vec<_>>()
            .as_slice(),
        [Some(UnitTypeKind::Builtin(BuiltinType::Long))]
    ));
}

#[test]
fn plans_bodyful_delegation_from_frontend_effective_targets() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int = 1 }\n\
         interface Derived: Readable { fun read(): Int = 2 }\n\
         class DefaultReader: Derived {}\n\
         class OverrideReader: Readable { override fun read(): Int = 7 }\n\
         class DefaultHost(val delegate: DefaultReader): Readable by delegate {}\n\
         class OverrideHost(val delegate: OverrideReader): Readable by delegate {}\n\
         fun entry(defaultHost: DefaultHost, overrideHost: OverrideHost): Int =\n\
             defaultHost.read() + overrideHost.read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let readable = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Readable"))
        .and_then(|signature| signature.nominal())
        .expect("Readable signature");
    let default_reader = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "DefaultReader"))
        .and_then(|signature| signature.nominal())
        .expect("DefaultReader signature");
    let derived = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Derived"))
        .and_then(|signature| signature.nominal())
        .expect("Derived signature");
    let override_reader = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "OverrideReader"))
        .and_then(|signature| signature.nominal())
        .expect("OverrideReader signature");
    let requirement = readable.members()[0].target();
    let inherited_target = derived.members()[0].target();
    let override_target = override_reader.members()[0].target();

    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("bodyful delegation must consume exact frontend effective targets");
    assert!(instances.iter().any(|instance| {
        instance.key().target() == inherited_target
            && instance.key().static_self() == Some(default_reader.ty())
    }));
    assert!(
        instances
            .iter()
            .all(|instance| instance.key().target() != requirement),
        "outer interface default must not bypass the delegate field route"
    );
    assert!(instances.iter().any(|instance| {
        instance.key().target() == override_target && instance.key().static_self().is_none()
    }));
}
