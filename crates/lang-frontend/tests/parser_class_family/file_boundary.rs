use super::*;

#[test]
fn unfinished_file_class_header_preserves_the_next_root() {
    let next = "value class Sentinel(val y: Int)";
    for (head, following, expected) in [
        (
            "class C(val x: Int",
            "public class Sentinel",
            &["L0010"][..],
        ),
        (
            "class C(val x: Int,\npublic val y: Int",
            "class Sentinel",
            &["L0010"][..],
        ),
        ("interface C:", next, &["L0070"][..]),
        ("class C: I by", next, &["L0078"][..]),
        ("class", next, &["L0067"][..]),
        ("class C(val x: (Int) ->", next, &["L0010", "L0014"][..]),
        ("class C(val x: pkg.", next, &["L0010", "L0014"][..]),
        (
            "class C(val x: Int = 0",
            "class Sentinel",
            &["L0077", "L0010"][..],
        ),
    ] {
        let text = format!("{head}\n{following}");
        let (sources, parsed) = file(&text);
        let ast = parsed.ast().items();
        let [first, second] = parsed.roots() else {
            panic!("two file roots for {text}");
        };
        assert_eq!(
            sources.slice(ast.get(*first).unwrap().span()).unwrap(),
            head
        );
        let next = ast.get(*second).unwrap();
        assert!(matches!(
            next.payload(),
            Item::Classifier(_) | Item::Modified { .. }
        ));
        assert_eq!(sources.slice(next.span()).unwrap(), following);
        assert_eq!(codes(parsed.diagnostics()), expected, "{text}");
        for diagnostic in parsed
            .diagnostics()
            .iter()
            .filter(|d| d.code().to_string() != "L0077")
        {
            assert!(diagnostic.primary_span().is_empty());
            assert_eq!(diagnostic.primary_span().start(), next.span().start());
        }
    }
}

#[test]
fn class_file_boundary_preserves_ordinary_contextual_identifiers_and_fields() {
    for head in [
        "class value",
        "class\nvalue",
        "interface C<value>",
        "class C(val x: value,\npublic val y: Box<value>,\nprivate\nvar z: value)",
        "class C: I by\nvalue",
    ] {
        let text = format!("{head}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        let ast = parsed.ast().items();
        let [first, second] = parsed.roots() else {
            panic!("two roots for {text}")
        };
        assert_eq!(
            sources.slice(ast.get(*first).unwrap().span()).unwrap(),
            head
        );
        assert_eq!(
            sources.slice(ast.get(*second).unwrap().span()).unwrap(),
            "class Sentinel"
        );
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
    }
}

#[test]
fn contextual_value_class_uses_the_file_separator_rule_on_both_sides() {
    for (first, second) in [
        ("class C", "value class D(val x: Int)"),
        ("value class C(val x: Int)", "class D"),
    ] {
        let (sources, parsed) = file(&format!("{first} {second}"));
        assert_eq!(codes(parsed.diagnostics()), ["L0047"]);
        let ast = parsed.ast().items();
        let [left, right] = parsed.roots() else {
            panic!("two declarations");
        };
        assert_eq!(
            sources.slice(ast.get(*left).unwrap().span()).unwrap(),
            first
        );
        assert_eq!(
            sources.slice(ast.get(*right).unwrap().span()).unwrap(),
            second
        );
    }
}

#[test]
fn class_file_boundary_does_not_promote_a_nested_recovery_token() {
    for (head, expected) in [
        (
            "class C(val x: Int = f(\npublic class Hidden\n), val y: Int)",
            &["L0077"][..],
        ),
        (
            "class C(val x: Int = \"${public class Hidden}\", val y: Int)",
            &["L0077"][..],
        ),
    ] {
        let (sources, parsed) = file(&format!("{head}\nclass Sentinel"));
        let ast = parsed.ast().items();
        let [first, second] = parsed.roots() else {
            panic!("two roots for {head}")
        };
        assert_eq!(
            sources.slice(ast.get(*first).unwrap().span()).unwrap(),
            head
        );
        assert_eq!(
            sources.slice(ast.get(*second).unwrap().span()).unwrap(),
            "class Sentinel"
        );
        assert_eq!(codes(parsed.diagnostics()), expected);
    }
}

#[test]
fn balanced_header_keeps_illegal_nested_tokens_and_its_real_closer() {
    for head in [
        "class C(\npublic class Hidden(val y: Int)\n)",
        "class C(\n]\npublic class Hidden(val y: Int)\n)",
        "class C(val x: Int\npublic class Hidden(val y: Int)\n)",
        "class C(val x: Int;\n)",
    ] {
        let text = format!("{head}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        let ast = parsed.ast().items();
        let [first, sentinel] = parsed.roots() else {
            panic!("only two file roots for {text}")
        };
        assert_eq!(
            sources.slice(ast.get(*first).unwrap().span()).unwrap(),
            head
        );
        assert_eq!(
            sources.slice(ast.get(*sentinel).unwrap().span()).unwrap(),
            "class Sentinel"
        );
        assert!(!parsed.diagnostics().is_empty());
    }
}

#[test]
fn comparisons_after_an_unfinished_constructor_keep_their_file_owner() {
    let text = "class C(val x: Int\npublic class Sentinel\nval later = 2 > 1";
    let (sources, parsed) = file(text);
    assert_eq!(codes(parsed.diagnostics()), ["L0010"]);
    let ast = parsed.ast().items();
    let [class, sentinel, later] = parsed.roots() else {
        panic!("three file roots")
    };
    for (root, expected) in [
        (*class, "class C(val x: Int"),
        (*sentinel, "public class Sentinel"),
        (*later, "val later = 2 > 1"),
    ] {
        assert_eq!(
            sources.slice(ast.get(root).unwrap().span()).unwrap(),
            expected
        );
    }
}

#[test]
fn grouped_delegation_cannot_jump_over_an_enclosing_right_brace() {
    let text = "class C: I by (\n}\nclass Sentinel\n)";
    let (sources, parsed) = file(text);
    assert!(
        codes(parsed.diagnostics())
            .iter()
            .any(|code| code == "L0078")
    );
    assert!(parsed.roots().iter().any(|&root| {
        let item = parsed.ast().items().get(root).unwrap();
        matches!(item.payload(), Item::Classifier(_))
            && sources.slice(item.span()).unwrap() == "class Sentinel"
    }));
}

#[test]
fn grouped_delegation_preserves_later_lexical_terminal_recovery() {
    let text = "class C: I by (\"bad\n), J by \"also bad\n, K\nclass Sentinel";
    let (sources, parsed) = file(text);
    assert_eq!(codes(parsed.diagnostics()), ["L0078", "L0004", "L0004"]);
    let [first, second] = parsed.roots() else {
        panic!("class and sentinel")
    };
    let ast = parsed.ast().items();
    let Item::Classifier(class) = ast.get(*first).unwrap().payload() else {
        panic!("class")
    };
    let [i, j, k] = class.supertypes.as_slice() else {
        panic!("three supertypes")
    };
    for (entry, target) in [(i, "(\"bad\n)"), (j, "\"also bad")] {
        let NameMarker::Error(span) = entry.delegation.unwrap().target else {
            panic!("invalid target")
        };
        assert_eq!(sources.slice(span).unwrap(), target);
    }
    assert_eq!(sources.slice(k.span).unwrap(), "K");
    assert_eq!(
        sources.slice(ast.get(*second).unwrap().span()).unwrap(),
        "class Sentinel"
    );
}
