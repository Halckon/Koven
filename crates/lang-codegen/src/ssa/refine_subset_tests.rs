//! SPEC-0350 routing: which functions the refinement layer lowers, and that the choice
//! and its output are independent of input order.
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    LoweringError,
    model::Program,
    refine::observe_routing,
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

/// Lowers `source` (package `p`) from entry `f`; returns the lowering result and whether
/// `f` itself went through the refinement layer.
fn route(source: &str) -> (Result<Program, LoweringError>, bool) {
    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "p", "f");
    let (result, routed) = observe_routing(|| {
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
    });
    let f = routed.iter().any(|name| name.starts_with("koven.p.f.d"));
    (result.map(|(program, _)| program), f)
}

#[test]
fn nullable_functions_inside_the_subset_are_routed() {
    for source in [
        "fun f(own x: Int?): Int { if (x != null) { return x }; return 0 }",
        "fun f(c: Boolean): Boolean? = if (c) true else null",
        "fun f(own s: String?): Unit { if (s != null && s == \"a\") { println(s) } }",
        "class Node(val v: Int, val name: String)\n\
         fun f(own n: Node?): Int { if (n != null) { return n.v }; return 0 }",
        "class Node(val v: Int)\nfun g(c: Boolean): Node? = if (c) Node(1) else null\n\
         fun f(c: Boolean): Int { val n = g(c)!!; return n.v }",
    ] {
        let (result, routed) = route(source);
        assert!(routed, "{source}");
        result.unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Each source differs from the subset in one construct (or touches no nullable); the
/// function keeps the per-family path whatever that path then does with it.
#[test]
fn functions_outside_the_subset_keep_the_per_family_path() {
    for (reason, source) in [
        ("no nullable", "fun f(own x: Int): Int = x + 1"),
        (
            "when",
            "fun f(own x: Int?): Int = when { x == null -> 0\n else -> 1 }",
        ),
        (
            "break",
            "fun f(own x: Int?): Int { while (x != null) { break }; return 0 }",
        ),
        (
            "MoveOnly Borrow parameter",
            "fun f(s: String?): Int { if (s != null) { return 1 }; return 0 }",
        ),
        (
            "MoveOnly reassignment",
            "fun f(): Int { var s: String? = null; s = \"a\"; if (s != null) { return 1 }; return 0 }",
        ),
        (
            "MoveOnly null test outside a condition",
            "fun f(own s: String?): Boolean { val present = s != null; return present }",
        ),
        (
            "receiver call",
            "class Node(val v: Int) { fun get(): Int = v }\n\
             fun f(own n: Node?): Int { if (n != null) { return n.get() }; return 0 }",
        ),
        (
            "Box",
            "fun f(own b: Box<Int>?): Int { if (b != null) { return 1 }; return 0 }",
        ),
        (
            "generic class",
            "class Cell<T>(val item: T)\n\
             fun f(own c: Cell<Int>?): Int { if (c != null) { return 1 }; return 0 }",
        ),
        (
            "non-scalar class field",
            "class Bag(val items: List<Int>)\n\
             fun f(own b: Bag?): Int { if (b != null) { return 1 }; return 0 }",
        ),
    ] {
        let (_, routed) = route(source);
        assert!(!routed, "{reason}: {source}");
    }
}

#[test]
fn routing_and_refined_output_do_not_depend_on_input_order() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Node(val v: Int)\n\
         fun pick(c: Boolean): Int? = if (c) 7 else null\n\
         fun node(c: Boolean): Node? = if (c) Node(1) else null",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun main(): Unit {\n\
             val x = p.pick(true)\n\
             if (x != null && x == 7) { println(\"seven\") }\n\
             val n = p.node(true)!!\n\
             if (n.v == 1) { println(\"one\") }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let lower = |inputs: &[SourceUnitInput<'_>]| {
        let (names, typed, owned) = analyze(&sources, inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "q", "main");
        let (result, mut routed) = observe_routing(|| {
            lower_scalar_unit_with_entry(
                &sources,
                inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                entry,
            )
        });
        routed.sort();
        let (program, _) = result.expect("refined unit lowers and verifies");
        (render_program(&program), routed)
    };
    let (forward, forward_routed) = lower(&inputs);
    let (backward, backward_routed) = lower(&reversed);
    assert_eq!(forward, backward);
    assert_eq!(forward_routed, backward_routed);
    for function in ["koven.p.pick.d", "koven.p.node.d", "koven.q.main.d"] {
        assert!(
            forward_routed.iter().any(|name| name.starts_with(function)),
            "{function}: {forward_routed:?}"
        );
    }
}
