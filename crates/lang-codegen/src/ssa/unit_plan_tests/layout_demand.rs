use super::*;

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
