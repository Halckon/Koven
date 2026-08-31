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
