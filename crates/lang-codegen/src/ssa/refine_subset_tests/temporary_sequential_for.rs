//! Local destructuring and temporary sequential-source routing contracts.

use super::route;
use crate::ssa::lowering_support::LoweringErrorKind;

#[test]
fn temporary_sequential_for_routes_with_local_destructuring_on_either_side() {
    let pattern = "val (second, first) = Pair(20, 22)";
    for constructor in ["arrayOf", "listOf", "mutableListOf"] {
        let iteration = format!(
            "for ((value, _) in {constructor}(Pair(20, 22))) {{ if (value == 20) {{ println(\"loop\") }} }}"
        );
        for (before, after) in [(pattern, iteration.as_str()), (&iteration, pattern)] {
            let source = format!(
                "value class Pair(val first: Int, val second: Int)\n\
                 fun f(): Int {{ {before}; {after}; return first }}"
            );
            let (result, routed) = route(&source);
            assert!(
                routed,
                "local destructuring requires refine: {source}\n{:?}",
                result.as_ref().err()
            );
            let program = result.expect("admitted body must lower");
            crate::ssa::verify::verify_program(&program).expect("program verifies");
        }
    }
}

#[test]
fn temporary_sequential_for_does_not_inherit_sibling_local_destructuring() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun sibling(): Int { val (second, first) = Pair(20, 22); return first }
    fun f(): Unit { for (item in listOf("kept")) { println(item) } }
    "#;
    let (result, routed) = route(source);
    assert!(!routed, "f has no local destructuring of its own");
    crate::ssa::verify::verify_program(&result.expect("the retained route lowers"))
        .expect("program verifies");
}

#[test]
fn while_and_temporary_for_route_after_local_destructuring() {
    let pattern = "val (second, first) = Pair(20, 22)";
    for constructor in ["arrayOf", "listOf", "mutableListOf"] {
        let iteration = format!(
            "while (false) {{}}; for ((value, _) in {constructor}(Pair(20, 22))) {{ if (value == 20) {{ println(\"loop\") }} }}"
        );
        for (before, after) in [(pattern, iteration.as_str()), (&iteration, pattern)] {
            let source = format!(
                "value class Pair(val first: Int, val second: Int)\n\
                 fun f(): Int {{ {before}; {after}; return first }}"
            );
            let (result, routed) = route(&source);
            assert!(routed, "{source}\n{:?}", result.as_ref().err());
            crate::ssa::verify::verify_program(&result.expect("combined provider lowers"))
                .expect("verified program");
        }
    }
}

#[test]
fn while_local_destructuring_refine_invalidates_reassigned_nullable_view() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(flag: Boolean): Unit {
        val (second, first) = Pair(20, 22)
        var outer: String? = "first"
        if (outer != null) {
            while (flag && first == 20) {
                outer = "second"
                break
            }
        }
        for ((value, _) in listOf(Pair(20, 22))) {
            if (value == first) { println("loop") }
        }
        if (outer != null && outer == "second") { println("changed") }
    }
    "#;
    let (result, routed) = route(source);
    assert!(routed, "{:?}", result.as_ref().err());
    crate::ssa::verify::verify_program(&result.expect("nullable loop lowers"))
        .expect("verified program");
}

#[test]
fn while_and_temporary_for_keep_legacy_when_destructuring_is_not_reached() {
    let cases = [
        "fun f(): Unit { while (false) {}; for (item in listOf(\"kept\")) { println(item) } }",
        "fun sibling(): Int { val (second, first) = Pair(20, 22); return first }\n\
         fun f(): Unit { while (false) {}; for (item in listOf(\"kept\")) { println(item) } }",
        "fun f(): Unit { while (false) {}; for (item in listOf(\"kept\")) { println(item) }; return; val (second, first) = Pair(20, 22) }",
        "fun f(): Unit { val nested = { val (second, first) = Pair(20, 22); first }; while (false) {}; for (item in listOf(\"kept\")) { println(item) } }",
    ];
    for code in cases {
        let source = format!("value class Pair(val first: Int, val second: Int)\n{code}");
        let (result, routed) = route(&source);
        assert!(!routed, "{code}");
        crate::ssa::verify::verify_program(&result.expect("legacy route still lowers"))
            .expect("verified program");
    }
}

#[test]
fn constant_while_edges_cannot_lend_destructuring_route() {
    let pattern = "val (second, first) = Pair(20, 22)";
    for code in [
        format!(
            "while (false) {{ {pattern} }}; for (item in listOf(\"kept\")) {{ println(item) }}"
        ),
        format!("for (item in listOf(\"kept\")) {{ println(item) }}; while (true) {{}}; {pattern}"),
    ] {
        let source = format!(
            "value class Pair(val first: Int, val second: Int)\nfun f(): Unit {{ {code} }}"
        );
        let (result, routed) = route(&source);
        assert!(!routed, "{code}");
        let program = result.unwrap_or_else(|error| panic!("{code}: {error:?}"));
        crate::ssa::verify::verify_program(&program).expect("reachable loop edges verify");
    }
}

#[test]
fn terminating_constructor_prefix_lowers_on_the_retained_route() {
    let source = r#"
        class Plain(val first: Int, val second: Int)
        fun firstArgument(): Int { println("first argument"); return 1 }
        fun f(flag: Boolean): Unit {
            val result = Plain(firstArgument(), if (flag) {
                while (true) { return }
                1
            } else {
                while (true) { return }
                2
            })
        }
    "#;
    let (result, routed) = route(source);
    assert!(
        !routed,
        "the scalar-field construction stays on retained Unit"
    );
    crate::ssa::verify::verify_program(&result.expect("the abandoned constructor lowers"))
        .expect("both return edges and the evaluated prefix verify");
}

#[test]
fn bare_loop_without_break_cannot_lend_destructuring_route() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(own pair: Pair): Unit {
        while (false) {}
        for (item in listOf("kept")) { println(item) }
        loop {}
        val (second, first) = pair
    }
    "#;
    let (result, routed) = route(source);
    assert!(
        !routed,
        "the local destructuring is after a loop without an exit"
    );
    crate::ssa::verify::verify_program(&result.expect("legacy route still lowers"))
        .expect("verified program");
}

#[test]
fn named_sequential_for_shares_the_qualified_while_route() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(): Int {
        val (second, first) = Pair(20, 22)
        val xs = listOf(Pair(20, 22))
        while (false) {}
        for ((value, _) in xs) { if (value == first) { println("loop") } }
        return first
    }
    "#;
    let (result, routed) = route(source);
    assert!(routed, "{:?}", result.as_ref().err());
    crate::ssa::verify::verify_program(&result.expect("named provider lowers"))
        .expect("verified program");
}

#[test]
fn qualified_sequential_for_invalidates_reassigned_nullable_view() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(): Unit {
        val (second, first) = Pair(20, 22)
        var outer: String? = "first"
        if (outer != null) {
            while (false) {}
            for (item in listOf("kept")) { outer = "second" }
        }
        if (outer != null && outer == "second") { println("changed") }
    }
    "#;
    let (result, routed) = route(source);
    assert!(routed, "{:?}", result.as_ref().err());
    crate::ssa::verify::verify_program(&result.expect("sequential loop lowers"))
        .expect("verified program");
}

#[test]
fn local_destructuring_qualifies_while_root_reassignment_without_a_for() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(flag: Boolean): Unit {
        val (second, first) = Pair(20, 22)
        var outer: String? = "first"
        if (outer != null) {
            while (flag && first == 20) {
                outer = "second"
                break
            }
        }
        if (outer != null && outer == "second") { println("changed") }
    }
    "#;
    let (result, routed) = route(source);
    assert!(routed, "{:?}", result.as_ref().err());
    crate::ssa::verify::verify_program(&result.expect("standalone while lowers"))
        .expect("verified program");
}

#[test]
fn numeric_for_cannot_use_local_destructuring_to_retire_outer_proof() {
    let source = r#"
    value class Pair(val first: Int, val second: Int)
    fun f(own range: IntRange): Unit {
        val (second, first) = Pair(20, 22)
        var outer: String? = "first"
        if (outer != null) {
            while (false) {}
            for (item in range) { outer = "second" }
        }
        if (outer != null && outer == "second") { println("changed") }
    }
    "#;
    let (result, routed) = route(source);
    assert!(!routed, "numeric for must retain its own proof policy");
    let Err(error) = result else {
        panic!("retained lowering should reject the local split");
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}
