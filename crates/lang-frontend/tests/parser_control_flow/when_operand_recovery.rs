use super::*;
use lang_frontend::parser::TypeRef;

#[test]
fn unclosed_generic_arguments_keep_later_when_arms() {
    for (operand, selector, expected_closers, missing_argument) in [
        ("as List<", "flag", 2, true),
        ("as List<", "flag > limit", 2, true),
        ("as List<", "flag > and", 2, true),
        ("as List<", "flag > and.member", 2, true),
        ("is Map<Int,", "flag", 2, true),
        ("as Map<Int, List<", "((flag))", 3, true),
        ("as List<Int", "-limit", 2, false),
    ] {
        let text = format!("when {{ false -> take(value {operand}\n{selector} -> 2\nelse -> 3 }}");
        let (sources, parsed) = expression(&text);
        let mut expected_codes = vec!["L0010"; expected_closers];
        if missing_argument {
            expected_codes.push("L0014");
        }
        assert_eq!(codes(parsed.diagnostics()), expected_codes, "{text}");
        let boundary = text.find(&format!("{selector} ->")).unwrap();
        for diagnostic in parsed.diagnostics() {
            let span = diagnostic.primary_span();
            assert_eq!((span.start(), span.end()), (boundary, boundary));
        }
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when expression");
        };
        let [damaged, next, fallback] = entries.as_slice() else {
            panic!("both later arms must survive: {text}");
        };
        assert_eq!(
            sources.slice(damaged.span),
            Ok(format!("false -> take(value {operand}").as_str())
        );
        assert_eq!(
            sources.slice(next.span),
            Ok(format!("{selector} -> 2").as_str())
        );
        assert_eq!(sources.slice(fallback.span), Ok("else -> 3"));
    }
}

#[test]
fn multiline_generic_type_arguments_keep_their_own_tokens() {
    for (type_ref, diagnostics) in [
        ("List<\nInt>", &["L0010"][..]),
        ("List<\n(Int) -> Int>", &["L0010"]),
        ("List<\n(Int) -> Int", &["L0010", "L0010"]),
    ] {
        let text = format!("when {{ false -> take(value is {type_ref}\nelse -> 2 }}");
        let (sources, parsed) = expression(&text);
        assert_eq!(codes(parsed.diagnostics()), diagnostics, "{text}");
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when expression");
        };
        let [damaged, fallback] = entries.as_slice() else {
            panic!("the function arrow is not a when selector");
        };
        assert_eq!(
            sources.slice(damaged.span),
            Ok(format!("false -> take(value is {type_ref}").as_str())
        );
        assert_eq!(sources.slice(fallback.span), Ok("else -> 2"));
    }

    let text = "when { false -> take(value is List<\nInt>\nflag -> 2\nelse -> 3 }";
    let (sources, parsed) = expression(text);
    assert_eq!(codes(parsed.diagnostics()), ["L0010"]);
    let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
        panic!("expected a when expression");
    };
    assert_eq!(
        sources.slice(entries[0].span),
        Ok("false -> take(value is List<\nInt>")
    );
    assert_eq!(sources.slice(entries[1].span), Ok("flag -> 2"));
}

#[test]
fn complete_generic_type_precedes_a_continuation_or_independent_arm() {
    for (type_and_tail, selector) in [("List<\nInt> and limit", "flag"), ("List<\nInt>", "limit")] {
        let text = format!(
            "when {{ false -> take(value as {type_and_tail}\n{selector} -> 2\nelse -> 3 }}"
        );
        let (sources, parsed) = expression(&text);
        assert_eq!(codes(parsed.diagnostics()), ["L0010"], "{text}");
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when expression");
        };
        let [damaged, next, _] = entries.as_slice() else {
            panic!("expected three arms");
        };
        assert_eq!(
            sources.slice(damaged.span),
            Ok(format!("false -> take(value as {type_and_tail}").as_str())
        );
        assert_eq!(
            sources.slice(next.span),
            Ok(format!("{selector} -> 2").as_str())
        );
    }
}

#[test]
fn missing_operator_type_in_an_unclosed_call_keeps_the_next_when_arm() {
    for (operator, selector, expected_codes) in [
        ("as", "flag", &["L0010", "L0014"][..]),
        ("as?", "((flag))", &["L0010", "L0014"]),
        ("is", "flag && other", &["L0010", "L0014"]),
        ("!is", "flag", &["L0010", "L0014"]),
        ("is Int is", "flag", &["L0012", "L0010", "L0014"]),
    ] {
        let text = format!("when {{ false -> take(value {operator}\n{selector} -> 2\nelse -> 3 }}");
        let (sources, parsed) = expression(&text);
        assert_eq!(codes(parsed.diagnostics()), expected_codes, "{text}");
        let boundary = text.find(&format!("{selector} ->")).unwrap();
        for diagnostic in &parsed.diagnostics()[expected_codes.len() - 2..] {
            let span = diagnostic.primary_span();
            assert_eq!((span.start(), span.end()), (boundary, boundary));
        }
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when expression");
        };
        let [damaged, next, fallback] = entries.as_slice() else {
            panic!("both later arms must survive: {text}");
        };
        assert_eq!(
            sources.slice(damaged.span),
            Ok(format!("false -> take(value {operator}").as_str())
        );
        assert_eq!(
            sources.slice(next.span),
            Ok(format!("{selector} -> 2").as_str())
        );
        assert_eq!(sources.slice(fallback.span), Ok("else -> 3"));
        let error_span = parsed
            .ast()
            .type_refs()
            .iter()
            .find_map(|(_, node)| matches!(node.payload(), TypeRef::Error).then_some(node.span()))
            .expect("a missing type has an explicit Error TypeRef");
        let end = text.find('\n').unwrap();
        assert_eq!((error_span.start(), error_span.end()), (end, end));
    }
}

#[test]
fn paired_call_retains_named_types_after_a_line_break() {
    let text = "when { false -> take(value as\nInt, value is\nInt)\nflag -> 2\nelse -> 3 }";
    let (sources, parsed) = expression(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
        panic!("expected a when expression");
    };
    assert_eq!(
        sources.slice(entries[0].span),
        Ok("false -> take(value as\nInt, value is\nInt)")
    );
    assert_eq!(sources.slice(entries[1].span), Ok("flag -> 2"));
}

#[test]
fn complete_function_type_owns_its_arrow_even_in_an_unclosed_call() {
    for type_start in ["(Int)", "move (Int)"] {
        let text = format!("when {{ false -> take(value is\n{type_start} -> Int\nelse -> 2 }}");
        let (sources, parsed) = expression(&text);
        assert_eq!(codes(parsed.diagnostics()), ["L0010"], "{text}");
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when expression");
        };
        let [damaged, fallback] = entries.as_slice() else {
            panic!("the function arrow is not a when selector");
        };
        assert_eq!(
            sources.slice(damaged.span),
            Ok(format!("false -> take(value is\n{type_start} -> Int").as_str())
        );
        assert_eq!(sources.slice(fallback.span), Ok("else -> 2"));
    }
}

#[test]
fn unfinished_call_operand_keeps_the_next_when_arm() {
    for (operand, selector, fallback) in [
        ("1 +", "flag", "\nelse -> 3"),
        ("1 +", "-limit", "\nelse -> 3"),
        ("!", "((flag))", "\nelse -> 3"),
        ("!", "else", ""),
    ] {
        let text = format!("when {{ false -> take({operand}\n{selector} -> 2{fallback} }}");
        let (sources, parsed) = expression(&text);
        assert_eq!(codes(parsed.diagnostics()), ["L0009", "L0010"], "{text}");
        let first_selector = text.find(&format!("{selector} ->")).unwrap();
        for diagnostic in parsed.diagnostics() {
            let at = diagnostic.primary_span();
            assert_eq!((at.start(), at.end()), (first_selector, first_selector));
        }
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when expression");
        };
        assert_eq!(
            sources.slice(entries[0].span),
            Ok(format!("false -> take({operand}").as_str())
        );
        assert_eq!(
            sources.slice(entries[1].span),
            Ok(format!("{selector} -> 2").as_str())
        );
        let Statement::Expression { expression: call } = parsed
            .ast()
            .statements()
            .get(entries[0].body)
            .unwrap()
            .payload()
        else {
            panic!("first arm must retain the expression");
        };
        let Expression::Call { arguments, .. } = expression_payload(&parsed, *call) else {
            panic!("first arm must retain its call");
        };
        let [argument] = arguments.as_slice() else {
            panic!("next selector must not become an argument");
        };
        assert_eq!(sources.slice(argument.span), Ok(operand));
    }
}

#[test]
fn an_unclosed_call_cannot_turn_the_outer_else_into_an_if_else() {
    for operand in ["if (flag) take(1 +", "take(if (flag) 1 +"] {
        let text = format!("when {{ false -> {operand}\nelse -> 2 }}");
        let (sources, parsed) = expression(&text);
        assert_eq!(
            codes(parsed.diagnostics()),
            ["L0057", "L0009", "L0010"],
            "{text}"
        );
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when expression");
        };
        let [damaged, fallback] = entries.as_slice() else {
            panic!("the outer when needs both arms");
        };
        assert_eq!(
            sources.slice(damaged.span),
            Ok(format!("false -> {operand}").as_str())
        );
        assert_eq!(sources.slice(fallback.span), Ok("else -> 2"));
    }
}

#[test]
fn missing_if_condition_closer_keeps_the_outer_when_body_boundary() {
    for selector in ["flag", "{ x -> x }"] {
        let text = format!("when {{ false -> if (take(1 +\n{selector} -> 2\nelse -> 3 }}");
        let (sources, parsed) = expression(&text);
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("expected a when expression");
        };
        let [damaged, next, fallback] = entries.as_slice() else {
            panic!("outer when must retain both later arms: {text}");
        };
        assert_eq!(sources.slice(damaged.span), Ok("false -> if (take(1 +"));
        assert_eq!(
            sources.slice(next.span),
            Ok(format!("{selector} -> 2").as_str())
        );
        assert_eq!(sources.slice(fallback.span), Ok("else -> 3"));
        assert!(
            codes(parsed.diagnostics())
                .iter()
                .any(|code| code == "L0009")
        );
    }
}

#[test]
fn paired_call_retains_multiline_prefix_and_infix_operands() {
    let text = "when { false -> take(1 +\nlimit, !\nflag)\nflag -> 2\nelse -> 3 }";
    let (sources, parsed) = expression(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
        panic!("expected a when expression");
    };
    assert_eq!(
        sources.slice(entries[0].span),
        Ok("false -> take(1 +\nlimit, !\nflag)")
    );
    assert_eq!(sources.slice(entries[1].span), Ok("flag -> 2"));
}
