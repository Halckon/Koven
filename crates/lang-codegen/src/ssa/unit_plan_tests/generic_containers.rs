//! SPEC-0275: canonical identity lookup and the direct/nested substitution boundary.

use super::*;
use lang_frontend::type_checking::{UnitCallableSignature, UnitTypeId};

use crate::ssa::{
    model::{EntityType, Operation, SequentialContainerKind, SsaTypeKind},
    unit_plan::resolve_concrete_type,
};

#[test]
fn unit_generic_container_instances_keep_source_parameters_and_deduplicate() {
    let mut sources = SourceMap::new();
    let (p_source, p) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun <T> arraySize(items: Array<T>): Int = items.size",
    );
    let (q_source, q) = parsed(
        &mut sources,
        "q/consumer.ko",
        r#"package q
        fun <T> listSize(items: List<T>): Int = items.size
        fun entry(): Int {
            val a = arrayOf(1)
            val s = arrayOf("text")
            val l = listOf(2)
            val t = listOf("other")
            return p.arraySize(a) + p.arraySize<Int>(a) + p.arraySize(s) + listSize(l) + listSize<Int>(l) + listSize(t)
        }"#,
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", p_source, &p),
        SourceUnitInput::new("root", "q/consumer.ko", q_source, &q),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let entry = declaration(&names, "entry");
    let array = declaration(&names, "arraySize");
    let list = declaration(&names, "listSize");
    let array_parameter = callable(&typed, array).type_parameters()[0];
    let list_parameter = callable(&typed, list).type_parameters()[0];
    assert_ne!(
        array_parameter, list_parameter,
        "spelling T is not identity"
    );
    assert_ne!(array_parameter.source_unit(), list_parameter.source_unit());
    let arena_len = typed.types().types().len();
    let forward = plan(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        entry,
    );
    let backward = plan(
        &sources,
        &reversed,
        &names,
        &environment,
        &typed,
        &owned,
        entry,
    );
    assert_eq!(forward, backward);
    assert_eq!(typed.types().types().len(), arena_len);
    assert_eq!(forward.len(), 5, "entry plus two Int/String pairs");
    let (fresh_names, fresh_typed, fresh_owned) =
        analyze(&sources, &reversed, &name_environment, &environment);
    let fresh = plan(
        &sources,
        &reversed,
        &fresh_names,
        &environment,
        &fresh_typed,
        &fresh_owned,
        declaration(&fresh_names, "entry"),
    );
    assert_eq!(
        forward, fresh,
        "fresh analysis keeps canonical instance order"
    );
    assert_eq!(fresh_typed.types().types().len(), arena_len);
    for (target, symbol, constructor) in [
        (array, array_parameter, IntrinsicTypeConstructor::Array),
        (list, list_parameter, IntrinsicTypeConstructor::List),
    ] {
        let instances = forward
            .iter()
            .filter(|instance| instance.key().target() == UnitCallableTarget::Declaration(target))
            .collect::<Vec<_>>();
        assert_eq!(instances.len(), 2);
        let arguments = instances
            .iter()
            .map(|instance| instance.key().type_arguments()[0])
            .collect::<BTreeSet<_>>();
        assert_eq!(
            arguments,
            [BuiltinType::Int, BuiltinType::String]
                .map(|builtin| typed.types().types().builtin(builtin).unwrap())
                .into_iter()
                .collect()
        );
        for instance in instances {
            assert_eq!(
                instance.substitutions().keys().copied().collect::<Vec<_>>(),
                [symbol],
                "a callable only substitutes its own source-qualified T"
            );
            let parameter = &callable(&typed, target).parameters()[0];
            let concrete =
                canonical_container(&typed, constructor, instance.key().type_arguments()[0]);
            let resolved = resolve_concrete_type(
                typed.types(),
                parameter.ty(),
                instance.substitutions(),
                instance.key().static_self(),
                parameter.span(),
            );
            assert_eq!(typed.types().types().len(), arena_len);
            assert_eq!(resolved.unwrap(), concrete);
        }
    }
}

#[test]
fn unit_generic_container_direct_parameter_accepts_existing_concrete_nested_argument() {
    for (container, constructor, intrinsic, kind) in [
        (
            "Array",
            "arrayOf",
            IntrinsicTypeConstructor::Array,
            SequentialContainerKind::Array,
        ),
        (
            "List",
            "listOf",
            IntrinsicTypeConstructor::List,
            SequentialContainerKind::List,
        ),
        (
            "MutableList",
            "mutableListOf",
            IntrinsicTypeConstructor::MutableList,
            SequentialContainerKind::MutableList,
        ),
    ] {
        let mut sources = SourceMap::new();
        let provider = format!(
            "package p\n\
             fun <T> sizeOf(items: {container}<T>): Int = items.size\n\
             fun <T> pass(own items: {container}<T>): {container}<T> = items"
        );
        let consumer = format!(
            "package q\nfun entry(): Int {{\n\
                 val inner = listOf(1, 2)\n\
                 val outer = {constructor}(inner)\n\
                 val before = p.sizeOf<List<Int>>(outer)\n\
                 val returned = p.pass(outer)\n\
                 return before + returned.size\n\
             }}"
        );
        let (p_source, p) = parsed(&mut sources, "p/provider.ko", &provider);
        let (q_source, q) = parsed(&mut sources, "q/consumer.ko", &consumer);
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", p_source, &p),
            SourceUnitInput::new("root", "q/consumer.ko", q_source, &q),
        ];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let int = typed.types().types().builtin(BuiltinType::Int).unwrap();
        let inner = canonical_container(&typed, IntrinsicTypeConstructor::List, int);
        let concrete = canonical_container(&typed, intrinsic, inner);
        let arena_len = typed.types().types().len();
        let instances = plan(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "entry"),
        );
        let target = declaration(&names, "sizeOf");
        let instance = instances
            .iter()
            .find(|instance| instance.key().target() == UnitCallableTarget::Declaration(target))
            .unwrap();
        assert_eq!(instance.key().type_arguments(), [inner]);
        let parameter = &callable(&typed, target).parameters()[0];
        let resolved = resolve_concrete_type(
            typed.types(),
            parameter.ty(),
            instance.substitutions(),
            None,
            parameter.span(),
        );
        assert_eq!(typed.types().types().len(), arena_len);
        assert_eq!(resolved.unwrap(), concrete);
        let lowered = unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "entry"),
        );
        assert_eq!(typed.types().types().len(), arena_len);
        let (program, _) = lowered.expect("direct T may be an already concrete nested container");
        let module = &program.modules[0];
        let size = module
            .functions
            .iter()
            .find(|function| function.name.contains("p.sizeOf"))
            .unwrap();
        let EntityType::Loan { target, .. } = size.entity(size.blocks[0].parameters[0]).unwrap().ty
        else {
            panic!("size receives the concrete outer shared loan")
        };
        let Some(SsaTypeKind::SequentialContainer {
            kind: actual,
            element,
        }) = module.type_kind(target)
        else {
            panic!("outer parameter is a sequential container")
        };
        assert_eq!(*actual, kind);
        assert!(matches!(
            module.type_kind(*element),
            Some(SsaTypeKind::SequentialContainer {
                kind: SequentialContainerKind::List,
                ..
            })
        ));
        let entry = module
            .functions
            .iter()
            .find(|function| function.name.contains("q.entry"))
            .unwrap();
        assert_eq!(
            entry
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
                .count(),
            1,
            "the returned outer owner retains cleanup for its nested elements"
        );
        crate::llvm::render_verified_program(&program).expect("nested concrete drop glue verifies");
    }
}

#[test]
fn unit_generic_container_recursive_template_stays_unsupported_with_canonical_present() {
    let mut sources = SourceMap::new();
    let (p_source, p) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun <T> nestedSize(items: List<List<T>>): Int = items.size",
    );
    let (q_source, q) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int {\n\
             val values = listOf(listOf(1))\n\
             return p.nestedSize<Int>(values)\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", p_source, &p),
        SourceUnitInput::new("root", "q/consumer.ko", q_source, &q),
    ];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let int = typed.types().types().builtin(BuiltinType::Int).unwrap();
    let inner = canonical_container(&typed, IntrinsicTypeConstructor::List, int);
    canonical_container(&typed, IntrinsicTypeConstructor::List, inner);
    let arena_len = typed.types().types().len();
    let target = declaration(&names, "nestedSize");
    let signature = callable(&typed, target);
    let parameter = &signature.parameters()[0];
    assert_eq!(
        source_slice(&sources, parameter.span()),
        "items: List<List<T>>"
    );
    let substitutions = BTreeMap::from([(signature.type_parameters()[0], int)]);
    let error = resolve_concrete_type(
        typed.types(),
        parameter.ty(),
        &substitutions,
        None,
        parameter.span(),
    )
    .expect_err("canonical presence does not authorize recursive template substitution");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(parameter.span()));
    assert_eq!(typed.types().types().len(), arena_len);
    let error = unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .err()
    .expect("recursive template fails before a program is returned");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(parameter.span()));
    assert_eq!(typed.types().types().len(), arena_len);
}

#[test]
fn unit_generic_container_body_only_uses_frontend_canonical_without_extending_arena() {
    let mut sources = SourceMap::new();
    let (p_source, p) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun <T> probe(own x: T): Int {\n\
             val xs = listOf(x)\n\
             return xs.size\n\
         }",
    );
    let (q_source, q) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.probe(1)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", p_source, &p),
        SourceUnitInput::new("root", "q/consumer.ko", q_source, &q),
    ];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let int = typed.types().types().builtin(BuiltinType::Int).unwrap();
    let expected = UnitTypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::List,
        arguments: vec![int],
    };
    let concrete = typed
        .types()
        .types()
        .find(&expected)
        .expect("the actual call publishes List<Int>");
    let [construction] = typed.types().container_constructions() else {
        panic!("only the generic body constructs a list")
    };
    let span = p
        .ast()
        .expressions()
        .get(construction.expression().expression())
        .unwrap()
        .span();
    assert_eq!(source_slice(&sources, span), "listOf(x)");
    assert_eq!(span.source_id(), p_source);
    let signature = callable(&typed, declaration(&names, "probe"));
    let substitutions = BTreeMap::from([(signature.type_parameters()[0], int)]);
    let arena_len = typed.types().types().len();
    let resolved = resolve_concrete_type(
        typed.types(),
        construction.container_type(),
        &substitutions,
        None,
        span,
    )
    .expect("backend only finds the body-only concrete list identity");
    assert_eq!(typed.types().types().len(), arena_len);
    assert_eq!(resolved, concrete);
    let lowered = unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    );
    assert_eq!(typed.types().types().len(), arena_len);
    let (program, _) = lowered.expect("body-only construction lowers with the published identity");
    crate::llvm::render_verified_program(&program)
        .expect("the body-only owner and size verify through LLVM");
    assert_eq!(typed.types().types().find(&expected), Some(concrete));
}

#[test]
fn unit_generic_container_missing_substitution_keeps_actual_parameter_span() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         fun <T> sizeOf(items: List<T>): Int = items.size\n\
         fun entry(): Int { val items = listOf(1); return sizeOf(items) }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let (names, typed, _) = analyze(&sources, &inputs, &name_environment, &environment);
    let int = typed.types().types().builtin(BuiltinType::Int).unwrap();
    canonical_container(&typed, IntrinsicTypeConstructor::List, int);
    let signature = callable(&typed, declaration(&names, "sizeOf"));
    let parameter = &signature.parameters()[0];
    assert_eq!(source_slice(&sources, parameter.span()), "items: List<T>");
    let UnitTypeKind::Intrinsic { arguments, .. } =
        typed.types().types().get(parameter.ty()).unwrap()
    else {
        panic!("parameter retains the direct container template")
    };
    let arena_len = typed.types().types().len();
    for template in [arguments[0], parameter.ty()] {
        let error = resolve_concrete_type(
            typed.types(),
            template,
            &BTreeMap::new(),
            None,
            parameter.span(),
        )
        .expect_err("neither direct T nor Container<T> can guess a missing substitution");
        assert_eq!(typed.types().types().len(), arena_len);
        assert_eq!(error.kind, LoweringErrorKind::MissingFact);
        assert_eq!(error.span, Some(parameter.span()));
    }
}

fn callable(
    typed: &ValidatedCompilationUnitTypes,
    declaration: DeclarationId,
) -> &UnitCallableSignature {
    typed
        .types()
        .signatures()
        .declaration(declaration)
        .and_then(|signature| signature.callable())
        .expect("source callable signature")
}

fn canonical_container(
    typed: &ValidatedCompilationUnitTypes,
    constructor: IntrinsicTypeConstructor,
    element: UnitTypeId,
) -> UnitTypeId {
    typed
        .types()
        .types()
        .find(&UnitTypeKind::Intrinsic {
            constructor,
            arguments: vec![element],
        })
        .expect("real construction/call signature publishes the canonical container")
}

fn source_slice(sources: &SourceMap, span: lang_frontend::source::Span) -> &str {
    &sources.source_text(span.source_id()).unwrap()[span.start()..span.end()]
}

pub(in crate::ssa) mod error_order {
use super::super::*;
use crate::test_support::diagnostics::assert_eq_diagnostic;

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
        assert_eq_diagnostic!(error.kind, LoweringErrorKind::UnsupportedNode, "{error:?}");
        assert_eq!(error.span, Some(a_witness));
        assert_ne!(error.span, Some(z_witness));
    }
}
}
