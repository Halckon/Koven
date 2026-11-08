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

#[test]
fn file_type_parameters_preserve_an_explicit_separator_before_the_next_declaration() {
    let comparison = "public val sentinel = 2 > 1";
    for (head, following, closing_count, missing_type) in [
        ("interface C<T", comparison, 1, false),
        ("interface C<T break", comparison, 1, false),
        ("interface C<T: Box<Int", comparison, 2, false),
        ("interface C<T: Box<", comparison, 2, true),
        ("interface C<T, U:", comparison, 1, true),
        (
            "interface C<T,",
            "value\nclass Sentinel(val y: Int)",
            1,
            false,
        ),
    ] {
        let text = format!("{head}; {following}");
        let (sources, parsed) = file(&text);
        let ast = parsed.ast().items();
        let [classifier, sentinel] = parsed.roots() else {
            panic!("two file roots for {text}: {:?}", parsed.roots());
        };
        assert_eq!(
            sources.slice(ast.get(*classifier).unwrap().span()).unwrap(),
            head
        );
        assert_eq!(
            sources.slice(ast.get(*sentinel).unwrap().span()).unwrap(),
            following
        );
        let diagnostics = parsed.diagnostics();
        for (code, count) in [
            ("L0010", closing_count),
            ("L0014", usize::from(missing_type)),
        ] {
            let at_boundary: Vec<_> = diagnostics
                .iter()
                .filter(|diag| diag.code().to_string() == code)
                .collect();
            assert_eq!(at_boundary.len(), count, "{text}: {diagnostics:?}");
            for diagnostic in at_boundary {
                assert!(diagnostic.primary_span().is_empty());
                assert_eq!(diagnostic.primary_span().start(), head.len());
            }
        }
    }
}

#[test]
fn type_parameter_recovery_keeps_a_real_closer_and_nested_separators() {
    for head in [
        "interface C<T;>",
        "interface C<T break f(; public val hidden = 2)>",
        "interface C<T break \"${x; public val hidden = 2}\">",
        "interface C<T: (Int; public val hidden = 2) -> Int>",
        "interface C<T: [; public val hidden = 2]>",
    ] {
        let text = format!("{head}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        let ast = parsed.ast().items();
        let [classifier, sentinel] = parsed.roots() else {
            panic!("two file roots for {text}: {:?}", parsed.roots());
        };
        assert_eq!(
            sources.slice(ast.get(*classifier).unwrap().span()).unwrap(),
            head
        );
        assert_eq!(
            sources.slice(ast.get(*sentinel).unwrap().span()).unwrap(),
            "class Sentinel"
        );
    }
}

#[test]
fn unclosed_type_bound_groups_still_own_their_semicolon() {
    for head in [
        "interface C<T: [",
        "interface C<T: (Int",
        "interface C<T break [",
    ] {
        let text = format!("{head}; public val hidden = 2");
        let (sources, parsed) = file(&text);
        let [root] = parsed.roots() else {
            panic!("only the classifier owns {text}");
        };
        assert_eq!(
            sources
                .slice(parsed.ast().items().get(*root).unwrap().span())
                .unwrap(),
            text
        );
    }
}

#[test]
fn a_group_opened_by_the_previous_root_does_not_own_the_type_parameter_separator() {
    let (sources, parsed) = file("class C(val x: Int\ninterface I<T; val sentinel = 2 > 1");
    let ast = parsed.ast().items();
    let [first, second, third] = parsed.roots() else {
        panic!("constructor, interface, and following declaration");
    };
    for (id, text) in [
        (*first, "class C(val x: Int"),
        (*second, "interface I<T"),
        (*third, "val sentinel = 2 > 1"),
    ] {
        assert_eq!(sources.slice(ast.get(id).unwrap().span()).unwrap(), text);
    }
}

#[test]
fn a_mismatched_closer_in_the_type_bound_cannot_match_a_previous_root() {
    let head = "interface I<T: (Int]; val hidden = 2";
    for prefix in ["", "val x: [ = 1\n"] {
        let text = format!("{prefix}{head}");
        let (sources, parsed) = file(&text);
        assert_eq!(
            parsed.roots().len(),
            1 + usize::from(!prefix.is_empty()),
            "{text}"
        );
        let node = parsed
            .ast()
            .items()
            .get(*parsed.roots().last().unwrap())
            .unwrap();
        assert_eq!(sources.slice(node.span()).unwrap(), head);
    }
}

#[test]
fn a_function_bound_closer_releases_its_group_after_an_invalid_parameter() {
    let head = "interface I<T: (Int, [)";
    let following = "val sentinel = 2";
    let (sources, parsed) = file(&format!("{head}; {following}"));
    let items = parsed.ast().items();
    let [class, sentinel] = parsed.roots() else {
        panic!("the real right paren ends the function bound's group");
    };
    assert_eq!(
        sources.slice(items.get(*class).unwrap().span()).unwrap(),
        head
    );
    assert_eq!(
        sources.slice(items.get(*sentinel).unwrap().span()).unwrap(),
        following
    );
}

#[test]
fn terminal_string_events_do_not_leak_into_or_out_of_the_type_parameter_owner() {
    let text = concat!(
        "val first = \"bad\n",
        "interface C<T break \"also bad\n",
        "; val sentinel = 2 > 1",
    );
    let (sources, parsed) = file(text);
    let items = parsed.ast().items();
    let [first, classifier, sentinel] = parsed.roots() else {
        panic!("first declaration, interface, and sentinel");
    };
    for (id, expected) in [
        (*first, "val first = \"bad"),
        (*classifier, "interface C<T break \"also bad"),
        (*sentinel, "val sentinel = 2 > 1"),
    ] {
        assert_eq!(
            sources.slice(items.get(id).unwrap().span()).unwrap(),
            expected
        );
    }
    assert_eq!(
        codes(parsed.diagnostics())
            .iter()
            .filter(|code| *code == "L0004")
            .count(),
        2
    );
}

#[test]
fn comparisons_in_closed_groups_do_not_end_the_type_parameter_list() {
    for head in [
        "interface C<T break f(; public val hidden = 2 > 1)>",
        "interface C<T break f([2 > 1)>",
        "interface C<T break f([(]2 > 1)>",
        "interface C<T break [2 > 1], U>",
    ] {
        let (sources, parsed) = file(&format!("{head}\nclass Sentinel"));
        let items = parsed.ast().items();
        let [class, sentinel] = parsed.roots() else {
            panic!("two file roots: {head}, {:?}", parsed.diagnostics());
        };
        assert_eq!(
            sources.slice(items.get(*class).unwrap().span()).unwrap(),
            head
        );
        assert_eq!(
            sources.slice(items.get(*sentinel).unwrap().span()).unwrap(),
            "class Sentinel"
        );
        assert_eq!(codes(parsed.diagnostics()), ["L0025"]);
    }
}

#[test]
fn an_unclosed_group_does_not_hide_the_type_parameter_closer() {
    let text = "interface C<T break f(>\nclass Sentinel";
    let (sources, parsed) = file(text);
    let items = parsed.ast().items();
    let [class, sentinel] = parsed.roots() else {
        panic!("two file roots: {:?}", parsed.diagnostics());
    };
    assert_eq!(
        sources.slice(items.get(*class).unwrap().span()).unwrap(),
        "interface C<T break f(>"
    );
    assert_eq!(
        sources.slice(items.get(*sentinel).unwrap().span()).unwrap(),
        "class Sentinel"
    );
    assert_eq!(codes(parsed.diagnostics()), ["L0025"]);
}

#[test]
fn a_paired_closer_releases_the_recovery_stack_after_a_wrong_inner_opener() {
    for following in [", U>\nclass Sentinel", "; public val sentinel = 2 > 1"] {
        let head = "interface C<T break f([2 > 1)";
        let (sources, parsed) = file(&format!("{head}{following}"));
        let items = parsed.ast().items();
        let [class, sentinel] = parsed.roots() else {
            panic!("two file roots: {following}, {:?}", parsed.diagnostics());
        };
        let first = items.get(*class).unwrap();
        let Item::Classifier(classifier) = first.payload() else {
            panic!("classifier")
        };
        if following.starts_with(',') {
            let [_, second] = classifier.type_parameters.as_slice() else {
                panic!("both parameters")
            };
            assert_eq!(sources.slice(second.span).unwrap(), "U");
            assert_eq!(
                sources.slice(items.get(*sentinel).unwrap().span()).unwrap(),
                "class Sentinel"
            );
            assert_eq!(codes(parsed.diagnostics()), ["L0025"]);
        } else {
            assert_eq!(sources.slice(first.span()).unwrap(), head);
            assert_eq!(
                sources.slice(items.get(*sentinel).unwrap().span()).unwrap(),
                "public val sentinel = 2 > 1"
            );
            assert_eq!(codes(parsed.diagnostics()), ["L0025", "L0010"]);
        }
    }
}

#[test]
fn a_comparison_in_a_recovered_group_cannot_hide_the_next_file_root() {
    let head = "interface C<T break f(2 > 1)";
    for following in ["class Sentinel", "public val sentinel = 2 > 1"] {
        let (sources, parsed) = file(&format!("{head}\n{following}"));
        let items = parsed.ast().items();
        let [class, sentinel] = parsed.roots() else {
            panic!("two roots: {following}, {:?}", parsed.diagnostics());
        };
        assert_eq!(
            sources.slice(items.get(*class).unwrap().span()).unwrap(),
            head
        );
        assert_eq!(
            sources.slice(items.get(*sentinel).unwrap().span()).unwrap(),
            following
        );
        assert_eq!(codes(parsed.diagnostics()), ["L0025", "L0010"]);
    }
}

#[test]
fn type_parameter_name_recovery_keeps_the_file_boundary_on_return() {
    let head = "interface C<break f(2 > 1)";
    let (sources, parsed) = file(&format!("{head}\nclass Sentinel"));
    let items = parsed.ast().items();
    let [class, sentinel] = parsed.roots() else {
        panic!("two roots: {:?}", parsed.diagnostics());
    };
    assert_eq!(
        sources.slice(items.get(*class).unwrap().span()).unwrap(),
        head
    );
    assert_eq!(
        sources.slice(items.get(*sentinel).unwrap().span()).unwrap(),
        "class Sentinel"
    );
}

#[test]
fn an_interpolation_closer_cannot_pop_an_enclosing_ordinary_recovery_group() {
    let head = "interface C<T break f(\"${x)}\" 2 > 1\nclass Hidden\n)>";
    let (sources, parsed) = file(&format!("{head}\nclass Sentinel"));
    let items = parsed.ast().items();
    let [class, sentinel] = parsed.roots() else {
        panic!("two roots: {:?}", parsed.diagnostics());
    };
    assert_eq!(
        sources.slice(items.get(*class).unwrap().span()).unwrap(),
        head
    );
    assert_eq!(
        sources.slice(items.get(*sentinel).unwrap().span()).unwrap(),
        "class Sentinel"
    );
    assert_eq!(codes(parsed.diagnostics()), ["L0025"]);
}

#[test]
fn a_previous_root_opener_cannot_cancel_a_recovered_groups_closer() {
    for (prefix, head) in [
        ("", "interface C<T break f(]2 > 1)>"),
        ("val x: [ = 1\n", "interface C<T break f(]2 > 1)>"),
        ("val x: [ = 1\n", "interface C<T break f(] [(] 2 > 1)>"),
    ] {
        let text = format!("{prefix}{head}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        let roots = parsed.roots();
        assert_eq!(
            roots.len(),
            2 + usize::from(!prefix.is_empty()),
            "{text}: {:?}",
            parsed.diagnostics()
        );
        let items = parsed.ast().items();
        let class = items.get(roots[roots.len() - 2]).unwrap();
        assert_eq!(sources.slice(class.span()).unwrap(), head);
        assert_eq!(
            sources
                .slice(items.get(*roots.last().unwrap()).unwrap().span())
                .unwrap(),
            "class Sentinel"
        );
    }
}

#[test]
fn a_group_closer_beyond_an_enclosing_brace_cannot_claim_an_angle() {
    let head = "interface C<T break f(>";
    let (sources, parsed) = file(&format!("{head}\n}}\nclass Sentinel\n)"));
    let items = parsed.ast().items();
    let first = items.get(parsed.roots()[0]).unwrap();
    assert_eq!(sources.slice(first.span()).unwrap(), head);
    assert!(
        parsed
            .roots()
            .iter()
            .any(|&id| sources.slice(items.get(id).unwrap().span()) == Ok("class Sentinel"))
    );
}

#[test]
fn file_class_angle_recovery_does_not_change_function_parameter_recovery() {
    for text in [
        "fun <T break f(2 > 1)\nclass Sentinel",
        "class C { fun <T break f(2 > 1)\nfun sentinel(): Int = 1\n}\nclass Sentinel",
    ] {
        let (sources, parsed) = file(text);
        let items = parsed.ast().items();
        assert_eq!(
            sources
                .slice(items.get(*parsed.roots().last().unwrap()).unwrap().span())
                .unwrap(),
            "class Sentinel"
        );
        if text.starts_with("class") {
            let Item::Classifier(classifier) = items.get(parsed.roots()[0]).unwrap().payload()
            else {
                panic!("outer class")
            };
            assert!(
                classifier
                    .body
                    .as_ref()
                    .unwrap()
                    .members
                    .iter()
                    .any(|&id| sources.slice(items.get(id).unwrap().span())
                        == Ok("fun sentinel(): Int = 1"))
            );
        }
    }
}
