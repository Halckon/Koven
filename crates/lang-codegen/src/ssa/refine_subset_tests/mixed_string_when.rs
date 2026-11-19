//! SPEC-0360 compatibility observation, not expanded refinement admission.
//! Keep the original terminal-argument negative route and the per-family fact guard.
use super::*;
use lang_frontend::ownership_checking::UnitDropPoint;

#[test]
fn string_when_and_terminal_argument_have_a_real_mixed_body_boundary() {
    const WHEN: &str = "when (s) { \"a\" -> 1; else -> 0 }";
    // This is the original route-negative source, unchanged.
    const TERMINAL: &str = "class Node()\nfun outer(node: Node, own r: Int): Int = r\n\
        fun f(flag: Boolean): Int = outer(Node(), when (flag) { true -> return 1\n else -> 0 })";
    let green = format!("fun f(s: String): Int = {WHEN}");
    let mixed = format!(
        "class Node()\nfun outer(node: Node, own r: Int): Int = r\n\
         fun f(s: String, flag: Boolean): Int {{ val n = {WHEN}\n\
         return outer(Node(), when (flag) {{ true -> return n\n else -> n }}) }}"
    );
    let mut comparison_counts = Vec::new();
    for (source, expected_refine, has_string_when) in [
        (green.as_str(), true, true),
        (TERMINAL, false, false),
        (mixed.as_str(), false, true),
    ] {
        let mut sources = SourceMap::new();
        let (id, file) = parsed(&mut sources, "p/main.ko", &format!("package p\n{source}"));
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (ne, te) = standard_environments();
        // All three sources must pass names, types and ownership before observing routing.
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        let comparisons = owned
            .ownership()
            .drops()
            .iter()
            .filter(|fact| matches!(fact.point(), UnitDropPoint::WhenComparisonCompleted { .. }))
            .count();
        if has_string_when {
            assert!(
                comparisons > 0,
                "must publish real candidate cleanup facts: {source}"
            );
            comparison_counts.push(comparisons);
        } else {
            assert_eq!(
                comparisons, 0,
                "original terminal control has no String comparison"
            );
        }
        let entry = declaration(&names, "p", "f");
        let (result, routed) = observe_routing(|| {
            lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
        });
        let f_refined = routed.iter().any(|name| name.starts_with("koven.p.f.d"));
        assert_eq!(f_refined, expected_refine, "{source}: {routed:?}");
        if expected_refine {
            let (program, _) = result.expect("standalone String when lowers");
            crate::llvm::render_verified_program(&program)
                .expect("standalone String when verifies");
        } else if has_string_when {
            let error = match result {
                Err(error) => error,
                Ok(_) => panic!("mixed body must not silently bypass candidate cleanup"),
            };
            assert_eq!(error.kind, super::super::LoweringErrorKind::UnsupportedNode);
            assert_eq!(
                sources
                    .slice(error.span.expect("exact rejected String when span"))
                    .unwrap(),
                WHEN
            );
        }
        // The original terminal fixture constrains routing, not fallback success.
    }
    assert_eq!(
        comparison_counts[0], comparison_counts[1],
        "same String when retains the same comparison cleanup fact count in the mixed body"
    );
}
