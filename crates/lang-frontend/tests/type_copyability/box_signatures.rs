use super::*;

#[test]
fn invalid_box_signature_positions_keep_one_root_diagnostic_and_error_type() {
    let text = "value class Point(val x: Int)\n\
                class Holder(val boxed: Box<Int>)\n\
                enum class Carrier { Payload(item: Box<Int>) }\n\
                fun parameter(arg: Box<Int>): Unit {}\n\
                fun result(): Box<Int>\n\
                fun functionType(arg: (Box<Int>) -> Unit): Unit {}\n\
                fun nullable(arg: Box<Int>?): Unit {}\n\
                fun nullableArgument(arg: Box<Point?>): Unit {}";
    let (sources, parsed, entries) = checked_entries(text, true);
    for (names, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0117"; 7]);
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("span"))
                .collect::<Vec<_>>(),
            ["Int", "Int", "Int", "Int", "Int", "Int", "Point?"]
        );
        assert_eq!(typed.signatures().diagnostics(), typed.diagnostics());
        assert!(typed.body_diagnostics().is_empty());
        let unit = names.source_units()[0].source_unit();
        let box_refs = parsed
            .ast()
            .type_refs()
            .iter()
            .filter(|(_, node)| {
                matches!(
                    sources.slice(node.span()),
                    Ok("Box<Int>" | "Box<Int>?" | "Box<Point?>")
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(box_refs.len(), 7);
        for (id, _) in box_refs {
            let ty = typed
                .type_ref_type(lang_frontend::type_checking::UnitTypeRefId::new(unit, id))
                .expect("signature type ref");
            assert_eq!(typed.types().get(ty), Some(&UnitTypeKind::Error));
        }
        assert!(typed.validate().is_err());
    }
}

#[test]
fn nested_box_signature_errors_keep_inner_root_causes_without_cascades() {
    let text = "value class Point(val x: Int)\n\
                fun bad(a: Box<Box<Int>>, b: Box<Box>, c: Box<Point, Point>,\n\
                        d: Box<() -> Int<String>>, e: Box<Point<Int>>): Unit {}";
    let (sources, _, entries) = checked_entries(text, true);
    for (names, typed) in entries {
        assert_eq!(
            codes(typed.diagnostics()),
            ["L0117", "L0091", "L0091", "L0082", "L0091"]
        );
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("span"))
                .collect::<Vec<_>>(),
            ["Int", "Box", "Box", "String", "Int"]
        );
        assert_eq!(typed.signatures().diagnostics(), typed.diagnostics());
        assert!(typed.body_diagnostics().is_empty());
        for name in ["a", "b", "c", "d", "e"] {
            let ty = typed
                .symbol_type(symbol(&typed, &names, name))
                .expect("parameter type");
            assert_eq!(typed.types().get(ty), Some(&UnitTypeKind::Error), "{name}");
        }
    }
}

#[test]
fn interface_box_arguments_keep_runtime_root_diagnostic_even_in_static_bounds() {
    for (text, primaries) in [
        (
            "interface Protocol\nfun use(arg: Box<Protocol>): Unit {}",
            vec!["Protocol"],
        ),
        (
            "interface Protocol\nfun <T: Box<Protocol>> use(): Unit {}",
            vec!["Protocol"],
        ),
        (
            "interface Protocol\nclass C : Box<Protocol>",
            vec!["Protocol"],
        ),
        (
            "interface Protocol\nfun use(arg: Box<Protocol?>): Unit {}",
            vec!["Protocol?"],
        ),
        (
            "interface Protocol\nfun use(arg: Box<() -> Protocol>): Unit {}",
            vec!["Protocol"],
        ),
        (
            "interface Protocol\nfun use(arg: Box<(Protocol) -> Protocol>): Unit {}",
            vec!["Protocol", "Protocol"],
        ),
        (
            "interface Protocol\nvalue class Wrap<T>(val x: T)\nfun use(arg: Box<Wrap<Protocol>>): Unit {}",
            vec!["Protocol"],
        ),
    ] {
        let (sources, parsed, entries) = checked_entries(text, true);
        for (names, typed) in entries {
            assert_eq!(
                codes(typed.diagnostics()),
                vec!["L0094"; primaries.len()],
                "{text}"
            );
            assert_eq!(
                typed
                    .diagnostics()
                    .iter()
                    .map(|d| sources.slice(d.primary_span()).expect("span"))
                    .collect::<Vec<_>>(),
                primaries
            );
            assert_eq!(typed.signatures().diagnostics(), typed.diagnostics());
            assert!(typed.body_diagnostics().is_empty());
            let unit = names.source_units()[0].source_unit();
            for (id, node) in parsed.ast().type_refs().iter() {
                let ty = typed
                    .type_ref_type(lang_frontend::type_checking::UnitTypeRefId::new(unit, id))
                    .expect("signature type ref");
                let span_text = sources.slice(node.span()).expect("span");
                if span_text.starts_with("Box<") || span_text.starts_with('(') {
                    assert_eq!(typed.types().get(ty), Some(&UnitTypeKind::Error));
                }
            }
        }
    }
}

#[test]
fn valid_box_generic_payloads_keep_known_nominal_kind_without_extra_bounds() {
    let text = "value class Wrap<T>(val value: T)\n\
                fun <T> use(boxed: Box<Wrap<T>>, nullable: Box<Wrap<T>>?): Unit {}";
    let (_, _, entries) = checked_entries(text, true);
    for (names, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let ty = typed
            .symbol_type(symbol(&typed, &names, "boxed"))
            .expect("parameter type");
        assert!(matches!(
            typed.types().get(ty),
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Box,
                ..
            })
        ));
        let ty = typed
            .symbol_type(symbol(&typed, &names, "nullable"))
            .expect("parameter type");
        let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(ty) else {
            panic!("nullable Box")
        };
        assert!(matches!(
            typed.types().get(*inner),
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Box,
                ..
            })
        ));
    }
}

#[test]
fn cross_file_box_signatures_keep_forward_recursive_identity_and_diagnostics() {
    let mut sources = SourceMap::new();
    let models_id = sources.add_source("models.ko", "package p\nvalue class Later(val n: Int)\nenum class Tree { End, Node(next: Box<Tree>) }\nvalue class Link(val next: Box<Link>?)\nclass Ref").expect("models");
    let uses_id = sources.add_source("uses.ko", "package p\nfun use(point: Box<Later>, tree: Box<Tree>, link: Box<Link>, bad: Box<Ref>): Unit {}").expect("uses");
    let models = parse_file_twice(&sources, models_id, "Box models");
    let uses = parse_file_twice(&sources, uses_id, "Box uses");
    assert!(models.diagnostics().is_empty());
    assert!(uses.diagnostics().is_empty());
    let inputs = [
        SourceUnitInput::new("root", "p/a_uses.ko", uses_id, &uses),
        SourceUnitInput::new("root", "p/z_models.ko", models_id, &models),
    ];
    let (environment, types) = environments(true);
    let mut previous = None;
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let index = index_compilation_unit(&sources, &inputs).expect("index");
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).expect("names");
        assert!(names.diagnostics().is_empty());
        let names = names.validate().expect("valid names");
        let signatures = lang_frontend::type_checking::collect_compilation_unit_signatures(
            &sources, &inputs, &names, &types,
        )
        .expect("signatures only");
        let typed =
            check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit types");
        assert_eq!(signatures.diagnostics(), typed.diagnostics());
        assert_eq!(codes(typed.diagnostics()), ["L0117"]);
        assert!(typed.body_diagnostics().is_empty());
        let diagnostic = &typed.diagnostics()[0];
        assert_eq!(diagnostic.primary_span().source_id(), uses_id);
        assert_eq!(
            sources.slice(diagnostic.primary_span()).expect("primary"),
            "Ref"
        );
        if let Some(previous) = &previous {
            assert_eq!(typed.diagnostics(), previous);
        }
        previous = Some(typed.diagnostics().to_vec());
        let index = names.names().index();
        let use_declaration = index
            .declarations()
            .iter()
            .find(|d| d.name() == "use")
            .expect("use declaration");
        let callable = signatures
            .declaration(use_declaration.id())
            .and_then(|d| d.callable())
            .expect("use callable");
        assert_eq!(callable.parameters().len(), 4);
        for parameter in &callable.parameters()[..3] {
            let Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Box,
                arguments,
            }) = signatures.types().get(parameter.ty())
            else {
                panic!("Box signature")
            };
            let [argument] = arguments.as_slice() else {
                panic!("one argument")
            };
            let Some(UnitTypeKind::Nominal { declaration, .. }) = signatures.types().get(*argument)
            else {
                panic!("nominal payload")
            };
            let source = index.source_units()[index.declarations()[declaration.index()]
                .source_unit()
                .index()]
            .source_id();
            assert_eq!(source, models_id);
        }
        assert_eq!(
            signatures.types().get(callable.parameters()[3].ty()),
            Some(&UnitTypeKind::Error)
        );
    }
}

#[test]
fn box_root_errors_keep_independent_sibling_type_argument_bounds() {
    for (bound, code) in [("Copyable", "L0115"), ("Marker", "L0093")] {
        for first_bound in ["", ": Copyable"] {
            for nullable in ["", "?"] {
                for interface in [false, true] {
                    let declaration = if interface {
                        format!("interface Pair<A{first_bound}, B: {bound}>")
                    } else {
                        format!("value class Pair<A{first_bound}, B: {bound}>(val a: A, val b: B)")
                    };
                    let payload = format!("Pair<Protocol, Ref>{nullable}");
                    let text = format!(
                        "interface Protocol\ninterface Marker\nclass Ref\n{declaration}\nfun use(arg: Box<{payload}>): Unit {{}}"
                    );
                    let (sources, parsed, entries) = checked_entries(&text, true);
                    for (names, typed) in entries {
                        let expected_codes = if interface {
                            vec!["L0094", "L0094", code]
                        } else {
                            vec!["L0094", code]
                        };
                        let expected_primaries = if interface {
                            vec![payload.as_str(), "Protocol", "Ref"]
                        } else {
                            vec!["Protocol", "Ref"]
                        };
                        assert_eq!(codes(typed.diagnostics()), expected_codes, "{text}");
                        assert_eq!(
                            typed
                                .diagnostics()
                                .iter()
                                .map(|d| sources.slice(d.primary_span()).expect("span"))
                                .collect::<Vec<_>>(),
                            expected_primaries
                        );
                        assert!(typed.body_diagnostics().is_empty());
                        let ty = typed
                            .symbol_type(symbol(&typed, &names, "arg"))
                            .expect("argument type");
                        assert_eq!(typed.types().get(ty), Some(&UnitTypeKind::Error));
                        let unit = names.source_units()[0].source_unit();
                        let pair_ref = parsed
                            .ast()
                            .type_refs()
                            .iter()
                            .find(|(_, node)| sources.slice(node.span()) == Ok(payload.as_str()))
                            .expect("original Pair type ref")
                            .0;
                        let mut pair = typed
                            .type_ref_type(lang_frontend::type_checking::UnitTypeRefId::new(
                                unit, pair_ref,
                            ))
                            .expect("recovery Pair");
                        if let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(pair) {
                            pair = *inner;
                        }
                        let Some(UnitTypeKind::Nominal { arguments, .. }) = typed.types().get(pair)
                        else {
                            panic!("nominal recovery retained")
                        };
                        assert_eq!(arguments.len(), 2);
                        assert_eq!(typed.types().get(arguments[0]), Some(&UnitTypeKind::Error));
                        assert!(matches!(
                            typed.types().get(arguments[1]),
                            Some(UnitTypeKind::Nominal { .. })
                        ));
                    }
                }
            }
        }
    }
}

#[test]
fn nested_nullable_boxes_keep_valid_nominal_payloads() {
    let mut payload = "Point".to_owned();
    for _ in 0..8 {
        payload = format!("Wrap<Box<{payload}>?>");
    }
    let text = format!(
        "value class Point(val n: Int)\nvalue class Wrap<T>(val inner: T)\nfun use(arg: Box<{payload}>): Unit {{}}"
    );
    let (sources, parsed, entries) = checked_entries(&text, true);
    assert_eq!(parsed.ast().type_refs().iter().filter(|(_, node)| matches!(node.payload(), lang_frontend::parser::TypeRef::Qualified { segments, .. } if segments.last().is_some_and(|segment| sources.slice(segment.name_span) == Ok("Box")))).count(), 9);
    for (names, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(copyability(&typed, &names, "arg"), Copyability::MoveOnly);
    }
}

#[test]
fn box_static_recovery_keeps_arity_shadowing_and_independent_interface_errors() {
    for (text, expected_codes, primaries) in [
        (
            "interface Protocol\nfun <T: Box<Protocol, Protocol>> use(): Unit {}",
            vec!["L0091"],
            vec!["Box"],
        ),
        (
            "interface Protocol\nclass Box<T>\nfun <T: Box<Protocol>> use(): Unit {}",
            vec!["L0092"],
            vec!["Box<Protocol>"],
        ),
        (
            "interface Protocol<T>\nfun use(arg: Box<Protocol<Box<Int>>>): Unit {}",
            vec!["L0094", "L0117"],
            vec!["Protocol<Box<Int>>", "Int"],
        ),
    ] {
        let (sources, _, entries) = checked_entries(text, true);
        for (_, typed) in entries {
            assert_eq!(codes(typed.diagnostics()), expected_codes, "{text}");
            assert_eq!(
                typed
                    .diagnostics()
                    .iter()
                    .map(|d| sources.slice(d.primary_span()).expect("span"))
                    .collect::<Vec<_>>(),
                primaries
            );
            assert!(typed.body_diagnostics().is_empty());
        }
    }
}
