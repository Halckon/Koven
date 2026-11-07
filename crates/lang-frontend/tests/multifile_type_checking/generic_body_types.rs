//! SPEC-0276: actual calls publish body-only concrete types without signature seeds.

use super::*;
use lang_frontend::type_checking::UnitTypeId;

#[test]
fn unit_generic_body_array_types_come_from_actual_calls() {
    assert_body_only_container_types("Array", "arrayOf", IntrinsicTypeConstructor::Array);
}

#[test]
fn unit_generic_body_list_types_come_from_actual_calls() {
    assert_body_only_container_types("List", "listOf", IntrinsicTypeConstructor::List);
}

#[test]
fn unit_generic_body_mutable_list_types_come_from_actual_calls() {
    assert_body_only_container_types(
        "MutableList",
        "mutableListOf",
        IntrinsicTypeConstructor::MutableList,
    );
}

fn assert_body_only_container_types(
    container: &str,
    construct: &str,
    constructor: IntrinsicTypeConstructor,
) {
    let mut missing = Vec::new();
    for values in [
        "1, 2",
        "\"one\", \"two\"",
        "Resource(\"one\"), Resource(\"two\")",
    ] {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "provider.ko",
            &format!(
                "package p\n\
                 class Resource(val name: String) {{ deinit() {{ println(this.name) }} }}\n\
                 fun <T> probe(own first: T, own second: T): Int {{\n\
                     val empty: {container}<T> = {construct}()\n\
                     val full = {construct}(first, second)\n\
                     val again: {container}<T> = {construct}()\n\
                     return empty.size + full.size + again.size\n\
                 }}",
            ),
        );
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "consumer.ko",
            &format!("package q\nimport p.Resource\nfun entry(): Int = p.probe({values})"),
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("body-only generic source is supported by the type checker");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(typed.clone().validate().is_ok());
        let probe = declaration(&names, "probe");
        let call = typed
            .calls()
            .iter()
            .find(|call| call.target() == UnitCallTarget::Declaration(probe))
            .expect("only the actual consumer instantiates probe");
        let [element] = call.instance().type_arguments() else {
            panic!("probe has one ordered actual type argument");
        };
        if typed
            .types()
            .find(&UnitTypeKind::Intrinsic {
                constructor,
                arguments: vec![*element],
            })
            .is_none()
        {
            missing.push(values);
        }
        let template = typed
            .signatures()
            .declaration(probe)
            .unwrap()
            .callable()
            .unwrap();
        let symbol = template.type_parameters()[0];
        let parameter = typed
            .types()
            .find(&UnitTypeKind::TypeParameter(symbol))
            .unwrap();
        let constructs = typed.container_constructions();
        assert_eq!(
            constructs.len(),
            3,
            "empty, full and second empty live only in the generic body"
        );
        for fact in constructs {
            assert_eq!(
                fact.expression().source_unit(),
                source_unit(&names, provider_source)
            );
            assert_eq!(
                typed.types().get(fact.container_type()),
                Some(&UnitTypeKind::Intrinsic {
                    constructor,
                    arguments: vec![parameter]
                }),
                "publishing concrete identities must retain the original template facts",
            );
        }
    }
    assert!(
        missing.is_empty(),
        "body-only {container} identities missing for {missing:?}; no signature seeds them"
    );
}

#[test]
fn unit_generic_body_transitive_calls_publish_nullable_and_container_types() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "provider.ko",
        "package p\n\
         fun <T> inner(own item: T): Int {\n\
             val none: T? = null\n\
             val values = listOf(item)\n\
             return values.size\n\
         }\n\
         fun <T> middle(own item: T): Int = inner(item)\n\
         fun <T> relay(own item: T): Int = middle(item)",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "consumer.ko",
        "package q\nfun entry(): Int = p.relay(1)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let int = typed.types().builtin(BuiltinType::Int).unwrap();
    let provider_unit = source_unit(&names, provider_source);
    let consumer_unit = source_unit(&names, consumer_source);
    let relay = declaration(&names, "relay");
    for (target, caller) in [("middle", "relay"), ("inner", "middle")] {
        let caller = typed
            .signatures()
            .declaration(declaration(&names, caller))
            .unwrap()
            .callable()
            .unwrap();
        let parameter = typed
            .types()
            .find(&UnitTypeKind::TypeParameter(caller.type_parameters()[0]))
            .unwrap();
        let call = typed
            .calls()
            .iter()
            .find(|call| call.target() == UnitCallTarget::Declaration(declaration(&names, target)))
            .unwrap();
        assert_eq!(call.expression().source_unit(), provider_unit);
        assert_eq!(
            call.instance().type_arguments(),
            &[parameter],
            "source call retains its caller's T"
        );
    }
    let actual = typed
        .calls()
        .iter()
        .find(|call| call.target() == UnitCallTarget::Declaration(relay))
        .unwrap();
    assert_eq!(actual.expression().source_unit(), consumer_unit);
    assert_eq!(actual.instance().type_arguments(), &[int]);
    for kind in [
        UnitTypeKind::Nullable(int),
        UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::List,
            arguments: vec![int],
        },
    ] {
        assert!(
            typed.types().find(&kind).is_some(),
            "transitive body demand: {kind:?}"
        );
    }
    assert_eq!(
        typed.calls().len(),
        3,
        "normalization must not rewrite symbolic source calls"
    );
}

#[test]
fn unit_generic_body_without_concrete_call_does_not_choose_int() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "main.ko",
        "fun <T> probe(own item: T): Int { val values = listOf(item); return values.size }\n\
         fun entry(): Int = 0",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let int: UnitTypeId = typed.types().builtin(BuiltinType::Int).unwrap();
    assert_eq!(
        typed.types().find(&UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::List,
            arguments: vec![int],
        }),
        None,
        "an uncalled template provides no actual Int substitution"
    );
}

#[test]
fn unit_generic_body_closed_enum_refinement_is_an_actual_seed() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "main.ko",
        "enum class Shape { Circle, Square }\n\
         fun <T> probe(own item: T): Int { val values = listOf(item); return values.size }\n\
         fun use(own shape: Shape): Int {\n\
             if (shape is Shape.Circle) { return probe(shape) }\n\
             return 0\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.clone().validate().is_ok());
    let probe = declaration(&names, "probe");
    let call = typed
        .calls()
        .iter()
        .find(|call| call.target() == UnitCallTarget::Declaration(probe))
        .unwrap();
    let [actual] = call.instance().type_arguments() else {
        panic!("one actual");
    };
    assert!(
        matches!(
            typed.types().get(*actual),
            Some(UnitTypeKind::EnumCase { .. })
        ),
        "inference retains the flow refinement"
    );
    assert!(
        typed
            .types()
            .find(&UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments: vec![*actual],
            })
            .is_some(),
        "closed source arguments are seeds even if their runtime layout stays unsupported"
    );
}

#[test]
fn instantiated_container_signature_rejects_unstorable_elements_before_publishing_call() {
    for (text, primary, rejected) in [
        (
            "fun <T> fail(): List<T> = error(\"never\")\nfun entry(): Unit { fail<Nothing>() }",
            "fail",
            "fail<Nothing>()",
        ),
        (
            "fun <T> accept(value: Array<T>?): Unit {}\nfun entry(): Unit { accept<Any>(null) }",
            "accept",
            "accept<Any>(null)",
        ),
        (
            "class Factory<T> { fun create(): List<T> = error(\"never\") }\nfun entry(): Unit { Factory<Nothing>().create() }",
            "Factory<Nothing>().create",
            "Factory<Nothing>().create()",
        ),
    ] {
        check_storage_instances(text, |sources, file, source_unit, typed| {
            assert_storage_diagnostic(sources, typed, primary);
            let rejected =
                UnitExpressionId::new(source_unit, expression_with_text(sources, file, rejected));
            assert!(
                typed
                    .calls()
                    .iter()
                    .all(|call| call.expression() != rejected)
            );
        });
    }
}

#[test]
fn invalid_container_candidate_does_not_poison_viable_overload() {
    let text = "class Holder<T>(val value: Int)\n\
                fun <T> choose(value: Int): (Holder<T>) -> List<T> = error(\"never\")\n\
                fun <T> choose(value: Long): Int = 7\n\
                fun entry(): Int = choose<Nothing>(1)";
    check_storage_instances(text, |sources, file, source_unit, typed| {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let expression = UnitExpressionId::new(
            source_unit,
            expression_with_text(sources, file, "choose<Nothing>(1)"),
        );
        let call = typed
            .calls()
            .iter()
            .find(|call| call.expression() == expression)
            .unwrap();
        assert_eq!(
            typed.types().get(call.arguments()[0].parameter_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Long))
        );
        let holder = typed
            .signatures()
            .declarations()
            .iter()
            .find_map(|signature| signature.nominal())
            .expect("Holder is the sole nominal");
        assert!(
            typed
                .types()
                .find(&UnitTypeKind::Nominal {
                    declaration: holder.declaration(),
                    arguments: vec![typed.types().builtin(BuiltinType::Nothing).unwrap()],
                })
                .is_none(),
            "discarded storage candidate must not seed a runtime layout"
        );
        assert!(typed.clone().validate().is_ok());
    });
}

#[test]
fn storage_rejection_discards_layouts_added_by_bound_substitution() {
    let text = "interface Pattern<T>\n\
                class Holder<T>(val value: Int)\n\
                class Marker<T> : Pattern<Holder<T>>\n\
                fun <T, U: Pattern<Holder<T>>> choose(value: Int, marker: U): List<T> = error(\"never\")\n\
                fun <T, U> choose(value: Long, marker: U): Int = 7\n\
                fun entry(): Int = choose<Nothing, Marker<Nothing>>(1, Marker<Nothing>())";
    check_storage_instances(text, |_, _, _, typed| {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let [layout] = typed.runtime_field_layouts() else {
            panic!("only the selected call's concrete Marker layout should survive");
        };
        assert!(layout.fields().is_empty(), "Marker has no fields");
        assert!(typed.clone().validate().is_ok());
    });
}

#[test]
fn body_only_container_storage_is_checked_at_concrete_call_roots() {
    for argument in ["Nothing", "Any"] {
        let text = format!(
            "fun <T> store(): Unit {{ val values: MutableList<T>? = null }}\n\
             fun <T> outer(): Unit {{ store<T>() }}\n\
             fun entry(): Unit {{ outer<{argument}>() }}"
        );
        check_storage_instances(&text, |sources, _, _, typed| {
            assert_storage_diagnostic(sources, typed, &format!("outer<{argument}>()"));
        });
    }
}

fn check_storage_instances(
    text: &str,
    mut inspect: impl FnMut(&SourceMap, &ParsedFile, SourceUnitId, &CompilationUnitTypes),
) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "storage.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::memory_document(source, &file),
        SourceUnitInput::new("root", "storage.ko", source, &file),
    ] {
        let inputs = [input];
        let names = validated_names(&sources, &inputs, &environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.signatures().diagnostics().is_empty());
        inspect(&sources, &file, source_unit(&names, source), &typed);
    }
}

fn assert_storage_diagnostic(sources: &SourceMap, typed: &CompilationUnitTypes, primary: &str) {
    let [diagnostic] = typed.diagnostics() else {
        panic!(
            "expected one storage diagnostic at {primary}: {:?}",
            typed.diagnostics()
        );
    };
    assert_eq!(diagnostic.code().to_string(), "L0125");
    assert_eq!(sources.slice(diagnostic.primary_span()), Ok(primary));
    assert!(typed.clone().validate().is_err());
}
