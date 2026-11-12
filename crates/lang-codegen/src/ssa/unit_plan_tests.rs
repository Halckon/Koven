use super::unit_lower;

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
        plan_unit_instances_with_limit, resolve_delegated_dispatch_owner_argument,
        resolve_inherited_dispatch_owner_argument, resolve_nominal_runtime_field_types,
        resolve_unit_call_instance,
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
        typed.types(),
        owned.ownership(),
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

mod body_type_budget {
    //! SPEC-0276: planner budgets retain their own gate and recipe arbitration.
    use super::*;

    #[test]
    fn unit_generic_body_budget_zero_one_two_keep_pure_limit_template_spans() {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "p/provider.ko",
            "package p\n\
             fun <T> inner(own item: T): Int {\n\
                 val values = listOf(item)\nreturn values.size\n}\n\
             fun <T> middle(own item: T): Int = inner(item)\n\
             fun <T> relay(own item: T): Int = middle(item)",
        );
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            "package q\nfun entry(): Int = p.relay(1)",
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let reversed = [inputs[1], inputs[0]];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        assert_concrete_body_container(&typed, IntrinsicTypeConstructor::List);
        let arena_len = typed.types().types().len();
        let entry = declaration(&names, "entry");
        let targets = ["relay", "middle", "inner"].map(|name| declaration(&names, name));
        for (limit, target) in targets.into_iter().enumerate() {
            let span = provider
                .ast()
                .items()
                .get(names.names().index().declarations()[target.index()].root())
                .unwrap()
                .span();
            assert_eq!(span.source_id(), provider_source);
            for unit_inputs in [&inputs[..], &reversed[..]] {
                let error = plan_unit_instances_with_limit(
                    &sources,
                    unit_inputs,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    entry,
                    limit,
                )
                .expect_err("the current specialized template retains its original budget gate");
                assert_eq!(error.kind, LoweringErrorKind::InstanceLimitExceeded);
                assert_eq!(error.span, Some(span));
                assert_eq!(typed.types().types().len(), arena_len);
            }
        }
        let complete = plan_unit_instances_with_limit(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
            3,
        )
        .expect("three distinct specializations complete the ordinary chain");
        assert_eq!(
            complete.len(),
            4,
            "entry plus exactly one Int instance of each relay"
        );
        let int = typed.types().types().builtin(BuiltinType::Int).unwrap();
        for target in targets {
            let instances = complete
                .iter()
                .filter(|instance| instance.key().target() == UnitCallableTarget::Declaration(target))
                .collect::<Vec<_>>();
            let [instance] = instances.as_slice() else {
                panic!("one instance for {target:?}")
            };
            assert_eq!(instance.key().type_arguments(), &[int]);
            assert_eq!(instance.substitutions().len(), 1);
            let symbol = typed
                .types()
                .signatures()
                .declaration(target)
                .unwrap()
                .callable()
                .unwrap()
                .type_parameters()[0];
            assert_eq!(instance.substitutions().get(&symbol), Some(&int));
        }
        assert_eq!(typed.types().types().len(), arena_len);
        let [construction] = typed.types().container_constructions() else {
            panic!("only inner constructs a source container")
        };
        assert!(
            matches!(
                typed.types().types().get(construction.element_type()),
                Some(UnitTypeKind::TypeParameter(_))
            ),
            "body facts stay symbolic after planning"
        );
    }

    #[test]
    fn unit_generic_body_budget_later_sibling_recipe_precedes_zero_one_two_limit() {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "p/provider.ko",
            "package p\n\
             interface Base<A> { fun readGrowth(): Int = 7 }\n\
             class GrowA<T>(val next: GrowB<List<T>>)\n\
             class GrowB<U>(val next: GrowA<U>)\n\
             class Host<T>: Base<GrowA<T>> {}\n\
             fun <T> cleanA(): Int {\n\
                 val values: MutableList<T> = mutableListOf()\nreturn cleanB<T>()\n}\n\
             fun <T> cleanB(): Int {\n\
                 val values: MutableList<T> = mutableListOf()\nreturn cleanC<T>()\n}\n\
             fun <T> cleanC(): Int {\n\
                 val values: MutableList<T> = mutableListOf()\nreturn values.size\n}\n\
             fun <T> bad(host: Host<T>): Int = host.readGrowth()",
        );
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            "package q\nimport p.Host\n\
             fun entry(host: Host<Int>): Int = p.cleanA<Int>() + p.bad(host)",
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let reversed = [inputs[1], inputs[0]];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        // MutableList<Int> appears only in these real bodies; the retained recipe
        // witness happens to use List<T>, so it cannot seed this concrete demand.
        assert_concrete_body_container(&typed, IntrinsicTypeConstructor::MutableList);
        let order = ["cleanA", "cleanB", "cleanC", "bad"].map(|name| declaration(&names, name));
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]));
        let witness = typed
            .types()
            .signatures()
            .declaration(declaration(&names, "GrowB"))
            .unwrap()
            .nominal()
            .unwrap()
            .fields()[0]
            .span();
        assert_eq!(witness.source_id(), provider_source);
        assert_eq!(sources.slice(witness), Ok("next"));
        let arena_len = typed.types().types().len();
        for limit in 0..=2 {
            for unit_inputs in [&inputs[..], &reversed[..]] {
                let error = plan_unit_instances_with_limit(
                    &sources,
                    unit_inputs,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    declaration(&names, "entry"),
                    limit,
                )
                .expect_err("the later pending bad recipe must win before the current limit");
                assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
                assert_eq!(error.span, Some(witness));
                assert_eq!(typed.types().types().len(), arena_len);
            }
        }
    }

    fn assert_concrete_body_container(
        typed: &ValidatedCompilationUnitTypes,
        constructor: IntrinsicTypeConstructor,
    ) {
        let arena = typed.types().types();
        let int = arena.builtin(BuiltinType::Int).unwrap();
        assert!(
            arena
                .find(&UnitTypeKind::Intrinsic {
                    constructor,
                    arguments: vec![int]
                })
                .is_some(),
            "the real frontend publishes the body-only concrete identity before planner arbitration"
        );
    }
}
mod callable_identity;
mod canonical_callables {
    //! Both concrete Function identity and its mode labels remain frontend-owned.

    use super::*;
    use crate::test_support::diagnostics::assert_diagnostic;

    #[test]
    fn canonical_callable_unit_specialization_keeps_modes_move_and_container_return() {
        let mut failures = Vec::new();
        for (function_type, concrete_type) in [
            ("(borrow Int) -> T", "(borrow Int) -> Int"),
            ("move (inout Int) -> T", "move (inout Int) -> Int"),
            ("(Int) -> Array<T>", "(Int) -> Array<Int>"),
        ] {
            let mut sources = SourceMap::new();
            let (source, p) = parsed(
                &mut sources,
                "p/callables.ko",
                &format!(
                    "package p\nfun <T> helper(f: {function_type}): Unit {{}}\n\
                 fun entry(f: {concrete_type}): Unit {{ helper<Int>(f) }}"
                ),
            );
            let inputs = [SourceUnitInput::new("root", "p/callables.ko", source, &p)];
            let (name_environment, environment) = standard_environments();
            let (names, typed, _) = analyze(&sources, &inputs, &name_environment, &environment);
            let helper = typed
                .types()
                .signatures()
                .declaration(declaration(&names, "helper"))
                .unwrap()
                .callable()
                .unwrap();
            let entry = typed
                .types()
                .signatures()
                .declaration(declaration(&names, "entry"))
                .unwrap()
                .callable()
                .unwrap();
            let substitutions = BTreeMap::from([(
                helper.type_parameters()[0],
                typed.types().types().builtin(BuiltinType::Int).unwrap(),
            )]);
            let len = typed.types().types().len();
            let resolved = super::super::unit_plan::resolve_concrete_type(
                typed.types(),
                helper.parameters()[0].ty(),
                &substitutions,
                None,
                helper.parameters()[0].span(),
            );
            assert_eq!(
                typed.types().types().len(),
                len,
                "lowering cannot intern frontend types"
            );
            match resolved {
                Ok(ty) => assert_eq!(ty, entry.parameters()[0].ty()),
                Err(error) => failures.push(format!("{function_type}: {error:?}")),
            }
            assert!(matches!(
                typed.types().types().get(entry.parameters()[0].ty()),
                Some(UnitTypeKind::Function { .. })
            ));
        }
        assert_diagnostic!(
            failures.is_empty(),
            "all canonical Function shapes must resolve: {failures:?}"
        );
    }

    #[test]
    fn canonical_callable_unit_missing_canonical_target_is_structured_and_readonly() {
        let mut sources = SourceMap::new();
        let (source, p) = parsed(
            &mut sources,
            "p/callables.ko",
            "package p\nfun <T> helper(f: (Int) -> T): Unit {}\n\
             fun entry(f: (Int) -> Int): Unit { helper<Int>(f) }",
        );
        let inputs = [SourceUnitInput::new("root", "p/callables.ko", source, &p)];
        let (name_environment, environment) = standard_environments();
        let (names, typed, _) = analyze(&sources, &inputs, &name_environment, &environment);
        let helper = typed
            .types()
            .signatures()
            .declaration(declaration(&names, "helper"))
            .unwrap()
            .callable()
            .unwrap();
        let substitutions = BTreeMap::from([(
            helper.type_parameters()[0],
            typed.types().types().builtin(BuiltinType::Boolean).unwrap(),
        )]);
        let len = typed.types().types().len();
        let error = super::super::unit_plan::resolve_concrete_type(
            typed.types(),
            helper.parameters()[0].ty(),
            &substitutions,
            None,
            helper.parameters()[0].span(),
        )
        .expect_err("absent (Int)->Boolean may not be created in codegen");
        assert_eq!(error.kind, LoweringErrorKind::MissingFact);
        assert_eq!(typed.types().types().len(), len);
    }
}
mod delegation_routes;
mod entry_identity {
    use super::*;

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

        let fields = resolve_nominal_runtime_field_types(typed.types(), owner, nominal, &[int])
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
        let error = resolve_nominal_runtime_field_types(typed.types(), owner, nominal, &[string])
            .expect_err("owner arguments cannot be replaced by another global canonical type");
        assert_eq!(error.kind, LoweringErrorKind::MissingFact);
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

    #[test]
    fn recovery_owner_without_a_layout_cannot_become_executable_ssa() {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(
            &mut sources,
            "p/recovery.ko",
            "package p\nclass Broken(val prefix: Int, val poison: Opaque)\nfun entry(input: Broken): Unit {}",
        );
        let inputs = [SourceUnitInput::new(
            "root",
            "p/recovery.ko",
            source,
            &parsed,
        )];
        let (mut name_environment, type_environment) = standard_environments();
        name_environment.declare_type("Opaque").unwrap();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
        assert!(typed.types().runtime_field_layouts().is_empty());
        let error = super::unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "entry"),
        )
        .err()
        .expect("typed recovery does not authorize lowering a poisoned field");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    }
}
mod error_order;
mod generic_containers;
mod instances {
    use super::*;

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
}
mod layout_demand;
mod owner_recipes;
mod recipe_cycles;
