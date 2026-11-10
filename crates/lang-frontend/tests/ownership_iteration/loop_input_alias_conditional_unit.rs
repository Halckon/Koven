//! A moved input keeps the actual arm and join evidence in both public input routes.
const EXITS: [(&str, &str); 9] = [
    ("", ""),
    ("continue", "continue"),
    ("continue", "break"),
    ("break", "continue"),
    ("break", "break"),
    ("", "continue"),
    ("continue", ""),
    ("", "break"),
    ("break", ""),
];
const SOURCE: &str = r#"fun first(flag: Boolean, other: Boolean) { val seed = "outer"; val held = seed; while(flag) { val local = "inner"; if(other) {} else {} }; val retired = held }"#;
fn check(rooted: bool, text: &str, left: &str, right: &str) {
    match (left, right) {
        ("", "") => {
            super::invariant_normal_join_owner_unit::check_source(rooted, text, true, false)
        }
        ("continue", "continue") => {
            super::invariant_split_continue_owner_unit::check_source(rooted, text, true)
        }
        ("", _) | (_, "") => {
            super::invariant_mixed_join_owner_unit::check_source(rooted, text, true)
        }
        _ => super::invariant_split_exits_owner_unit::check_source(rooted, text, true, false),
    }
}
// Preserve the exact former nine §176 rejection sources, now checking their specific capability.
pub(super) fn check_original_conditional(rooted: bool, text: &str) -> bool {
    let Some((left, right)) = EXITS.into_iter().find(|(left, right)| {
        text == SOURCE.replace("first", "run").replace(
            "if(other) {} else {}",
            &format!("if(other) {{ {left} }} else {{ {right} }}"),
        )
    }) else {
        return false;
    };
    check(rooted, text, left, right);
    true
}
#[test]
fn conditional_named_input_matches_both_public_routes() {
    for (condition, other) in [
        ("flag", "other"),
        ("(flag)", "(other)"),
        ("!flag", "other"),
        ("flag", "!other"),
        ("!(!flag)", "(!other)"),
    ] {
        for result in ["", ": Unit"] {
            for body in [
                r#"val local = "inner""#,
                r#"val local = "inner"; val alias = (local)"#,
                r#"var root = "old"; val incoming = "next"; val old = replace(&root, incoming)"#,
                r#"var left = "left"; var right = "right"; val old = replace(&left, replace(&right, "next"))"#,
            ] {
                for (left, right) in EXITS {
                    let text = format!(
                        r#"fun first(flag: Boolean, other: Boolean){result} {{ val seed = "outer"; val held = seed; while({condition}) {{ {body}; if({other}) {{ {left} }} else {{ {right} }} }}; val retired = held }}"#
                    );
                    for rooted in [false, true] {
                        check(rooted, &text, left, right);
                    }
                }
            }
        }
    }
    for text in [
        SOURCE.replace("val seed", "var seed"),
        SOURCE.replace("val held", "var held"),
        SOURCE.replace("val retired", "var retired"),
        SOURCE.replace("val seed =", "val seed: String? ="),
        SOURCE.replace("val held =", "val held: String? ="),
        SOURCE.replace("val retired =", "val retired: String? ="),
        SOURCE.replace("= seed", "= (seed)"),
        SOURCE.replace("= held", "= (held)"),
        SOURCE.replace("= \"outer\"", "= (\"outer\")"),
        SOURCE.replace("val held = seed", "val middle = seed; val held = middle"),
        SOURCE.replace("if(other)", "if(flag)"),
        SOURCE.replace("if(other)", "if(false)"),
    ] {
        for rooted in [false, true] {
            super::invariant_normal_join_owner_unit::check_source(rooted, &text, false, true);
        }
    }
}
