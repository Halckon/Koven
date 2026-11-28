use super::*;

#[test]
fn selector_with_a_paired_group_keeps_its_own_arm_when_separator_is_missing() {
    for newline in ["\n", "\r\n"] {
        for (body, selector) in [
            ("produce(1)", "((true))"),
            ("produce(1)", "{ -> true }"),
            ("produce", "{ -> true }"),
        ] {
            let following = "fun after(): Int = 7";
            let selector_entry = format!("{selector} -> 2");
            let first = format!("false -> {body}");
            let probe =
                format!("fun probe(): Int = when {{ {first} /*臂*/ {selector_entry}; else -> 3 }}");
            let text = format!("{probe}{newline}{following}");
            let mut sources = SourceMap::new();
            let id = add_source(&mut sources, &text);
            let parsed = parse_file_twice(&sources, id, &text);
            assert_eq!(codes(parsed.diagnostics()), ["L0065"], "{text:?}");

            let start = text.find(&selector_entry).unwrap();
            let primary = parsed.diagnostics()[0].primary_span();
            assert_eq!(
                (primary.start(), primary.end()),
                (start, start + 1),
                "point to the start of the missing-separator arm"
            );
            let [probe_root, following_root] = parsed.roots() else {
                panic!("both declarations remain file roots: {text:?}");
            };
            for (root, expected) in [(*probe_root, probe.as_str()), (*following_root, following)] {
                let span = parsed.ast().items().get(root).unwrap().span();
                assert_eq!(sources.slice(span), Ok(expected));
            }
            let entries = parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(_, node)| match node.payload() {
                    Expression::When { entries, .. } => Some(entries),
                    _ => None,
                })
                .expect("the when remains a file expression");
            let [initial, next, other] = entries.as_slice() else {
                panic!("expected three real arms: {entries:?}");
            };
            assert_eq!(sources.slice(initial.span), Ok(first.as_str()));
            assert_eq!(sources.slice(next.span), Ok(selector_entry.as_str()));
            assert_eq!(sources.slice(next.arrow_span), Ok("->"));
            assert!(other.else_span.is_some());
            assert_eq!(sources.slice(other.span), Ok("else -> 3"));
        }
    }
}

#[test]
fn a_real_arm_separator_keeps_valid_postfixes_in_their_original_body() {
    for (body, next_arm) in [
        ("produce(1) (true)", "true -> 2"),
        ("produce(1) { -> true }", "true -> 2"),
        ("produce { -> true }", "true -> 2"),
    ] {
        let text = format!("when {{ false -> {body}; {next_arm}; else -> 3 }}");
        let (sources, parsed) = expression(&text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when");
        };
        let [first, second, _] = entries.as_slice() else {
            panic!("expected three real arms");
        };
        let body_span = parsed.ast().statements().get(first.body).unwrap().span();
        assert_eq!(sources.slice(body_span), Ok(body));
        assert_eq!(sources.slice(second.span), Ok(next_arm));
    }
}

#[test]
fn an_arrow_inside_a_paired_operand_cannot_start_an_outer_arm() {
    for (body, expected_code) in [
        ("outer(produce(1) (true) -> 2)", "L0034"),
        ("outer(produce(1) { -> true } -> 2)", "L0034"),
        ("table[produce(1) (true) -> 2]", "L0010"),
        ("(produce(1) (true) -> 2)", "L0010"),
    ] {
        let text = format!("when {{ false -> {body}; else -> 3 }}");
        let (sources, parsed) = expression(&text);
        let last_arrow = text.find("-> 2").unwrap();
        let same_diagnostic: Vec<_> = parsed
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code().to_string() == expected_code)
            .map(|diagnostic| diagnostic.primary_span().start())
            .collect();
        assert_eq!(same_diagnostic, [last_arrow], "{text:?}");
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when: {text:?}");
        };
        assert!(
            !entries
                .iter()
                .filter(|entry| entry.else_span.is_none())
                .any(|entry| {
                    sources.slice(entry.span).is_ok_and(|source| {
                        source.starts_with("(true)") || source.starts_with("{ -> true }")
                    })
                }),
            "inner postfix promoted to an outer arm: {text:?}"
        );
    }
}

#[test]
fn unparenthesized_for_inside_a_control_block_keeps_its_own_postfix() {
    let text = "when { false -> { for item in produce(1) (true) -> 2 }; else -> 3 }";
    let (sources, parsed) = expression(text);
    let body_error = parsed
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code().to_string() == "L0060")
        .expect("the malformed for needs a body");
    assert_eq!(
        (
            body_error.primary_span().start(),
            body_error.primary_span().end()
        ),
        (text.find("-> 2").unwrap(), text.find("-> 2").unwrap() + 2)
    );
    let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
        panic!("expected a when");
    };
    let [_, otherwise] = entries.as_slice() else {
        panic!("for recovery stays in the first arm");
    };
    assert_eq!(sources.slice(otherwise.span), Ok("else -> 3"));
}

#[test]
fn same_line_arm_recovery_restores_the_outer_owner_and_handles_missing_closers() {
    let inner = "(when { else -> 1 })";
    let nested = format!("when {{ false -> {inner} (true) -> 2; else -> 3 }}");
    let unclosed = "consume(when { false -> produce(1) (true) -> 2)";
    for (text, expected) in [
        (nested.as_str(), &["L0065"][..]),
        (unclosed, &["L0065", "L0010"][..]),
    ] {
        let (sources, parsed) = expression(text);
        assert_eq!(codes(parsed.diagnostics()), expected, "{text:?}");
        assert!(
            parsed.ast().expressions().iter().any(|(_, node)| {
                let Expression::When { entries, .. } = node.payload() else {
                    return false;
                };
                entries
                    .iter()
                    .any(|entry| sources.slice(entry.span) == Ok("(true) -> 2"))
            }),
            "{text:?}"
        );
    }
}

#[test]
fn same_line_compound_selector_keeps_the_preceding_call_and_its_own_arrow() {
    for newline in ["\n", "\r\n"] {
        for (operator, jump, earlier) in [
            ("&&", "break", "produce(1)"),
            ("||", "continue", "produce (1)"),
            ("&&", "continue", "keep (1) && produce(1)"),
            ("&&", "break", "keep (1, 2) && produce(1)"),
        ] {
            let selector = format!("((true)) {operator} if (flag) {jump} else true -> 2");
            let following = "fun after(): Int = 7";
            let probe = format!(
                "fun probe(flag: Boolean): Unit {{ loop {{ when {{ false -> {earlier} /*臂*/ {selector}; else -> 3 }}; break }} }}"
            );
            let text = format!("{probe}{newline}{following}");
            let mut sources = SourceMap::new();
            let id = add_source(&mut sources, &text);
            let parsed = parse_file_twice(&sources, id, &text);
            assert_eq!(codes(parsed.diagnostics()), ["L0065"], "{text:?}");
            let selector_start = text.find(&selector).unwrap();
            let primary = parsed.diagnostics()[0].primary_span();
            assert_eq!(
                (primary.start(), primary.end()),
                (selector_start, selector_start + 1)
            );
            let [_, next] = parsed.roots() else {
                panic!("keep the later declaration");
            };
            assert_eq!(
                sources.slice(parsed.ast().items().get(*next).unwrap().span()),
                Ok(following)
            );
            let entries = parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(_, node)| match node.payload() {
                    Expression::When { entries, .. } => Some(entries),
                    _ => None,
                })
                .expect("the when remains a file expression");
            let [first, second, otherwise] = entries.as_slice() else {
                panic!("expected three real arms: {entries:?}");
            };
            assert_eq!(
                sources.slice(first.span),
                Ok(format!("false -> {earlier}").as_str())
            );
            assert_eq!(sources.slice(second.span), Ok(selector.as_str()));
            assert_eq!(sources.slice(second.arrow_span), Ok("->"));
            assert_eq!(sources.slice(otherwise.span), Ok("else -> 3"));
        }
    }
}

#[test]
fn same_line_grouped_selector_reuses_infix_shapes_and_preserves_postfix_calls() {
    for (selector, expected) in [
        ("(true) == predicate() -> 2", true),
        ("(1) is Int -> 2", true),
        ("(true) && predicate () -> 2", true),
        ("(true) && predicate (1, 2) == 3 -> 2", true),
        ("(true) && predicate (&x) == 1 -> 2", true),
        ("(true) && move { -> true } != other -> 2", true),
        ("(true) && false, predicate (1) == 1 -> 2", true),
        ("(value as Pair<A, B>) != sentinel -> 2", true),
        ("(load<A, B>(1, 2)) != sentinel -> 2", true),
        ("(super<InterfaceType<A, B>>.ready()) && true -> 2", true),
        (
            "(true) && if (flag) predicate (1) == 1 else true -> 2",
            true,
        ),
        ("(true) && predicate (); true -> 2", false),
    ] {
        let text = format!("when {{ false -> produce(1) {selector}; else -> 3 }}");
        let (sources, parsed) = expression(&text);
        assert_eq!(
            codes(parsed.diagnostics()),
            if expected { &["L0065"][..] } else { &[][..] },
            "{text:?}"
        );
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when");
        };
        assert_eq!(entries.len(), 3);
        if expected {
            assert_eq!(sources.slice(entries[0].span), Ok("false -> produce(1)"));
            assert_eq!(sources.slice(entries[1].span), Ok(selector));
        } else {
            assert_eq!(
                sources.slice(entries[0].span),
                Ok("false -> produce(1) (true) && predicate ()")
            );
        }
    }
}

#[test]
fn a_when_inside_a_then_body_still_recovers_its_own_same_line_selector() {
    let selector = "(true) && if (inner) predicate (1) == 1 else true -> 2";
    let text = format!("if (outer) when {{ false -> produce(1) {selector}; else -> 3 }} else 4");
    let (sources, parsed) = expression(&text);
    assert_eq!(codes(parsed.diagnostics()), ["L0065"]);
    let Expression::If {
        else_branch: Some(other),
        ..
    } = expression_payload(&parsed, parsed.root())
    else {
        panic!("keep the containing if's own else");
    };
    assert_eq!(
        sources.slice(parsed.ast().statements().get(*other).unwrap().span()),
        Ok("4")
    );
    assert!(parsed.ast().expressions().iter().any(|(_, node)| {
        match node.payload() {
            Expression::When { entries, .. } => entries
                .iter()
                .any(|entry| sources.slice(entry.span) == Ok(selector)),
            _ => false,
        }
    }));
}

#[test]
fn same_line_compound_selector_can_follow_a_then_only_arm() {
    let selector = "(true) && if (other) break else true -> 2";
    let separated = format!("when {{ false -> if (flag) produce(1); {selector}; else -> 3 }}");
    let (_, control) = expression(&separated);
    assert_eq!(codes(control.diagnostics()), ["L0057"]);
    let source = separated.replacen("produce(1);", "produce(1)", 1);
    let (sources, parsed) = expression(&source);
    assert_eq!(
        codes(parsed.diagnostics()),
        ["L0057", "L0065"],
        "{source:?}"
    );
    let (_, statement) = block(&format!("{{ {source} }}"));
    assert_eq!(codes(statement.diagnostics()), ["L0065"], "{source:?}");
    let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
        panic!("expected a when");
    };
    let [first, second, otherwise] = entries.as_slice() else {
        panic!("expected three real arms");
    };
    assert_eq!(
        sources.slice(first.span),
        Ok("false -> if (flag) produce(1)")
    );
    assert_eq!(sources.slice(second.span), Ok(selector));
    assert_eq!(sources.slice(otherwise.span), Ok("else -> 3"));
}

#[test]
fn grouped_selector_followed_by_comma_keeps_both_conditions() {
    assert_grouped_selector_follower("(true), false", 2);
}

#[test]
fn grouped_selector_followed_by_a_member_keeps_the_member_expression() {
    for selector in [
        "(value).ready() && flag",
        "(value)?.ready() == true",
        "(value)::ready == other",
    ] {
        assert_grouped_selector_follower(selector, 1);
    }
}

fn assert_grouped_selector_follower(selector: &str, conditions: usize) {
    let first = "false -> produce(1)";
    let next = format!("{selector} -> 2");
    for separator in [";", ""] {
        let source = format!("when {{ {first}{separator} /*臂*/ {next}; else -> 3 }}");
        let (sources, parsed) = expression(&source);
        let diagnostics = parsed.diagnostics();
        if separator.is_empty() {
            assert_eq!(codes(diagnostics), ["L0065"], "{source:?}");
            let start = source.find(selector).unwrap();
            let primary = diagnostics[0].primary_span();
            assert_eq!((primary.start(), primary.end()), (start, start + 1));
        } else {
            assert!(diagnostics.is_empty(), "{source:?}: {diagnostics:?}");
        }
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when");
        };
        let [before, arm, otherwise] = entries.as_slice() else {
            panic!("expected three real arms: {source:?}");
        };
        assert_eq!(sources.slice(before.span), Ok(first));
        assert_eq!(sources.slice(arm.span), Ok(next.as_str()));
        assert_eq!(arm.conditions.len(), conditions);
        assert_eq!(sources.slice(otherwise.span), Ok("else -> 3"));
    }
}

#[test]
fn grouped_call_with_member_continuation_remains_in_the_arm_without_an_arrow() {
    let first = "false -> produce(1) (value).ready() && flag";
    let source = format!("when {{ {first}; true -> 2; else -> 3 }}");
    let (sources, parsed) = expression(&source);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
        panic!("expected a when");
    };
    assert_eq!(entries.len(), 3);
    assert_eq!(sources.slice(entries[0].span), Ok(first));
}

#[test]
fn grouped_selector_postfix_index_keeps_its_own_arm() {
    assert_grouped_selector_follower("(value)[0]", 1);
}

#[test]
fn grouped_selector_postfix_call_keeps_its_own_arm() {
    assert_grouped_selector_follower("(callback)()", 1);
    assert_grouped_selector_follower("(callback)(true)", 1);
}

#[test]
fn grouped_selector_postfix_lambda_keeps_its_own_arm() {
    assert_grouped_selector_follower("(callback){ -> true }", 1);
    assert_grouped_selector_follower("(callback)<Boolean> { -> true }", 1);
}

#[test]
fn grouped_selector_postfix_non_null_keeps_its_own_arm() {
    assert_grouped_selector_follower("(value)!!", 1);
}

#[test]
fn grouped_selector_postfix_propagation_keeps_its_own_arm() {
    assert_grouped_selector_follower("(value)?", 1);
}

#[test]
fn grouped_postfixes_without_a_same_level_arrow_stay_in_the_arm() {
    for suffix in [
        "(value)[0]",
        "(value)(true)",
        "(value)!!",
        "(value)?",
        "(value)<Boolean> { -> true }",
        "(value).ready { -> true }",
    ] {
        let first = format!("false -> produce(1) {suffix}");
        let source = format!("when {{ {first}; true -> 2; else -> 3 }}");
        let (sources, parsed) = expression(&source);
        assert!(
            parsed.diagnostics().is_empty(),
            "{source:?}: {:?}",
            parsed.diagnostics()
        );
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when");
        };
        assert_eq!(entries.len(), 3);
        assert_eq!(sources.slice(entries[0].span), Ok(first.as_str()));
    }
}

#[test]
fn spaced_lambda_with_its_own_arrow_uses_the_latest_eligible_boundary() {
    let first = "false -> produce(1) (callback)";
    let next = "{ -> true } -> 2";
    for separator in [" ", "\n", "\r\n"] {
        let source = format!("when {{ {first}{separator}{next}; else -> 3 }}");
        let (sources, parsed) = expression(&source);
        let diagnostics = parsed.diagnostics();
        if separator == " " {
            assert_eq!(codes(diagnostics), ["L0065"], "{source:?}");
            let start = source.find(next).unwrap();
            let primary = diagnostics[0].primary_span();
            assert_eq!((primary.start(), primary.end()), (start, start + 1));
        } else {
            assert!(diagnostics.is_empty(), "{source:?}: {diagnostics:?}");
        }
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when");
        };
        let [before, arm, otherwise] = entries.as_slice() else {
            panic!("expected three real arms");
        };
        assert_eq!(sources.slice(before.span), Ok(first));
        assert_eq!(sources.slice(arm.span), Ok(next));
        assert_eq!(sources.slice(otherwise.span), Ok("else -> 3"));
    }
}
