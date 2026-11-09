//! Ordinary completion must agree with emitted CFG exits.
use super::{render_program, route};

#[test]
fn named_nothing_call_uses_refine_and_stops_at_the_call() {
    for callee in [
        r#"fun stop(): Nothing = error("stop")"#,
        "fun stop(): Nothing = stop()",
        "fun stop(): Nothing { loop { continue } }",
    ] {
        let (result, routed) = route(&format!("{callee}\nfun f(): Int = stop()"));
        assert!(routed, "a direct Nothing result must qualify for refine");
        let program = result.unwrap();
        let rendered = render_program(&program);
        assert!(rendered.contains("noreturn"));
        assert!(rendered.contains("unreachable"));
    }
}

#[test]
fn exhaustive_when_inside_loop_keeps_the_suffix_unreachable() {
    for (declarations, subject, arms) in [
        ("", "Boolean", "(true) -> continue; false -> continue"),
        (
            "",
            "Boolean",
            "true -> continue; false -> continue; else -> break",
        ),
        (
            "enum class Flag { A, B }",
            "Flag",
            "Flag.A -> continue; Flag.B -> continue; else -> break",
        ),
    ] {
        let source = format!(
            "{declarations}\nfun f(flag: {subject}): Int {{ loop {{ when (flag) {{ {arms} }}; break }} }}"
        );
        let (result, routed) = route(&source);
        let program = result.unwrap_or_else(|error| panic!("{arms}: {error:?}"));
        assert!(routed, "{arms}");
        crate::llvm::render_verified_program(&program).expect("loop lowers to verified LLVM");
    }
}

#[test]
fn while_condition_without_a_false_edge_keeps_the_suffix_unreachable() {
    let (program, routed) = route(
        "fun unreachable(): Unit {}
         fun f(flag: Boolean): Int {
             loop {
                 when { flag -> {}; else -> {} }
                 while (flag || ((return 7) == 0)) {}
                 unreachable()
                 break
             }
             unreachable()
         }",
    );
    assert!(
        routed,
        "refine must skip the dead calls in both loop suffixes"
    );
    crate::llvm::render_verified_program(&program.unwrap()).expect("verified while control");
}
