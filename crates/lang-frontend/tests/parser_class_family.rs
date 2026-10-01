//! SPEC-0017 / SPEC-0125 class-family 的 Phase 1 AST、诊断与恢复契约测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    parser::{
        ClassifierDeclaration, ClassifierKind, Item, NameMarker, ParameterModeMarker,
        ParsedDeclaration, ParsedFile, VisibilityModifier,
    },
    source::{SourceId, SourceMap},
};

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
    let text = "public class Boxed<T: Copyable>(private val item: T, var count: Int): Printable, Resettable {\n    override fun reset(): Unit {}\n    private companion object { const val VERSION: Int = 1; fun create(): Boxed<Int> = Boxed(1, 0) }\n}";
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
            "class C { companion object { borrow fun f(): Unit {} } }",
            "borrow",
        ),
        ("object O { borrow const val X: Int = 1 }", "borrow"),
        ("class C { borrow companion object {} }", "borrow"),
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
    let text = "enum class Shape { Circle(radius: Double), Rectangle(w: Double, h: Double), Point; fun area(): Double = 0.0; companion object { const val COUNT: Int = 3 } }";
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
        "interface Protocol { fun send(item: Int): Unit; public fun version(): Int = 1; companion object { const val VERSION: Int = 1 } }",
        "object Config: Protocol { const val VERSION: Int = 1; override fun send(item: Int): Unit {}; fun describe(): String = \"config\" }",
        "class Factory { companion object { const val VERSION: Int = 1; fun create(): Factory = Factory() } }",
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
fn malformed_companion_constant_preserves_string_owner_and_both_closers() {
    let text =
        r#"interface Printable { fun show(): Unit; companion object { const "printable" } }"#;
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
