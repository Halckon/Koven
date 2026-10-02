use super::*;

#[test]
fn delegation_publishes_only_borrow_receiver_forwarders_and_reports_l0152() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "delegation-receivers.ko",
        "package p\n\
         interface Readable { fun read(): Int = 0 }\n\
         class Reader: Readable { override fun read(): Int = 1 }\n\
         class ReadHost(val reader: Reader): Readable by reader\n\
         interface Mutable { inout fun z(): Unit; own fun a(): Unit }\n\
         class Mutator: Mutable {\n\
             override inout fun z(): Unit {}\n\
             override own fun a(): Unit {}\n\
         }\n\
         class MutateHost(val mutator: Mutator): Mutable by mutator",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/delegation-receivers.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("delegation receiver diagnostics remain recoverable");

    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0152"]
    );
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("L0152 primary"),
        "by"
    );
    assert_eq!(
        typed.diagnostics()[0]
            .details()
            .iter()
            .find_map(|detail| match detail {
                DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
                DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
            }),
        Some("z")
    );
    let plans = typed.signatures().delegations();
    assert_eq!(plans.len(), 2);
    let readable = plans
        .iter()
        .find(|plan| plan.owner() == declaration(&names, "ReadHost"))
        .expect("Readable delegation");
    let mutable = plans
        .iter()
        .find(|plan| plan.owner() == declaration(&names, "MutateHost"))
        .expect("Mutable delegation");
    assert_eq!(readable.forwarders().len(), 1);
    let readable_signature = typed
        .signatures()
        .declaration(declaration(&names, "Readable"))
        .and_then(|signature| signature.nominal())
        .expect("Readable signature");
    let reader_signature = typed
        .signatures()
        .declaration(declaration(&names, "Reader"))
        .and_then(|signature| signature.nominal())
        .expect("Reader signature");
    let forwarder = &readable.forwarders()[0];
    assert_eq!(
        forwarder.requirement(),
        readable_signature.members()[0].target()
    );
    assert_eq!(
        forwarder
            .implementation()
            .map(|implementation| implementation.target()),
        Some(reader_signature.members()[0].target(),),
        "delegate local override must replace the interface default"
    );
    assert_eq!(
        forwarder
            .implementation()
            .map(|implementation| implementation.receiver_type()),
        Some(reader_signature.ty())
    );
    assert_eq!(forwarder.receiver_mode(), ParameterMode::Borrow);
    assert!(mutable.forwarders().is_empty());
}

#[test]
fn delegation_forwarders_publish_inherited_effective_default_targets() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "delegation-default-targets.ko",
        "package p\n\
         interface Base { fun read(): Int = 1 }\n\
         interface Derived: Base { fun read(): Int = 2 }\n\
         class DefaultReader: Base {}\n\
         class DerivedReader: Derived {}\n\
         class DefaultHost(val reader: DefaultReader): Base by reader\n\
         class DerivedHost(val reader: DerivedReader): Base by reader",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/delegation-default-targets.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("delegation default target facts remain recoverable")
        .validate()
        .expect("delegation default target fixture must be valid");
    let nominal = |name| {
        typed
            .types()
            .signatures()
            .declaration(declaration(&names, name))
            .and_then(|signature| signature.nominal())
            .unwrap_or_else(|| panic!("{name} signature"))
    };
    let base = nominal("Base");
    let derived = nominal("Derived");
    let default_reader = nominal("DefaultReader");
    let derived_reader = nominal("DerivedReader");
    let plan = |name| {
        typed
            .types()
            .signatures()
            .delegations()
            .iter()
            .find(|plan| plan.owner() == declaration(&names, name))
            .unwrap_or_else(|| panic!("{name} delegation"))
    };

    let inherited = &plan("DefaultHost").forwarders()[0];
    assert_eq!(inherited.requirement(), base.members()[0].target());
    let inherited_implementation = inherited.implementation().expect("inherited default");
    assert_eq!(
        inherited_implementation.target(),
        base.members()[0].target()
    );
    assert_eq!(inherited_implementation.receiver_type(), base.ty());
    assert_ne!(
        inherited_implementation.receiver_type(),
        default_reader.ty()
    );

    let replacement = &plan("DerivedHost").forwarders()[0];
    assert_eq!(replacement.requirement(), base.members()[0].target());
    let replacement_implementation = replacement.implementation().expect("replacement default");
    assert_eq!(
        replacement_implementation.target(),
        derived.members()[0].target()
    );
    assert_eq!(replacement_implementation.receiver_type(), derived.ty());
    assert_ne!(
        replacement_implementation.receiver_type(),
        derived_reader.ty()
    );
}

#[test]
fn delegation_forwarders_keep_generic_owner_templates_and_publish_exact_next_hops() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "delegation-generic-and-recursive.ko",
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class Reader<T>: Readable { override fun read(): Int = 7 }\n\
         class GenericHost<T>(val reader: Reader<T>): Readable by reader\n\
         class DeferredHost<T: Readable>(val target: T): Readable by target\n\
         class Middle(val reader: Reader<Int>): Readable by reader\n\
         class RecursiveHost(val middle: Middle): Readable by middle",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/delegation-generic-and-recursive.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("delegation implementation facts remain recoverable")
        .validate()
        .expect("generic and recursive delegation fixture must be valid");
    let signatures = typed.types().signatures();
    let nominal = |name| {
        signatures
            .declaration(declaration(&names, name))
            .and_then(|signature| signature.nominal())
            .unwrap_or_else(|| panic!("{name} signature"))
    };
    let reader = nominal("Reader");
    let generic_host = nominal("GenericHost");
    let plan = |name| {
        signatures
            .delegations()
            .iter()
            .find(|plan| plan.owner() == declaration(&names, name))
            .unwrap_or_else(|| panic!("{name} delegation"))
    };

    let generic = plan("GenericHost").forwarders()[0]
        .implementation()
        .expect("direct generic delegate implementation");
    assert_eq!(generic.target(), reader.members()[0].target());
    let Some(UnitTypeKind::Nominal {
        declaration: receiver_declaration,
        arguments,
    }) = typed
        .types()
        .signatures()
        .types()
        .get(generic.receiver_type())
    else {
        panic!("generic implementation receiver must remain nominal");
    };
    assert_eq!(*receiver_declaration, declaration(&names, "Reader"));
    assert_eq!(arguments.len(), 1);
    assert!(matches!(
        typed.types().signatures().types().get(arguments[0]),
        Some(UnitTypeKind::TypeParameter(parameter))
            if *parameter == generic_host.type_parameters()[0]
    ));

    let deferred = &plan("DeferredHost").forwarders()[0];
    assert_eq!(deferred.implementation(), None);
    assert_eq!(
        deferred.next_hop(),
        None,
        "type-parameter delegate stays unresolved until monomorphization"
    );

    assert!(
        plan("Middle").forwarders()[0].implementation().is_some(),
        "the first direct forwarding hop must remain concrete"
    );
    let recursive = &plan("RecursiveHost").forwarders()[0];
    assert_eq!(recursive.implementation(), None);
    let next = recursive
        .next_hop()
        .expect("a legal recursive delegation route must publish its exact next hop");
    assert_eq!(next.requirement(), recursive.requirement());
    assert_eq!(next.receiver_type(), recursive.receiver_type());
}

#[test]
fn delegation_forwarders_publish_identity_changing_next_hops() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "delegation-replacement-next-hop.ko",
        "package p\n\
         interface Base { fun read(): Int = 1 }\n\
         interface Derived: Base { fun read(): Int = 2 }\n\
         class Reader: Derived { override fun read(): Int = 7 }\n\
         class Middle(val reader: Reader): Derived by reader\n\
         class Host(val middle: Middle): Base by middle",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/delegation-replacement-next-hop.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("identity-changing delegation facts remain recoverable")
        .validate()
        .expect("identity-changing delegation fixture must be valid");
    let signatures = typed.types().signatures();
    let nominal = |name| {
        signatures
            .declaration(declaration(&names, name))
            .and_then(|signature| signature.nominal())
            .unwrap_or_else(|| panic!("{name} signature"))
    };
    let base = nominal("Base");
    let derived = nominal("Derived");
    let plan = |name| {
        signatures
            .delegations()
            .iter()
            .find(|plan| plan.owner() == declaration(&names, name))
            .unwrap_or_else(|| panic!("{name} delegation"))
    };

    let outer = &plan("Host").forwarders()[0];
    assert_eq!(outer.requirement(), base.members()[0].target());
    assert_eq!(outer.implementation(), None);
    let next = outer
        .next_hop()
        .expect("replacement chain must publish the Derived requirement identity");
    assert_eq!(next.requirement(), derived.members()[0].target());
    assert_eq!(next.receiver_type(), derived.ty());
}

#[test]
fn delegation_next_hop_receiver_is_instantiated_through_the_delegate_field() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "delegation-generic-next-hop.ko",
        "package p\n\
         interface Echo<T> { fun echo(own input: T): T }\n\
         class Leaf<T>: Echo<T> { override fun echo(own input: T): T = input }\n\
         class Middle<U>(val leaf: Leaf<U>): Echo<U> by leaf\n\
         class Host<T>(val middle: Middle<T>): Echo<T> by middle",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/delegation-generic-next-hop.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("generic delegation next-hop facts remain recoverable")
        .validate()
        .expect("generic delegation next-hop fixture must be valid");
    let signatures = typed.types().signatures();
    let host = signatures
        .declaration(declaration(&names, "Host"))
        .and_then(|signature| signature.nominal())
        .expect("Host signature");
    let plan = signatures
        .delegations()
        .iter()
        .find(|plan| plan.owner() == host.declaration())
        .expect("Host delegation");
    let next = plan.forwarders()[0]
        .next_hop()
        .expect("generic chain must publish an exact next hop");
    let Some(UnitTypeKind::Nominal { arguments, .. }) =
        typed.types().signatures().types().get(next.receiver_type())
    else {
        panic!("next-hop receiver must remain a nominal interface instance");
    };
    assert!(matches!(
        arguments.as_slice(),
        [argument]
            if matches!(
                typed.types().signatures().types().get(*argument),
                Some(UnitTypeKind::TypeParameter(parameter))
                    if *parameter == host.type_parameters()[0]
            )
    ));
}
