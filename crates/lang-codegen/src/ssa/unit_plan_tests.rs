use std::collections::{BTreeMap, BTreeSet};

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
        BuiltinType, IntrinsicTypeConstructor, TypeEnvironment, UnitCallTarget, UnitCallableTarget,
        UnitTypeKind, ValidatedCompilationUnitTypes, check_compilation_unit_types,
        standard_environments,
    },
};

use super::{
    LoweringErrorKind,
    unit_plan::{
        UnitInstancePlan, UnitRuntimeTypeDemand, plan_unit_instances,
        resolve_delegated_dispatch_owner_argument, resolve_inherited_dispatch_owner_argument,
        resolve_nominal_runtime_field_types, resolve_unit_call_instance,
    },
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

fn only_member_call_route(
    parsed: &ParsedFile,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
) -> super::unit_plan::ResolvedUnitCallInstance {
    let call = typed
        .types()
        .calls()
        .iter()
        .find(|call| call.receiver().is_some())
        .expect("one member call");
    let target = match call.target() {
        UnitCallTarget::Declaration(declaration) => UnitCallableTarget::Declaration(declaration),
        UnitCallTarget::Symbol(symbol) => UnitCallableTarget::Symbol(symbol),
        _ => panic!("source member call has a static target"),
    };
    let span = parsed
        .ast()
        .expressions()
        .get(call.expression().expression())
        .expect("call expression")
        .span();
    resolve_unit_call_instance(
        typed,
        owned,
        target,
        call.instance().type_arguments().to_vec(),
        call.receiver().map(|receiver| receiver.ty()),
        span,
    )
    .expect("member route resolves")
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
) -> UnitInstancePlan {
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
fn nested_runtime_layout_requires_the_exact_frontend_owner_descriptor() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/layout.ko",
        "package p\n\
         class Dependent<T>(val items: List<T>)\n\
         fun entry(input: Dependent<Int>): Unit {}",
    );
    let inputs = [SourceUnitInput::new("root", "p/layout.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, _) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let dependent = declaration(&names, "Dependent");
    let nominal = typed
        .types()
        .signatures()
        .declaration(dependent)
        .and_then(|signature| signature.nominal())
        .expect("Dependent signature exists");
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int is seeded");
    let string = typed
        .types()
        .types()
        .builtin(BuiltinType::String)
        .expect("String is seeded");
    let owner = typed
        .types()
        .types()
        .find(&UnitTypeKind::Nominal {
            declaration: dependent,
            arguments: vec![int],
        })
        .expect("Dependent<Int> is canonical");

    let fields = resolve_nominal_runtime_field_types(&typed, owner, nominal, &[int])
        .expect("exact owner descriptor resolves nested List<Int>");
    assert!(matches!(
        fields.as_slice(),
        [field]
            if matches!(
                typed.types().types().get(*field),
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::List,
                    arguments,
                }) if arguments == &[int]
            )
    ));
    let error = resolve_nominal_runtime_field_types(&typed, owner, nominal, &[string])
        .expect_err("owner arguments cannot be replaced by another global canonical type");
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
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
fn merges_dependent_inherited_runtime_demand_independent_of_input_order() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> {\n\
             fun read(): Int = 7\n\
             fun echo(own input: B): B = input\n\
         }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<Y>: Derived<Wrapper<Y>> {}\n\
         fun keyOnly(): Int = Host<Int>().throughRequirement()\n\
         fun runtime(): Int = Host<Int>().echo(Wrapper<Int>(7)).item",
    );
    let (entry_source, entry_file) = parsed(
        &mut sources,
        "p/entry.ko",
        "package p\nfun entry(): Int = keyOnly() + runtime()",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "p/entry.ko", entry_source, &entry_file),
    ];
    let reversed_inputs = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");
    let wrapper_int = typed
        .types()
        .types()
        .find(&UnitTypeKind::Nominal {
            declaration: declaration(&names, "Wrapper"),
            arguments: vec![int],
        })
        .expect("frontend exact Wrapper<Int> identity");
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
        forward.runtime_type_demand(wrapper_int),
        Some(UnitRuntimeTypeDemand::RuntimeLayoutRequired),
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
fn delegated_dispatch_owner_recipe_keeps_unsupported_nested_kinds_closed() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/dispatch-recipes.ko",
        "package p\n\
         value class ValueWrapper<T>(val item: T)\n\
         class Pair<A, B>(val first: A, val second: B)\n\
         class ArrayOwner<T>(val item: Array<T>)\n\
         class NullableOwner<T>(val item: T?)\n\
         class FunctionOwner<T>(val item: (T) -> T)\n\
         class ValueOwner<T>(val item: ValueWrapper<T>)\n\
         class MultiOwner<T>(val item: Pair<T, T>)\n\
         fun entry(\n\
             array: ArrayOwner<Int>,\n\
             nullable: NullableOwner<Int>,\n\
             callable: FunctionOwner<Int>,\n\
             wrapped: ValueOwner<Int>,\n\
             multi: MultiOwner<Int>\n\
         ): Unit {}",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/dispatch-recipes.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, _) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");

    for owner_name in [
        "ArrayOwner",
        "NullableOwner",
        "FunctionOwner",
        "ValueOwner",
        "MultiOwner",
    ] {
        let owner = typed
            .types()
            .signatures()
            .declaration(declaration(&names, owner_name))
            .and_then(|signature| signature.nominal())
            .unwrap_or_else(|| panic!("{owner_name} signature"));
        let [parameter] = owner.type_parameters() else {
            panic!("{owner_name} has one type parameter");
        };
        let [field] = owner.fields() else {
            panic!("{owner_name} has one field");
        };
        let substitutions = BTreeMap::from([(*parameter, int)]);
        let error = resolve_delegated_dispatch_owner_argument(
            &typed,
            field.ty(),
            &substitutions,
            field.span(),
            &mut BTreeSet::new(),
        )
        .unwrap_err();
        assert_eq!(
            error.kind,
            LoweringErrorKind::UnsupportedNode,
            "{owner_name}"
        );
        assert_eq!(error.span, Some(field.span()), "{owner_name}");
    }
}

#[test]
fn inherited_dispatch_owner_recipe_keeps_dependent_kinds_and_missing_canonical_closed() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/inherited-recipes.ko",
        "package p\n\
         value class ValueWrapper<T>(val item: T)\n\
         class Wrapper<T>(val item: T)\n\
         class Pair<A, B>(val first: A, val second: B)\n\
         class ArrayOwner<T>(val item: Array<T>)\n\
         class NullableOwner<T>(val item: T?)\n\
         class FunctionOwner<T>(val item: (T) -> T)\n\
         class ValueOwner<T>(val item: ValueWrapper<T>)\n\
         class MultiOwner<T>(val item: Pair<T, T>)\n\
         class NominalOwner<T>(val item: Wrapper<T>)\n\
         class GrowingOwner<T>(val item: GrowingOwner<List<T>>)\n\
         class SelfNode<T>(val next: SelfNode<T>)\n\
         class SelfOwner<T>(val item: SelfNode<T>)\n\
         class LeftNode<T>(val right: RightNode<T>)\n\
         class RightNode<T>(val left: LeftNode<T>)\n\
         class MutualOwner<T>(val item: LeftNode<T>)\n\
         class ClosedSelf<T>(val item: T, val next: ClosedSelf<Int>)\n\
         class ClosedSelfOwner<T>(val item: ClosedSelf<T>)\n\
         class ClosedLeft<T>(val item: T, val right: ClosedRight<Int>)\n\
         class ClosedRight<T>(val item: T, val left: ClosedLeft<Int>)\n\
         class ClosedMutualOwner<T>(val item: ClosedLeft<T>)\n\
         class ParamLeft<T>(val right: ParamRight<Int>)\n\
         class ParamRight<U>(val left: ParamLeft<U>)\n\
         class ParamCycleOwner<T>(val item: ParamLeft<T>)\n\
         enum class ClosedChoice { Item(item: ClosedEnumNode<Int>), Empty }\n\
         class ClosedEnumNode<T>(val item: T, val choice: ClosedChoice)\n\
         class ClosedEnumOwner<T>(val item: ClosedEnumNode<T>)\n\
         class ClosedLeaf<U>(val items: List<U>)\n\
         class ClosedDag<T>(val item: T, val leaf: ClosedLeaf<Int>)\n\
         class ClosedDagOwner<T>(val item: ClosedDag<T>)\n\
         fun closedDagSeed(input: ClosedDag<Int>, items: List<Int>): Unit {}\n\
         class ListOwner<T>(val item: List<T>)\n\
         class Marker<T>(val marker: Int)\n\
         class MarkerOwner<T>(val item: Marker<T>)\n\
         fun entry(): Long = 1L",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/inherited-recipes.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, _) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");

    for owner_name in [
        "ArrayOwner",
        "NullableOwner",
        "FunctionOwner",
        "ValueOwner",
        "MultiOwner",
        "GrowingOwner",
        "SelfOwner",
        "MutualOwner",
        "ClosedSelfOwner",
        "ClosedMutualOwner",
        "ParamCycleOwner",
        "ClosedEnumOwner",
    ] {
        let owner = typed
            .types()
            .signatures()
            .declaration(declaration(&names, owner_name))
            .and_then(|signature| signature.nominal())
            .unwrap_or_else(|| panic!("{owner_name} signature"));
        let [parameter] = owner.type_parameters() else {
            panic!("{owner_name} has one type parameter");
        };
        let [field] = owner.fields() else {
            panic!("{owner_name} has one field");
        };
        let error = resolve_inherited_dispatch_owner_argument(
            &typed,
            field.ty(),
            &BTreeMap::from([(*parameter, int)]),
            field.span(),
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
        )
        .unwrap_err();
        assert_eq!(
            error.kind,
            LoweringErrorKind::UnsupportedNode,
            "{owner_name}"
        );
        if matches!(
            owner_name,
            "SelfOwner"
                | "MutualOwner"
                | "ClosedSelfOwner"
                | "ClosedMutualOwner"
                | "ParamCycleOwner"
                | "ClosedEnumOwner"
        ) {
            assert!(error.span.is_some(), "{owner_name}");
        } else {
            assert_eq!(error.span, Some(field.span()), "{owner_name}");
        }
    }

    let list_owner = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "ListOwner"))
        .and_then(|signature| signature.nominal())
        .expect("ListOwner signature");
    let [parameter] = list_owner.type_parameters() else {
        panic!("ListOwner has one type parameter");
    };
    let [field] = list_owner.fields() else {
        panic!("ListOwner has one field");
    };
    let long = typed
        .types()
        .types()
        .builtin(BuiltinType::Long)
        .expect("Long type");
    assert!(
        typed
            .types()
            .types()
            .find(&UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments: vec![long],
            })
            .is_none(),
        "fixture must not pre-intern List<Long>"
    );
    let error = resolve_inherited_dispatch_owner_argument(
        &typed,
        field.ty(),
        &BTreeMap::from([(*parameter, long)]),
        field.span(),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    )
    .unwrap_err();
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
    assert_eq!(error.span, Some(field.span()));

    let dag_owner = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "ClosedDagOwner"))
        .and_then(|signature| signature.nominal())
        .expect("ClosedDagOwner signature");
    let [parameter] = dag_owner.type_parameters() else {
        panic!("ClosedDagOwner has one type parameter");
    };
    let [field] = dag_owner.fields() else {
        panic!("ClosedDagOwner has one field");
    };
    resolve_inherited_dispatch_owner_argument(
        &typed,
        field.ty(),
        &BTreeMap::from([(*parameter, int)]),
        field.span(),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    )
    .expect("closed generic List substitution is a finite recipe");

    let marker = declaration(&names, "Marker");
    let marker_owner = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "MarkerOwner"))
        .and_then(|signature| signature.nominal())
        .expect("MarkerOwner signature");
    let [parameter] = marker_owner.type_parameters() else {
        panic!("MarkerOwner has one type parameter");
    };
    let [field] = marker_owner.fields() else {
        panic!("MarkerOwner has one field");
    };
    assert!(
        typed
            .types()
            .types()
            .find(&UnitTypeKind::Nominal {
                declaration: marker,
                arguments: vec![long],
            })
            .is_none(),
        "fixture must not pre-intern Marker<Long>"
    );
    let error = resolve_inherited_dispatch_owner_argument(
        &typed,
        field.ty(),
        &BTreeMap::from([(*parameter, long)]),
        field.span(),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    )
    .unwrap_err();
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
    assert_eq!(error.span, Some(field.span()));
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
fn remaps_list_inherited_owner_recipe_to_the_effective_default() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/list-inherited.ko",
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> { fun read(): Int = 7 }\n\
         class Host<Y>: Derived<List<Y>> {}\n\
         fun entry(host: Host<Int>): Int = host.throughRequirement()",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/list-inherited.ko",
        source,
        &parsed,
    )];
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

    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("List owner recipe must select the exact inherited default");
    let implementation = instances
        .iter()
        .find(|instance| instance.key().target() == implementation)
        .expect("concrete inherited default instance");
    let [owner_argument] = implementation.key().type_arguments() else {
        panic!("Derived default has one concrete owner argument");
    };
    let Some(UnitTypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::List,
        arguments,
    }) = typed.types().types().get(*owner_argument)
    else {
        panic!("inherited owner argument must be List<Int>");
    };
    assert!(matches!(
        arguments.as_slice(),
        [argument]
            if matches!(
                typed.types().types().get(*argument),
                Some(UnitTypeKind::Builtin(BuiltinType::Int))
            )
    ));
    let static_self = implementation
        .key()
        .static_self()
        .expect("inherited default retains concrete StaticSelf");
    assert!(matches!(
        typed.types().types().get(static_self),
        Some(UnitTypeKind::Nominal {
            declaration: owner,
            arguments,
        }) if *owner == declaration(&names, "Host")
            && matches!(
                arguments.as_slice(),
                [argument]
                    if matches!(
                        typed.types().types().get(*argument),
                        Some(UnitTypeKind::Builtin(BuiltinType::Int))
                    )
            )
    ));
    assert!(
        instances
            .iter()
            .all(|instance| instance.key().target() != requirement)
    );
}

#[test]
fn plans_dependent_inherited_owner_recipe_as_instance_key_only() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/class-inherited.ko",
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> { fun read(): Int = 7 }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<Y>: Derived<Wrapper<Y>> {}\n\
         fun entry(host: Host<Int>): Int = host.throughRequirement()",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/class-inherited.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");
    let wrapper = declaration(&names, "Wrapper");
    let wrapper_int = typed
        .types()
        .types()
        .find(&UnitTypeKind::Nominal {
            declaration: wrapper,
            arguments: vec![int],
        })
        .expect("frontend must intern canonical Wrapper<Int>");
    let implementation = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Derived"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| {
            nominal
                .members()
                .iter()
                .find(|member| member.name() == "read")
        })
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

    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("dependent class owner recipe must select the inherited default");
    assert_eq!(
        instances.runtime_type_demand(wrapper_int),
        Some(UnitRuntimeTypeDemand::InstanceKeyOnly),
    );
    let implementation = instances
        .iter()
        .find(|instance| instance.key().target() == implementation)
        .expect("concrete inherited default instance");
    assert_eq!(implementation.key().type_arguments(), &[wrapper_int]);
    assert!(matches!(
        typed.types().types().get(wrapper_int),
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) if *declaration == wrapper && arguments == &[int]
    ));
    let static_self = implementation
        .key()
        .static_self()
        .expect("inherited default retains concrete StaticSelf");
    assert!(matches!(
        typed.types().types().get(static_self),
        Some(UnitTypeKind::Nominal {
            declaration: owner,
            arguments,
        }) if *owner == declaration(&names, "Host") && arguments == &[int]
    ));
    assert!(
        instances
            .iter()
            .all(|instance| instance.key().target() != requirement)
    );
}

#[test]
fn upgrades_dependent_inherited_owner_recipe_to_runtime_layout_required() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/dependent-runtime.ko",
        "package p\n\
         interface Base<A> { fun read(): Int }\n\
         interface Derived<B>: Base<String> {\n\
             fun read(): Int = 7\n\
             fun echo(own input: B): B = input\n\
         }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<Y>: Derived<Wrapper<Y>> {}\n\
         fun entry(host: Host<Int>, own input: Wrapper<Int>): Wrapper<Int> = host.echo(input)",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/dependent-runtime.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");
    let wrapper_int = typed
        .types()
        .types()
        .find(&UnitTypeKind::Nominal {
            declaration: declaration(&names, "Wrapper"),
            arguments: vec![int],
        })
        .expect("frontend exact Wrapper<Int> identity");

    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("runtime dependent owner recipe must use the exact frontend descriptor");
    assert_eq!(
        instances.runtime_type_demand(wrapper_int),
        Some(UnitRuntimeTypeDemand::RuntimeLayoutRequired),
    );
}

#[test]
fn upgrades_dependent_inherited_owner_recipe_used_only_by_lambda_abi() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/dependent-lambda.ko",
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> { fun read(): Int = 7 }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<Y>: Derived<Wrapper<Y>> {}\n\
         fun entry(host: Host<Int>): Int {\n\
             val action: move (own Wrapper<Int>) -> Unit = move { item -> val read = item.item }\n\
             return host.throughRequirement()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/dependent-lambda.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");
    let wrapper_int = typed
        .types()
        .types()
        .find(&UnitTypeKind::Nominal {
            declaration: declaration(&names, "Wrapper"),
            arguments: vec![int],
        })
        .expect("frontend exact Wrapper<Int> identity");

    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("lambda ABI must upgrade the dependent owner demand before lowering");
    assert_eq!(
        instances.runtime_type_demand(wrapper_int),
        Some(UnitRuntimeTypeDemand::RuntimeLayoutRequired),
    );
}

#[test]
fn upgrades_dependent_inherited_owner_recipe_used_only_by_enum_payload() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/dependent-enum.ko",
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> { fun read(): Int = 7 }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<Y>: Derived<Wrapper<Y>> {}\n\
         enum class Payload { Item(item: Wrapper<Int>), Empty }\n\
         fun entry(host: Host<Int>, own payload: Payload): Int = host.throughRequirement()",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/dependent-enum.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");
    let wrapper_int = typed
        .types()
        .types()
        .find(&UnitTypeKind::Nominal {
            declaration: declaration(&names, "Wrapper"),
            arguments: vec![int],
        })
        .expect("frontend exact Wrapper<Int> identity");

    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("enum payload storage must upgrade the dependent owner demand");
    assert_eq!(
        instances.runtime_type_demand(wrapper_int),
        Some(UnitRuntimeTypeDemand::RuntimeLayoutRequired),
    );
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
