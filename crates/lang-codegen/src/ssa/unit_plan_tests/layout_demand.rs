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

pub(in crate::ssa) mod recipe_cycles {
    use super::super::*;
    use crate::test_support::diagnostics::assert_eq_diagnostic;

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
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
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
                let error = result.expect_err(
                    "recipe failure must precede planning and the generic instance limit",
                );
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
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
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
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
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
            assert_eq_diagnostic!(error.span, Some(witness), "{mode}: {error:?}");
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
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
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
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
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
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
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
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);

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
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
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
}
