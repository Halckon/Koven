//! SPEC-0017 / SPEC-0125 class-family 的 Phase 1 AST、诊断与恢复契约测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    parser::{
        ClassifierDeclaration, ClassifierKind, Item, NameMarker, ParameterModeMarker,
        ParsedDeclaration, ParsedFile, VisibilityModifier,
    },
    source::{SourceId, SourceMap},
};

#[path = "parser_class_family/header_recovery.rs"]
mod header_recovery;

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{parse_declaration_twice, parse_expression_twice, parse_file_twice};

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources.add_source(name, text).expect("unique source")
}

fn declaration(text: &str) -> (SourceMap, ParsedDeclaration) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "class-family.ko", text);
    let parsed = parse_declaration_twice(&sources, source_id, text);
    (sources, parsed)
}

fn file(text: &str) -> (SourceMap, ParsedFile) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "class-family-file.ko", text);
    let parsed = parse_file_twice(&sources, source_id, text);
    (sources, parsed)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn item(parsed: &ParsedDeclaration, id: lang_frontend::ast::ItemId) -> &Item {
    parsed.ast().items().get(id).expect("item").payload()
}

fn classifier(parsed: &ParsedDeclaration) -> &ClassifierDeclaration {
    let root = item(parsed, parsed.root());
    let id = match root {
        Item::Modified { declaration, .. } => *declaration,
        Item::Classifier(classifier) => return classifier,
        other => panic!("expected classifier, got {other:?}"),
    };
    let Item::Classifier(classifier) = item(parsed, id) else {
        panic!("expected classifier")
    };
    classifier
}

fn receiver_mode(
    parsed: &ParsedDeclaration,
    id: lang_frontend::ast::ItemId,
) -> Option<ParameterModeMarker> {
    match item(parsed, id) {
        Item::Modified { modifiers, .. } => modifiers.receiver_mode,
        Item::Function { .. } => None,
        other => panic!("expected function member, got {other:?}"),
    }
}

fn receiver_mode_span(marker: ParameterModeMarker) -> lang_frontend::source::Span {
    match marker {
        ParameterModeMarker::Own(span)
        | ParameterModeMarker::Borrow(span)
        | ParameterModeMarker::Inout(span) => span,
    }
}

#[test]
fn parses_all_classifier_kinds_and_preserves_source_identity() {
    for (text, expected) in [
        ("value class Point(val x: Int)", "value"),
        ("class Node", "class"),
        ("interface Shape", "interface"),
        ("enum class State { Ready }", "enum"),
        ("object Config", "object"),
    ] {
        let (sources, parsed) = declaration(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text}: {:?}",
            parsed.diagnostics()
        );
        assert_eq!(parsed.source_id(), parsed.ast().source_id());
        let node = parsed.ast().items().get(parsed.root()).expect("root");
        assert_eq!(sources.slice(node.span()).expect("source"), text);
        let kind = &classifier(&parsed).kind;
        assert!(
            matches!(
                (expected, kind),
                ("value", ClassifierKind::ValueClass { .. })
                    | ("class", ClassifierKind::Class { .. })
                    | ("interface", ClassifierKind::Interface { .. })
                    | ("enum", ClassifierKind::EnumClass { .. })
                    | ("object", ClassifierKind::Object { .. })
            ),
            "{text}: {kind:?}"
        );
    }
}

#[test]
fn class_header_and_members_preserve_typed_children_and_modifiers() {
    let text = "public class Boxed<T: Copyable>(private val item: T, var count: Int): Printable, Resettable {\n    override fun reset(): Unit {}\n    private companion { const val VERSION: Int = 1; fun create(): Boxed<Int> = Boxed(1, 0) }\n}";
    let (sources, parsed) = declaration(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Item::Modified {
        modifiers,
        declaration: classifier_id,
    } = item(&parsed, parsed.root())
    else {
        panic!("modified classifier")
    };
    assert!(matches!(
        modifiers.visibility,
        Some(VisibilityModifier::Public(_))
    ));
    let ClassifierDeclaration {
        name,
        type_parameters,
        primary_constructor: Some(constructor),
        supertypes,
        body: Some(body),
        ..
    } = classifier(&parsed)
    else {
        panic!("classifier payload")
    };
    assert!(matches!(name, NameMarker::Present(_)));
    assert_eq!(type_parameters.len(), 1);
    assert_eq!(constructor.fields.len(), 2);
    assert!(matches!(
        constructor.fields[0].visibility,
        Some(VisibilityModifier::Private(_))
    ));
    assert_eq!(supertypes.len(), 2);
    assert_eq!(body.members.len(), 2);
    assert_eq!(
        sources
            .slice(parsed.ast().items().get(*classifier_id).unwrap().span())
            .unwrap(),
        &text[7..]
    );
    assert!(matches!(
        item(&parsed, body.members[0]),
        Item::Modified { .. }
    ));
    assert!(matches!(
        item(&parsed, body.members[1]),
        Item::Modified { .. }
    ));
}

#[test]
fn instance_receiver_modes_preserve_marker_kind_and_source_span_for_every_owner() {
    for text in [
        "class C { fun plain(): Unit {}; borrow fun read(): Unit {}; inout fun write(): Unit {}; own fun take(): Unit {} }",
        "value class V(val raw: Int) { fun plain(): Unit {}; borrow fun read(): Unit {}; inout fun write(): Unit {}; own fun take(): Unit {} }",
        "interface I { fun plain(): Unit {}; borrow fun read(): Unit {}; inout fun write(): Unit {}; own fun take(): Unit {} }",
        "enum class E { A; fun plain(): Unit {}; borrow fun read(): Unit {}; inout fun write(): Unit {}; own fun take(): Unit {} }",
        "object O { fun plain(): Unit {}; borrow fun read(): Unit {}; inout fun write(): Unit {}; own fun take(): Unit {} }",
    ] {
        let (sources, parsed) = declaration(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text}: {:?}",
            parsed.diagnostics()
        );
        let members = &classifier(&parsed).body.as_ref().expect("body").members;
        assert_eq!(members.len(), 4, "{text}");
        assert_eq!(receiver_mode(&parsed, members[0]), None, "{text}");
        for (id, expected) in members[1..].iter().zip(["borrow", "inout", "own"]) {
            let marker = receiver_mode(&parsed, *id).expect("explicit receiver mode");
            assert_eq!(
                sources
                    .slice(receiver_mode_span(marker))
                    .expect("receiver span"),
                expected,
                "{text}"
            );
        }
        assert!(matches!(
            receiver_mode(&parsed, members[1]),
            Some(ParameterModeMarker::Borrow(_))
        ));
        assert!(matches!(
            receiver_mode(&parsed, members[2]),
            Some(ParameterModeMarker::Inout(_))
        ));
        assert!(matches!(
            receiver_mode(&parsed, members[3]),
            Some(ParameterModeMarker::Own(_))
        ));
    }
}

#[test]
fn receiver_mode_follows_visibility_and_override_in_the_shared_modifier_wrapper() {
    let text = "class C { public override own fun consume(): Unit {} }";
    let (sources, parsed) = declaration(text);
    assert!(parsed.diagnostics().is_empty());
    let member = classifier(&parsed).body.as_ref().expect("body").members[0];
    let Item::Modified { modifiers, .. } = item(&parsed, member) else {
        panic!("modified member")
    };
    assert!(matches!(
        modifiers.visibility,
        Some(VisibilityModifier::Public(_))
    ));
    assert!(modifiers.override_span.is_some());
    let marker = modifiers.receiver_mode.expect("receiver mode");
    assert!(matches!(marker, ParameterModeMarker::Own(_)));
    assert_eq!(
        sources
            .slice(receiver_mode_span(marker))
            .expect("receiver span"),
        "own"
    );
}

#[test]
fn receiver_mode_rejects_duplicates_ordering_and_non_instance_positions() {
    let cases = [
        ("class C { borrow borrow fun f(): Unit {} }", "borrow"),
        ("class C { borrow public fun f(): Unit {} }", "public"),
        ("class C { borrow override fun f(): Unit {} }", "override"),
        ("borrow fun top(): Unit {}", "borrow"),
        ("borrow class C", "borrow"),
        (
            "class C { companion { borrow fun f(): Unit {} } }",
            "borrow",
        ),
        ("object O { borrow const val X: Int = 1 }", "borrow"),
        ("class C { borrow companion {} }", "borrow"),
        (
            "interface I { internal borrow fun f(): Unit {} }",
            "internal",
        ),
    ];
    for (text, expected_primary) in cases {
        let (sources, parsed) = declaration(text);
        let diagnostic = parsed
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code().to_string() == "L0076")
            .unwrap_or_else(|| panic!("{text}: {:?}", parsed.diagnostics()));
        assert_eq!(
            sources
                .slice(diagnostic.primary_span())
                .expect("modifier primary"),
            expected_primary,
            "{text}: {diagnostic:?}"
        );
    }

    let (_, top_level) = declaration("borrow fun top(): Unit {}");
    assert!(matches!(
        item(&top_level, top_level.root()),
        Item::Function { .. }
    ));

    let (_, object) = declaration("object O { borrow const val X: Int = 1 }");
    let constant = classifier(&object).body.as_ref().expect("body").members[0];
    assert!(matches!(item(&object, constant), Item::Constant { .. }));
}

#[test]
fn malformed_receiver_member_recovers_before_later_member_and_root() {
    let text = concat!(
        "class C { borrow @\n",
        "fun next(): Unit {} }\n",
        "class After",
    );
    let (_, parsed) = file(text);
    assert_eq!(parsed.roots().len(), 2, "{:?}", parsed.diagnostics());
    let Item::Classifier(first) = parsed
        .ast()
        .items()
        .get(parsed.roots()[0])
        .expect("first root")
        .payload()
    else {
        panic!("classifier")
    };
    let members = &first.body.as_ref().expect("body").members;
    assert_eq!(members.len(), 2, "{:?}", parsed.diagnostics());
    assert!(matches!(
        parsed
            .ast()
            .items()
            .get(members[1])
            .expect("next")
            .payload(),
        Item::Function { .. }
    ));
    assert!(codes(parsed.diagnostics()).contains(&"L0076".to_owned()));
    assert!(codes(parsed.diagnostics()).contains(&"L0071".to_owned()));
}

#[test]
fn receiver_without_fun_does_not_attach_to_the_next_member() {
    let text = concat!(
        "class C { borrow val state: Int = 1\n",
        "fun next(): Unit {} }",
    );
    let (_, parsed) = declaration(text);
    let members = &classifier(&parsed).body.as_ref().expect("body").members;
    assert_eq!(members.len(), 2, "{:?}", parsed.diagnostics());
    assert!(matches!(item(&parsed, members[0]), Item::Variable { .. }));
    assert!(matches!(item(&parsed, members[1]), Item::Function { .. }));
    assert!(codes(parsed.diagnostics()).contains(&"L0076".to_owned()));
    assert!(codes(parsed.diagnostics()).contains(&"L0077".to_owned()));
}

#[test]
fn receiver_after_fun_is_rejected_without_becoming_a_name_or_sibling_member() {
    for mode in ["borrow", "inout", "own"] {
        let text =
            format!("class C {{ fun {mode} misplaced(): Unit {{}}; fun next(): Unit {{}} }}");
        let (sources, parsed) = declaration(&text);
        assert_eq!(codes(parsed.diagnostics()), ["L0076"], "{text}");
        assert_eq!(
            sources
                .slice(parsed.diagnostics()[0].primary_span())
                .expect("misplaced receiver"),
            mode
        );
        let members = &classifier(&parsed).body.as_ref().expect("body").members;
        assert_eq!(members.len(), 2, "{text}");
        assert!(
            members
                .iter()
                .all(|member| matches!(item(&parsed, *member), Item::Function { .. }))
        );
    }

    let (sources, top_level) = declaration("fun own top(): Unit {}");
    assert_eq!(codes(top_level.diagnostics()), ["L0076"]);
    assert_eq!(
        sources
            .slice(top_level.diagnostics()[0].primary_span())
            .expect("top-level misplaced receiver"),
        "own"
    );
    assert!(matches!(
        item(&top_level, top_level.root()),
        Item::Function { .. }
    ));
}

#[test]
fn enum_uses_commas_and_semicolon_before_shared_members() {
    let text = "enum class Shape { Circle(radius: Double), Rectangle(w: Double, h: Double), Point; fun area(): Double = 0.0; companion { const val COUNT: Int = 3 } }";
    let (_, parsed) = declaration(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let body = classifier(&parsed).body.as_ref().expect("enum body");
    assert_eq!(body.variants.len(), 3);
    assert_eq!(body.variants[0].parameters.len(), 1);
    assert_eq!(body.variants[1].parameters.len(), 2);
    assert!(body.enum_member_delimiter_span.is_some());
    assert_eq!(body.members.len(), 2);
}

#[test]
fn nested_and_anonymous_forms_are_directionally_rejected() {
    let (_, nested) = declaration("class Outer { class Inner }");
    assert_eq!(codes(nested.diagnostics()), ["L0077"]);

    let (_, anonymous) = file("val x = object { fun run() {} }\nclass Next");
    assert!(codes(anonymous.diagnostics()).contains(&"L0009".to_owned()));
    assert_eq!(anonymous.roots().len(), 3);
}

#[test]
fn malformed_member_recovers_to_the_next_member_and_top_level_declaration() {
    let text = "class Broken { fun first() {} @@@\nfun second() {} }\nclass Next";
    let (_, parsed) = file(text);
    assert_eq!(parsed.roots().len(), 2, "{:?}", parsed.diagnostics());
    let first = parsed.ast().items().get(parsed.roots()[0]).expect("first");
    let Item::Classifier(first_classifier) = first.payload() else {
        panic!("first classifier")
    };
    let body = first_classifier.body.as_ref().expect("body");
    assert_eq!(body.members.len(), 2);
    assert!(matches!(
        parsed.ast().items().get(body.members[1]).unwrap().payload(),
        Item::Function { .. }
    ));
    assert!(codes(parsed.diagnostics()).contains(&"L0072".to_owned()));
}

#[test]
fn l0066_through_l0077_have_stable_first_primary_spans() {
    let cases = [
        ("value Thing(val x: Int)", "L0066", "Thing"),
        ("class {}", "L0067", ""),
        ("value class Empty()", "L0068", ")"),
        ("class Pair(val first: Int val second: Int)", "L0069", "val"),
        ("class Child: {}", "L0070", ""),
        ("class Broken { @ }", "L0071", "@"),
        ("class C { fun a() {} fun b() {} }", "L0072", "fun"),
        ("enum class Empty { }", "L0073", "}"),
        ("enum class E { A B }", "L0074", "B"),
        ("enum class E { A fun f() {} }", "L0075", "fun"),
        ("public private class C", "L0076", "private"),
        ("class C(val item: Int = 1)", "L0077", "="),
    ];
    for (text, expected_code, expected_primary) in cases {
        let (sources, parsed) = declaration(text);
        let diagnostic = parsed
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code().to_string() == expected_code)
            .unwrap_or_else(|| panic!("{text:?}: {:?}", parsed.diagnostics()));
        assert_eq!(
            sources.slice(diagnostic.primary_span()).expect("primary"),
            expected_primary,
            "{text:?}: {diagnostic:?}"
        );
    }
}

#[test]
fn segmented_string_classifier_name_recovers_the_complete_owner() {
    let text = r#"class "text" {}"#;
    let (sources, parsed) = declaration(text);
    assert_eq!(codes(parsed.diagnostics()), ["L0067"]);

    let NameMarker::Error(span) = classifier(&parsed).name else {
        panic!("expected recovered classifier name")
    };
    assert_eq!(
        sources.slice(span).expect("name recovery span"),
        r#""text""#
    );
}

#[test]
fn interface_object_and_companion_keep_the_v1_static_member_boundary() {
    for text in [
        "interface Protocol { fun send(item: Int): Unit; public fun version(): Int = 1; companion { const val VERSION: Int = 1 } }",
        "object Config: Protocol { const val VERSION: Int = 1; override fun send(item: Int): Unit {}; fun describe(): String = \"config\" }",
        "class Factory { companion { const val VERSION: Int = 1; fun create(): Factory = Factory() } }",
    ] {
        let (_, parsed) = declaration(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text}: {:?}",
            parsed.diagnostics()
        );
        let body = classifier(&parsed).body.as_ref().expect("classifier body");
        assert!(!body.members.is_empty());
    }
}

#[test]
fn unsupported_kotlin_class_forms_do_not_expand_the_v1_grammar() {
    for text in [
        "interface I(val item: Int)",
        "class C: Base() {}",
        "class C(val item: Int,)",
        "class C { const val X: Int = 1 }",
        "object O { val state: Int = 1 }",
        "class C { companion object Named {} }",
        "class Outer { object Inner }",
        "enum class E { A; }",
        "class C(borrow val item: Int)",
        "class C(own val item: Int)",
        "enum class E { A(val item: Int) }",
        "enum class E { A(own item: Int) }",
        "nocopy value class Id(val raw: Int)",
        "class C { constructor() }",
        "class C { init {} }",
    ] {
        let (_, parsed) = declaration(text);
        assert!(
            codes(parsed.diagnostics()).contains(&"L0077".to_owned()),
            "{text}: {:?}",
            parsed.diagnostics()
        );
    }

    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "anonymous.ko", "object { fun run() {} }");
    let parsed = parse_expression_twice(&sources, source_id, "anonymous object expression");
    assert_eq!(codes(parsed.diagnostics())[0], "L0009");
}

#[test]
fn field_supertype_and_variant_recovery_preserves_later_children() {
    let (_, fields) = declaration("class C(val first: Int @@@, val second: Int)");
    let constructor = classifier(&fields)
        .primary_constructor
        .as_ref()
        .expect("constructor");
    assert_eq!(constructor.fields.len(), 2);
    assert!(codes(fields.diagnostics()).contains(&"L0069".to_owned()));

    let (_, supertypes) = declaration("class C: @@@, Good {}");
    let list = &classifier(&supertypes).supertypes;
    assert_eq!(list.len(), 1);
    assert!(codes(supertypes.diagnostics()).contains(&"L0070".to_owned()));

    let (_, variants) = declaration("enum class E { First @@@, Second }");
    let body = classifier(&variants).body.as_ref().expect("enum");
    assert_eq!(body.variants.len(), 2);
    assert!(codes(variants.diagnostics()).contains(&"L0074".to_owned()));
}

#[test]
fn member_recovery_respects_lexical_owners_and_outer_hard_closers() {
    let (_, lexical) = file("class C { @ \"bad\nfun next() {} }\nclass After");
    assert_eq!(lexical.roots().len(), 2);
    let Item::Classifier(classifier) = lexical
        .ast()
        .items()
        .get(lexical.roots()[0])
        .expect("class")
        .payload()
    else {
        panic!("classifier")
    };
    assert_eq!(classifier.body.as_ref().expect("body").members.len(), 2);
    assert_eq!(
        codes(lexical.diagnostics()),
        ["L0071".to_owned(), "L0004".to_owned()]
    );

    let (_, closer) = file("class C { @ [ item }\nclass After");
    assert_eq!(closer.roots().len(), 2, "{:?}", closer.diagnostics());
    assert!(codes(closer.diagnostics()).contains(&"L0071".to_owned()));
}

#[test]
fn enum_members_do_not_satisfy_the_required_first_variant() {
    let text = "enum class E { ; fun member(): Unit {} }";
    let (sources, parsed) = declaration(text);
    assert_eq!(codes(parsed.diagnostics()), ["L0073"]);
    let body = classifier(&parsed).body.as_ref().unwrap();
    assert!(body.variants.is_empty());
    assert_eq!(body.members.len(), 1);
    let delimiter = body.enum_member_delimiter_span.unwrap();
    assert_eq!(parsed.diagnostics()[0].primary_span(), delimiter);
    assert_eq!(sources.slice(delimiter).unwrap(), ";");
    assert_eq!(sources.slice(body.right_brace_span.unwrap()).unwrap(), "}");
}

#[test]
fn missing_enum_payload_closer_preserves_member_delimiter_and_file_owner() {
    let text = "enum class E { A(x: Int; fun member(): Unit {} }\nclass Sentinel";
    let (sources, parsed) = file(text);
    assert_eq!(codes(parsed.diagnostics()), ["L0010"]);
    let [enumeration, sentinel] = parsed.roots() else {
        panic!("enum and following classifier");
    };
    let Item::Classifier(enumeration) = parsed.ast().items().get(*enumeration).unwrap().payload()
    else {
        panic!("recovered enum");
    };
    let body = enumeration.body.as_ref().unwrap();
    let [variant] = body.variants.as_slice() else {
        panic!("recovered variant");
    };
    assert_eq!(variant.parameters.len(), 1);
    assert!(variant.right_paren_span.is_none());
    let delimiter = body.enum_member_delimiter_span.unwrap();
    let primary = parsed.diagnostics()[0].primary_span();
    assert!(primary.is_empty());
    assert_eq!(primary.start(), delimiter.start());
    assert_eq!(sources.slice(delimiter).unwrap(), ";");
    let [member] = body.members.as_slice() else {
        panic!("enum member");
    };
    let member = parsed.ast().items().get(*member).unwrap();
    assert_eq!(
        sources.slice(member.span()).unwrap(),
        "fun member(): Unit {}"
    );
    assert_eq!(sources.slice(body.right_brace_span.unwrap()).unwrap(), "}");
    let sentinel = parsed.ast().items().get(*sentinel).unwrap();
    assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
}

#[test]
fn enum_payload_keeps_its_outer_delimiter_during_nested_type_recovery() {
    for payload in ["T<", "("] {
        let text = format!("enum class E {{ A(x: {payload}; fun member(): Unit {{}} }}");
        let (sources, parsed) = declaration(&text);
        let body = classifier(&parsed).body.as_ref().unwrap();
        let delimiter = body.enum_member_delimiter_span.unwrap();
        assert_eq!(sources.slice(delimiter).unwrap(), ";");
        assert!(body.variants[0].right_paren_span.is_none());
        assert!(body.right_brace_span.is_some());
        let [member] = body.members.as_slice() else {
            panic!("enum member")
        };
        let member = parsed.ast().items().get(*member).unwrap();
        assert_eq!(
            sources.slice(member.span()).unwrap(),
            "fun member(): Unit {}"
        );
        assert!(parsed.diagnostics().iter().all(|diagnostic| {
            let span = diagnostic.primary_span();
            span.is_empty() && span.start() == delimiter.start()
        }));
        assert!(codes(parsed.diagnostics()).contains(&"L0010".to_owned()));
    }
}

#[test]
fn recovered_enum_variant_spans_end_at_the_last_real_token() {
    for (payload, variant_text) in [("(", "A("), ("(x:   ", "A(x:")] {
        let text = format!("enum class E {{ A{payload}}}");
        let (sources, parsed) = declaration(&text);
        let body = classifier(&parsed).body.as_ref().unwrap();
        let [variant] = body.variants.as_slice() else {
            panic!("recovered enum variant")
        };
        assert_eq!(sources.slice(variant.span).unwrap(), variant_text);
        assert!(variant.right_paren_span.is_none());
        if let [parameter] = variant.parameters.as_slice() {
            assert_eq!(sources.slice(parameter.span).unwrap(), "x:");
        }
        assert!(body.right_brace_span.is_some());
    }
}

#[test]
fn missing_companion_constant_keeps_the_member_separator_in_both_entries() {
    for (constant, expected) in [
        ("const val A ->", &["L0020"][..]),
        ("const", &["L0018", "L0020", "L0022"][..]),
        ("const val", &["L0018", "L0020"][..]),
    ] {
        let text = format!("class C {{ companion {{ {constant}   ; fun member(): Unit {{}} }} }}");
        let (declaration_sources, declaration) = declaration(&text);
        let (file_sources, file) = file(&text);
        let [file_root] = file.roots() else {
            panic!("one file declaration")
        };
        for (sources, ast, root, diagnostics) in [
            (
                &declaration_sources,
                declaration.ast(),
                declaration.root(),
                declaration.diagnostics(),
            ),
            (&file_sources, file.ast(), *file_root, file.diagnostics()),
        ] {
            let Item::Classifier(class) = ast.items().get(root).unwrap().payload() else {
                panic!("class")
            };
            let outer = class.body.as_ref().unwrap();
            let Item::Companion(companion) = ast.items().get(outer.members[0]).unwrap().payload()
            else {
                panic!("companion")
            };
            let [recovered, member] = companion.body.members.as_slice() else {
                panic!("constant and following companion method")
            };
            let recovered = ast.items().get(*recovered).unwrap();
            assert_eq!(sources.slice(recovered.span()).unwrap(), constant);
            let member = ast.items().get(*member).unwrap();
            assert_eq!(
                sources.slice(member.span()).unwrap(),
                "fun member(): Unit {}"
            );
            assert!(companion.body.right_brace_span.is_some());
            assert!(outer.right_brace_span.is_some());
            assert_eq!(codes(diagnostics), expected);
        }
    }
}

#[test]
fn lexer_poison_in_a_constructor_or_enum_parameter_list_is_reported_once() {
    for text in [
        "class C(val x: Int async)",
        "class C(val x: Int async, val y: Int)",
        "enum class E { A(x: Int async) }",
        "enum class E { A(x: Int async, y: Int) }",
    ] {
        let (sources, parsed) = declaration(text);
        assert_eq!(codes(parsed.diagnostics()), ["L0002"], "{text}");
        assert_eq!(
            sources
                .slice(parsed.diagnostics()[0].primary_span())
                .unwrap(),
            "async"
        );
        assert_eq!(
            sources
                .slice(parsed.ast().items().get(parsed.root()).unwrap().span())
                .unwrap(),
            text
        );
    }
}

#[test]
fn a_classifier_cannot_declare_two_companions() {
    let text = "class C { companion {}; companion {} }";
    let (sources, parsed) = declaration(text);
    assert_eq!(codes(parsed.diagnostics()), ["L0077"]);
    let body = classifier(&parsed).body.as_ref().unwrap();
    assert_eq!(body.members.len(), 2);
    assert_eq!(
        sources
            .slice(parsed.diagnostics()[0].primary_span())
            .unwrap(),
        "companion"
    );
    assert_eq!(
        parsed.diagnostics()[0].primary_span().start(),
        text.rfind("companion").unwrap()
    );
    assert_eq!(sources.slice(body.right_brace_span.unwrap()).unwrap(), "}");
}

#[test]
fn missing_companion_constant_keeps_both_closers_and_following_members() {
    let text = "class C { companion { const }; fun member(): Unit {} }\nclass Sentinel";
    let (sources, parsed) = file(text);
    assert_eq!(codes(parsed.diagnostics()), ["L0018", "L0020", "L0022"]);
    let [class, sentinel] = parsed.roots() else {
        panic!("class and following classifier");
    };
    let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
        panic!("recovered class");
    };
    let body = class.body.as_ref().unwrap();
    let [companion, member] = body.members.as_slice() else {
        panic!("companion and ordinary method");
    };
    let Item::Companion(companion) = parsed.ast().items().get(*companion).unwrap().payload() else {
        panic!("recovered companion");
    };
    assert_eq!(companion.body.members.len(), 1);
    let inner_closer = companion.body.right_brace_span.unwrap();
    let outer_closer = body.right_brace_span.unwrap();
    assert_eq!(inner_closer.start(), text.find("};").unwrap());
    assert_eq!(outer_closer.start(), text.rfind('}').unwrap());
    assert!(parsed.diagnostics().iter().all(|diagnostic| {
        let span = diagnostic.primary_span();
        span.is_empty() && span.start() == inner_closer.start()
    }));
    let member = parsed.ast().items().get(*member).unwrap();
    assert_eq!(
        sources.slice(member.span()).unwrap(),
        "fun member(): Unit {}"
    );
    let sentinel = parsed.ast().items().get(*sentinel).unwrap();
    assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
}

#[test]
fn malformed_companion_constant_preserves_string_owner_and_both_closers() {
    let text = r#"interface Printable { fun show(): Unit; companion { const "printable" } }"#;
    let (_, parsed) = declaration(text);
    assert_eq!(
        codes(parsed.diagnostics()),
        ["L0018".to_owned(), "L0022".to_owned(), "L0020".to_owned()]
    );

    let root = parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("interface root");
    assert_eq!(root.span().end(), text.len());
    let outer_body = classifier(&parsed).body.as_ref().expect("interface body");
    assert!(outer_body.right_brace_span.is_some());
    assert_eq!(outer_body.members.len(), 2);
    let Item::Companion(companion) = item(&parsed, outer_body.members[1]) else {
        panic!("expected recovered companion")
    };
    assert!(companion.body.right_brace_span.is_some());
    assert_eq!(companion.body.members.len(), 1);
}

#[test]
fn companion_block_is_the_only_companion_form() {
    let text = "class Point { companion { const val ORIGIN: Int = 0 } }";
    let (_, parsed) = declaration(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let body = classifier(&parsed).body.as_ref().expect("class body");
    let Item::Companion(companion) = item(&parsed, body.members[0]) else {
        panic!("expected companion block")
    };
    assert_eq!(
        &text[companion.companion_span.start()..companion.companion_span.end()],
        "companion"
    );
    assert!(companion.object_span.is_empty());
    assert_eq!(
        companion.object_span.start(),
        text.find("{ const").expect("body")
    );
    assert_eq!(companion.body.members.len(), 1);

    let retired = "class Point { companion object { const val ORIGIN: Int = 0 } }";
    let (_, parsed) = declaration(retired);
    assert_eq!(codes(parsed.diagnostics()), ["L0077".to_owned()]);
    let diagnostic = &parsed.diagnostics()[0];
    let object = retired.find("object").expect("object keyword");
    assert_eq!(diagnostic.primary_span().start(), object);
    assert_eq!(diagnostic.primary_span().end(), object + "object".len());
    assert_eq!(
        diagnostic.message(),
        "write `companion { ... }`; `companion object` is not Koven syntax"
    );
    let body = classifier(&parsed).body.as_ref().expect("class body");
    let Item::Companion(companion) = item(&parsed, body.members[0]) else {
        panic!("expected recovered companion")
    };
    assert_eq!(companion.body.members.len(), 1, "body is still recovered");
}

#[test]
fn phase_one_aggregate_guide_example_now_parses_as_one_file() {
    let text = r#"value class Point(val x: Int, val y: Int)

enum class Shape {
    Circle(radius: Double),
    Point;

    fun area(): Double = when (this) {
        is Circle -> 3.14159 * radius * radius
        is Point -> 0.0
    }
}

fun main(): Unit {
    val points: List<Point> = listOf(Point(x = 1, y = 2), Point(x = 3, y = 4))
    val first = points[0]
    val double: (Int) -> Int = { x -> x * 2 }
    println(double(first.x))
}
"#;
    let (_, parsed) = file(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.roots().len(), 3);
}

#[test]
fn parses_class_deinit_member() {
    let text = "class FileGuard(val fd: Int) {\n    deinit() {\n        close(fd)\n    }\n}";
    let (_sources, parsed) = declaration(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let cls = classifier(&parsed);
    let body = cls.body.as_ref().expect("body");
    assert_eq!(body.members.len(), 1);
    let member = item(&parsed, body.members[0]);
    assert!(matches!(member, Item::Deinit { .. }));
}

#[test]
fn rejects_deinit_in_value_class_and_interface() {
    let val_text = "value class Point(val x: Int) {\n    deinit() {}\n}";
    let (_, parsed) = declaration(val_text);
    assert_eq!(codes(parsed.diagnostics()), ["L0077"]);

    let iface_text = "interface Resource {\n    deinit() {}\n}";
    let (_, parsed) = declaration(iface_text);
    assert_eq!(codes(parsed.diagnostics()), ["L0077"]);
}

#[test]
fn rejects_deinit_with_modifiers() {
    let text = "class Resource {\n    public deinit() {}\n}";
    let (_, parsed) = declaration(text);
    assert!(codes(parsed.diagnostics()).contains(&"L0076".to_string()));
}

#[test]
fn rejects_deinit_with_parameters() {
    let text = "class Resource {\n    deinit(x: Int) {}\n}";
    let (_, parsed) = declaration(text);
    assert!(codes(parsed.diagnostics()).contains(&"L0077".to_string()));
}
