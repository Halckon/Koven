//! Bare Continue keeps its real jump while the condition-false suffix consumes the input.
#[test]
fn continue_suffix_consumption_matches_both_public_inputs() {
    for condition in ["flag", "(flag)", "!flag", "(!flag)", "!(!flag)"] {
        for result in ["", ": Unit"] {
            for body in [
                r#"val local = "inner""#,
                r#"val local = "inner"; val alias = (local)"#,
                r#"var root = "old"; val incoming = "next"; val old = replace(&root, incoming)"#,
                r#"var left = "left"; var right = "right"; val old = replace(&left, replace(&right, "next"))"#,
            ] {
                let text = format!(
                    r#"fun run(flag: Boolean){result} {{ val held = "outer"; while({condition}) {{ {body}; continue }}; val retired = held }}"#
                );
                for rooted in [false, true] {
                    super::invariant_suffix_owner_unit::check(rooted, &text, true, true);
                }
            }
        }
    }
    for suffix in [
        "val retired = (held)",
        "val retired: String? = held",
        "val retired = held; val extra = \"extra\"",
    ] {
        let text = format!(
            r#"fun run(flag: Boolean) {{ val held = "outer"; while(flag) {{ val local = "inner"; continue }}; {suffix} }}"#
        );
        for rooted in [false, true] {
            super::invariant_suffix_owner_unit::check(rooted, &text, false, false);
        }
    }
}
