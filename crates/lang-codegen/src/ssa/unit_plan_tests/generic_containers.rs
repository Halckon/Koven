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
