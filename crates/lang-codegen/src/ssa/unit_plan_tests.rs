use lang_frontend::{
    lexer::lex,
    name_resolution::{
        DeclarationId, NameEnvironment, SourceUnitInput, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{ValidatedCompilationUnitOwnership, check_compilation_unit_ownership},
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, TypeEnvironment, UnitCallableTarget, UnitTypeKind,
        ValidatedCompilationUnitTypes, check_compilation_unit_types, standard_environments,
    },
};

use super::{
    LoweringErrorKind,
    unit_plan::{UnitPlannedInstance, plan_unit_instances},
};

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique source");
    let lexed = lex(sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (source, parsed)
}

fn analyze(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    name_environment: &NameEnvironment,
    type_environment: &TypeEnvironment,
) -> (
    ValidatedCompilationUnitNames,
    ValidatedCompilationUnitTypes,
    ValidatedCompilationUnitOwnership,
) {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    let names = resolve_compilation_unit_names(sources, inputs, &index, name_environment)
        .expect("name resolution succeeds internally")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(sources, inputs, &names, type_environment)
        .expect("type checking succeeds internally")
        .validate()
        .expect("valid types");
    let owned = check_compilation_unit_ownership(sources, inputs, &names, type_environment, &typed)
        .expect("ownership checking succeeds internally")
        .validate()
        .expect("valid ownership");
    (names, typed, owned)
}

fn declaration(names: &ValidatedCompilationUnitNames, name: &str) -> DeclarationId {
    names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == name)
        .expect("declaration exists")
        .id()
}

fn plan<'a>(
    sources: &SourceMap,
    inputs: &'a [SourceUnitInput<'a>],
    names: &ValidatedCompilationUnitNames,
    type_environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
    entry: DeclarationId,
) -> Vec<UnitPlannedInstance> {
    plan_unit_instances(
        sources,
        inputs,
        names,
        type_environment,
        typed,
        owned,
        entry,
    )
    .expect("unit instance plan")
}

#[test]
fn plans_only_cross_file_reachable_functions_and_deduplicates_recursion() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun recurse(input: Int): Int = if (input == 0) 0 else recurse(input - 1)\n\
         fun dead(): Int = 99",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Int = p.recurse(2)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let instances = plan(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    );
    assert_eq!(instances.len(), 2);
    assert_eq!(
        instances
            .iter()
            .map(|instance| instance.key().target())
            .collect::<Vec<_>>(),
        [
            UnitCallableTarget::Declaration(declaration(&names, "recurse")),
            UnitCallableTarget::Declaration(declaration(&names, "entry")),
        ]
    );
    assert!(instances.iter().all(|instance| {
        instance.key().type_arguments().is_empty()
            && instance.substitutions().is_empty()
            && instance.span().source_id()
                == names.names().index().source_units()[instance.source_unit().index()].source_id()
            && matches!(instance.key().target(), UnitCallableTarget::Declaration(declaration)
                if instance.item().index()
                    == names.names().index().declarations()[declaration.index()].root().index())
    }));
}

#[test]
fn canonicalizes_generic_instances_and_is_input_order_independent() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun <T> identity(own input: T): T = input\n\
         fun <T> relay(own input: T): T = identity(input)\n\
         fun unused(): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Unit {\n\
             val first = p.relay(1)\n\
             val second = p.relay(2)\n\
             val text = p.relay(\"text\")\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed_inputs = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "entry");

    let forward = plan(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    );
    let reversed = plan(
        &sources,
        &reversed_inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    );
    assert_eq!(forward, reversed);
    assert_eq!(
        forward.len(),
        5,
        "entry + relay<Int/String> + identity<Int/String>"
    );
    let identity = declaration(&names, "identity");
    let arguments = forward
        .iter()
        .filter(|instance| instance.key().target() == UnitCallableTarget::Declaration(identity))
        .map(|instance| instance.key().type_arguments()[0])
        .collect::<Vec<_>>();
    assert_eq!(arguments.len(), 2);
    assert_ne!(arguments[0], arguments[1]);
    assert!(arguments.iter().any(|argument| matches!(
        typed.types().types().get(*argument),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    )));
    assert!(arguments.iter().any(|argument| matches!(
        typed.types().types().get(*argument),
        Some(UnitTypeKind::Builtin(BuiltinType::String))
    )));
    assert!(
        forward
            .iter()
            .filter(|instance| !instance.key().type_arguments().is_empty())
            .all(|instance| instance.substitutions().len() == 1)
    );
}

#[test]
fn plans_reachable_member_instances_with_owner_and_callable_arguments() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         value class Box<T>(val item: T) {\n\
             fun <R> keep(own ignored: R): Int = 1\n\
         }\n\
         fun entry(): Int = Box(7).keep(\"ignored\")",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let instances = plan(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    );
    assert_eq!(instances.len(), 2, "entry and one concrete member instance");
    let member = instances
        .iter()
        .find(|instance| matches!(instance.key().target(), UnitCallableTarget::Symbol(_)))
        .expect("reachable member instance");
    assert_eq!(member.owner(), Some(declaration(&names, "Box")));
    assert_eq!(member.key().type_arguments().len(), 2);
    assert_eq!(member.substitutions().len(), 2);
    assert!(matches!(
        typed.types().types().get(member.key().type_arguments()[0]),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(matches!(
        typed.types().types().get(member.key().type_arguments()[1]),
        Some(UnitTypeKind::Builtin(BuiltinType::String))
    ));
}

#[test]
fn plans_one_interface_default_instance_per_concrete_static_self() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(): Int = 1 }\n\
         class First: Readable {}\n\
         class Second: Readable {}\n\
         fun entry(): Int = First().read() + Second().read()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let instances = plan(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    );
    let defaults = instances
        .iter()
        .filter(|instance| instance.key().static_self().is_some())
        .collect::<Vec<_>>();
    assert_eq!(defaults.len(), 2, "one default body per concrete Self");
    assert!(
        defaults
            .iter()
            .all(|instance| instance.key().is_specialized())
    );
    assert!(
        instances
            .iter()
            .find(|instance| instance.key().target()
                == UnitCallableTarget::Declaration(declaration(&names, "entry")))
            .is_some_and(|instance| !instance.key().is_specialized()),
        "concrete StaticSelf uses the same bounded specialization path as generic instances"
    );
    assert_eq!(defaults[0].key().target(), defaults[1].key().target());
    assert_eq!(
        defaults[0].key().type_arguments(),
        defaults[1].key().type_arguments()
    );
    let concrete_owners = defaults
        .iter()
        .map(|instance| {
            let ty = instance.key().static_self().expect("concrete StaticSelf");
            match typed.types().types().get(ty) {
                Some(UnitTypeKind::Nominal { declaration, .. }) => *declaration,
                other => panic!("concrete StaticSelf must be nominal: {other:?}"),
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        concrete_owners,
        [declaration(&names, "First"), declaration(&names, "Second")]
    );
}

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
fn rejects_delegation_chain_that_changes_requirement_identity() {
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

    let error = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect_err("identity-changing delegation chain needs an explicit frontend next-hop fact");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
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
fn rejects_generic_delegate_field_in_the_route_resolver() {
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

    let error = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect_err("generic delegate layout remains outside the first native route slice");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}

#[test]
fn rejects_generic_outer_receiver_in_the_route_resolver() {
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

    let error = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect_err("generic outer layout remains outside this native route slice");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
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

#[test]
fn remaps_requirement_arguments_to_concrete_owner_and_callable_slots() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface GenericBase<A> {\n\
             fun <R> id(own input: R): R\n\
             fun <R> throughRequirement(own input: R): R = this.id(input)\n\
         }\n\
         class Host<T>: GenericBase<String> {\n\
             override fun <R> id(own input: R): R = input\n\
         }\n\
         fun entry(host: Host<Int>): Long = host.throughRequirement(2L)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let host = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Host"))
        .and_then(|signature| signature.nominal())
        .expect("Host signature");
    let implementation = host
        .members()
        .iter()
        .find(|member| member.name() == "id")
        .expect("generic concrete override")
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
    let implementation = instances
        .iter()
        .find(|instance| instance.key().target() == implementation)
        .expect("concrete generic override instance");
    assert_eq!(implementation.key().type_arguments().len(), 2);
    assert!(matches!(
        typed
            .types()
            .types()
            .get(implementation.key().type_arguments()[0]),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(matches!(
        typed
            .types()
            .types()
            .get(implementation.key().type_arguments()[1]),
        Some(UnitTypeKind::Builtin(BuiltinType::Long))
    ));
}

#[test]
fn remaps_inherited_default_owner_recipe_and_callable_slots() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> {\n\
             fun <R> read(own input: R): R\n\
             fun <R> throughRequirement(own input: R): R = this.read(input)\n\
         }\n\
         interface Derived<B>: Base<String> {\n\
             fun <R> read(own input: R): R = input\n\
         }\n\
         class Host<X, Y>: Derived<Y> {}\n\
         fun entry(host: Host<Int, Long>): Int = host.throughRequirement(2)",
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
    let implementation = derived
        .members()
        .iter()
        .find(|member| member.name() == "read")
        .expect("inherited default implementation")
        .target();
    let requirement = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Base"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| {
            nominal
                .members()
                .iter()
                .find(|member| member.name() == "read")
        })
        .expect("ancestor abstract requirement")
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
    let implementation = instances
        .iter()
        .find(|instance| instance.key().target() == implementation)
        .expect("inherited generic default instance");
    assert_eq!(implementation.key().type_arguments().len(), 2);
    assert!(matches!(
        typed
            .types()
            .types()
            .get(implementation.key().type_arguments()[0]),
        Some(UnitTypeKind::Builtin(BuiltinType::Long))
    ));
    assert!(matches!(
        typed
            .types()
            .types()
            .get(implementation.key().type_arguments()[1]),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(implementation.key().static_self().is_some());
    assert!(
        instances
            .iter()
            .all(|instance| instance.key().target() != requirement)
    );
}

#[test]
fn rejects_nested_inherited_owner_recipe_before_generic_nominal_layout() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> { fun read(): Int = 7 }\n\
         class Host<Y>: Derived<List<Y>> {}\n\
         fun entry(host: Host<Int>): Int = host.throughRequirement()",
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
    .expect_err("nested generic owner recipes remain behind generic nominal layout");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}

#[test]
fn rejects_non_callable_entries_and_foreign_ownership_products() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\nclass NotCallable {}\nfun entry(): Unit {}",
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
        declaration(&names, "NotCallable"),
    )
    .expect_err("classifier cannot be a native entry");
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
    assert!(error.span.is_some());

    let (_, foreign_typed, foreign_owned) =
        analyze(&sources, &inputs, &name_environment, &type_environment);
    assert!(!foreign_owned.ownership().is_compatible_with(&typed));
    assert!(foreign_owned.ownership().is_compatible_with(&foreign_typed));
    let error = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &foreign_owned,
        declaration(&names, "entry"),
    )
    .expect_err("foreign ownership must fail before planning");
    assert_eq!(error.kind, LoweringErrorKind::MismatchedAnalysis);
    assert!(error.span.is_none());
}
