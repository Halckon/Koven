//! Local destructuring and temporary sequential-source routing contracts.

use super::route;

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
