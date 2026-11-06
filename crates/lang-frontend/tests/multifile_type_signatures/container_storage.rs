//! Signature-only and full Unit rejection of non-storable builtin container elements.
use super::*;
use lang_frontend::type_checking::{
    FinalTypeAdmissionError, check_compilation_unit_types, complete_compilation_unit_types,
};

fn check(text: &str, expected: &[(&str, &str)]) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "storage.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::memory_document(source, &file),
        SourceUnitInput::new("root", "storage.ko", source, &file),
    ] {
        let inputs = [input];
        let names = names(&sources, &inputs, &environment);
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid names");
        let signatures = collect_compilation_unit_signatures(&sources, &inputs, &names, &types)
            .expect("signature analysis");
        let typed =
            check_compilation_unit_types(&sources, &inputs, &names, &types).expect("full analysis");
        for diagnostics in [signatures.diagnostics(), typed.diagnostics()] {
            assert_eq!(diagnostics.len(), expected.len(), "{text}: {diagnostics:?}");
            for (diagnostic, (code, span_text)) in diagnostics.iter().zip(expected) {
                assert_eq!(diagnostic.code().to_string(), *code, "{text}");
                assert_eq!(
                    sources.slice(diagnostic.primary_span()).unwrap(),
                    *span_text
                );
                assert_eq!(diagnostic.primary_span().source_id(), source);
            }
        }
        assert_eq!(signatures.diagnostics(), typed.diagnostics());
        if expected.is_empty() {
            assert!(signatures.validate().is_ok());
            assert!(typed.validate().is_ok());
        } else {
            assert!(signatures.validate().is_err());
            let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) =
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            else {
                panic!("invalid signature must fail the final gate with diagnostics")
            };
            assert_eq!(diagnostics, typed.diagnostics());
            assert!(typed.validate().is_err());
        }
    }
}

#[test]
fn builtin_bottom_and_top_elements_are_rejected_in_each_signature_container() {
    for container in ["Array", "List", "MutableList"] {
        for element in ["Any", "Nothing", "Any?", "Nothing?"] {
            for outer_nullable in ["", "?"] {
                check(
                    &format!("fun inspect(value: {container}<{element}>{outer_nullable}): Unit"),
                    &[("L0125", element)],
                );
            }
        }
    }
}

#[test]
fn signature_storage_checks_cover_fields_returns_and_function_types() {
    for text in [
        "class Holder(val value: List<Nothing>)",
        "fun output(): Array<Nothing>",
        "fun callback(input: (MutableList<Nothing>) -> Unit): Unit",
        "fun callback(input: () -> List<Nothing>): Unit",
    ] {
        check(text, &[("L0125", "Nothing")]);
    }
}

#[test]
fn nested_signature_containers_preserve_one_root_cause() {
    for text in [
        "fun inspect(value: List<Array<Nothing>>): Unit",
        "fun inspect(value: Array<MutableList<Any?>?>): Unit",
    ] {
        let leaf = if text.contains("Nothing") {
            "Nothing"
        } else {
            "Any?"
        };
        check(text, &[("L0125", leaf)]);
    }
}

#[test]
fn existing_signature_container_diagnostics_keep_their_order() {
    check(
        "interface Shape\nfun invalid(never: List<Nothing>, top: Array<Any>, dynamic: List<Shape>, arity: MutableList<Int, Long>, callable: List<(Nothing) -> Nothing>): Unit {}",
        &[
            ("L0125", "Nothing"),
            ("L0125", "Any"),
            ("L0094", "Shape"),
            ("L0091", "MutableList"),
        ],
    );
}

#[test]
fn valid_function_elements_generics_and_source_names_remain_accepted() {
    check(
        "fun <T> inspect(a: Array<Int?>, b: List<T>, c: MutableList<(Nothing) -> Nothing>): Unit",
        &[],
    );
    check(
        "class List<T>\nfun inspect(value: List<Nothing>): Unit",
        &[],
    );
}

#[test]
fn cross_file_signature_storage_diagnostics_are_input_order_independent() {
    let mut sources = SourceMap::new();
    let (a_id, a) = parsed(
        &mut sources,
        "a.ko",
        "package p\npublic class Holder(val values: List<Any>)",
    );
    let (b_id, b) = parsed(
        &mut sources,
        "b.ko",
        "package p\nfun inspect(holder: Holder, values: Array<Nothing>): Unit",
    );
    let forward = [
        SourceUnitInput::new("root", "p/a.ko", a_id, &a),
        SourceUnitInput::new("root", "p/b.ko", b_id, &b),
    ];
    let reverse = [forward[1], forward[0]];
    let (environment, types) = standard_environments();
    let mut previous = None;
    for inputs in [forward, reverse] {
        let names = names(&sources, &inputs, &environment)
            .validate()
            .expect("valid names");
        let signatures =
            collect_compilation_unit_signatures(&sources, &inputs, &names, &types).unwrap();
        assert_eq!(codes(signatures.diagnostics()), ["L0125", "L0125"]);
        assert_eq!(signatures.diagnostics()[0].primary_span().source_id(), a_id);
        assert_eq!(signatures.diagnostics()[1].primary_span().source_id(), b_id);
        if let Some(previous) = previous {
            assert_eq!(signatures, previous);
        }
        previous = Some(signatures);
    }
}

#[test]
fn nested_errors_preserve_arity_diagnostics_without_storage_cascades() {
    check(
        "fun inspect(value: List<Array<Int, Long>>): Unit",
        &[("L0091", "Array")],
    );
    check(
        "fun inspect(value: MutableList<Nothing, Int>): Unit",
        &[("L0091", "MutableList")],
    );
}

#[test]
fn invalid_elements_do_not_merge_distinct_overload_shapes() {
    check(
        "fun f(x: List<Nothing>): Unit\nfun f(x: Array<Any>): Unit",
        &[("L0125", "Nothing"), ("L0125", "Any")],
    );
    check(
        "class Holder {\nfun f(x: List<Nothing>): Unit {}\nfun f(x: Array<Any>): Unit {}\n}",
        &[("L0125", "Nothing"), ("L0125", "Any")],
    );
    check(
        "fun f(x: (List<Nothing>) -> Unit): Unit\nfun f(x: (Array<Any>) -> Unit): Unit",
        &[("L0125", "Nothing"), ("L0125", "Any")],
    );
}

#[test]
fn repeated_invalid_elements_keep_distinct_spans_and_valid_neighbor_types() {
    let text = "fun inspect(good: List<Int>, first: List<Nothing>, second: List<Nothing>): Unit";
    check(text, &[("L0125", "Nothing"), ("L0125", "Nothing")]);
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "storage.ko", text);
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::memory_document(source, &file)];
    let names = names(&sources, &inputs, &environment).validate().unwrap();
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &names, &types).unwrap();
    let expected: Vec<_> = text
        .match_indices("Nothing")
        .map(|(start, _)| {
            sources
                .span(source, start, start + "Nothing".len())
                .unwrap()
        })
        .collect();
    assert_eq!(
        signatures
            .diagnostics()
            .iter()
            .map(|d| d.primary_span())
            .collect::<Vec<_>>(),
        expected
    );
    let source_unit = names.names().index().source_units()[0].id();
    for (id, node) in file.ast().type_refs().iter() {
        if sources.slice(node.span()).unwrap() == "List<Int>" {
            let ty = signatures
                .type_ref_type(UnitTypeRefId::new(source_unit, id))
                .unwrap();
            let Some(UnitTypeKind::Intrinsic {
                constructor: lang_frontend::type_checking::IntrinsicTypeConstructor::List,
                arguments,
            }) = signatures.types().get(ty)
            else {
                panic!("valid neighbor keeps List identity")
            };
            assert!(matches!(
                signatures.types().get(arguments[0]),
                Some(UnitTypeKind::Builtin(
                    lang_frontend::type_checking::BuiltinType::Int
                ))
            ));
            return;
        }
    }
    panic!("original valid neighbor type ref exists");
}
