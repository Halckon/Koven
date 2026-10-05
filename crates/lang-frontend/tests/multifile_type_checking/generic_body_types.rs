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
