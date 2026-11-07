//! SPEC-0350 routing: which functions the refinement layer lowers, and that the choice
//! and its output are independent of input order.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

use super::{
    LoweringError,
    model::{Operation, Program},
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
    let (name_environment, mut type_environment) = standard_environments();
    let _ = type_environment.authorize_range_source(&sources, id);
    let _ = type_environment.authorize_range_extension_source(&sources, id);
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
        "fun f(own s: String): Int = when (s) { \"a\" -> 1\n else -> 0 }",
        "fun f(s: String): Int = when ((s)) { (\"a\") -> 1\n else -> 0 }",
        "fun f(own x: Int?): Int { if (x != null) { return x }; return 0 }",
        "fun f(own x: Int?): Int = when { x == null -> 0\n else -> 1 }",
        "fun f(own x: Int?): Int { while (x != null) { break }; var i = 0\n\
         loop { i = i + 1; if (i > 2) break else continue }; return i }",
        "fun f(c: Boolean): Boolean? = if (c) true else null",
        "fun f(own s: String?): Unit { if (s != null && s == \"a\") { println(s) } }",
        "class Node(val v: Int, val name: String)\n\
         fun f(own n: Node?): Int { if (n != null) { return n.v }; return 0 }",
        "class Node(val v: Int)\nfun g(c: Boolean): Node? = if (c) Node(1) else null\n\
         fun f(c: Boolean): Int { val n = g(c)!!; return n.v }",
        "enum class Shape { Circle(r: Int), Point }\n\
         fun f(x: Shape): Int = when (x) { is Shape.Circle -> x.r\n Shape.Point -> 0 }",
        "enum class Shape { Circle(r: Int), Point }\n\
         fun f(): Boolean { val x = Shape.Circle(1); return x is Shape.Circle }",
        "fun f(inout x: Int): Unit { x = x + 1 }",
        "fun g(inout x: Int): Unit { x = 1 }\nfun f(inout x: Int): Unit { g(&x) }",
        "class Data(val n: Int)\nfun f(): Int { val a = Rc(Data(1)); val b = a.share(); return b.value.n }",
        "class Counter(var count: Int)\nfun f(): Int { var c = Counter(0); c.count = 5; return c.count }",
        "class Counter(var n: Int) { inout fun inc(): Unit { n = n + 1 } }\n\
         fun f(inout c: Counter): Unit { c.inc() }",
        "fun g(inout x: Int): Unit { x = 1 }\nfun f(): Int { var n = 0; g(&n); return n }",
        "fun f(own x: Int?): Int { val g = { 1 }; return 0 }",
        "value class Wrapped(val v: Int)\n\
         fun f(own b: Box<Wrapped>?): Int { if (b != null) { return 1 }; return 0 }",
        "class Cell<T>(val item: T)\n\
         fun f(own c: Cell<Int>?): Int { if (c != null) { return 1 }; return 0 }",
        "enum class Opt<T> { Some(v: T), None }\n\
         fun f(o: Opt<Int>): Int = when (o) { is Opt.Some -> 1\n else -> 0 }",
        "fun f(b: Boolean): Int = when (b) { true -> 1\n false -> 0 }",
        "fun f(own s: String?): Boolean { val present = s != null; return present }",
        "class Node(val v: Int) { fun get(): Int = v }\n\
         fun f(own n: Node?): Int { if (n != null) { return n.get() }; return 0 }",
        "fun f(): Int { val a = Rc(\"s\"); val b = a.share(); return 0 }",
        "class Data(val n: Int)\nfun f(own r: Rc<Data>?): Int { if (r != null) { return r.value.n }; return 0 }",
        "class Bag(val items: List<Int>)\n\
         fun f(own b: Bag?): Int { if (b != null) { return 1 }; return 0 }",
        "fun f(): Int { var s: String? = null; s = \"a\"; if (s != null) { return 1 }; return 0 }",
        "enum class Tag { Named(n: String), Empty }\n\
         fun f(own t: Tag): Int = when (t) { is Tag.Named -> 1\n else -> 0 }",
        "value class Entry(val label: String, val items: List<Int>)\n\
         fun f(own b: Box<Entry>?): Int { if (b != null) { return 1 }; return 0 }",
        "class Leaf(val name: String) { deinit() { println(this.name) } }\n\
         class Guard(val first: Leaf) { deinit() { println(this.first.name) } }\n\
         fun f(): Int { val g = Guard(Leaf(\"x\")); return 0 }",
        "enum class Owned { Empty, Full(text: String); own fun score(): Int = 9 }\n\
         fun f(): Int = Owned.Full(\"a\").score()",
        "value class Wrapped(val n: Int)\n\
         fun f(inout w: Wrapped): Unit { w = Wrapped(2) }",
        "object Registry { fun ping(): Int = 7 }\n\
         fun f(): Int = (Registry).ping()",
        "interface Readable { fun read(message: String): Int = 1 }\n\
         class Reader: Readable { override fun read(message: String): Int = 7 }\n\
         class Host(val tag: Int, val delegate: Reader): Readable by delegate {}\n\
         fun f(): Int = Host(0, Reader()).read(\"argument\")",
        "fun f(m: Map<String, String>): Boolean = m.withValue(\"k\", { v -> println(v) })",
        "value class Counter(var item: Int) { inout fun set(next: Int): Unit { item = next }; inout fun read(): Int = item }\n\
         fun f(): Int { var c = Counter(1); c.set(7); return c.read() }",
        "interface Finishable { own fun finish(): Int = 40; own fun relay(): Int = finish() }\n\
         class Resource: Finishable {}\n\
         fun f(): Int = Resource().relay()",
        "class Holder(var text: String, val tag: Int)\n\
         fun f(): String { val h = Holder(\"old\", 7); return replace(&h.text, \"new\") }",
        "fun f(m: Map<String, Int>): Int { val v = m.get(\"k\"); return if (v != null) v else 0 }",
        "fun f(): Int { var m = mutableMapOf<String, String>(); m[\"k\"] = \"v\"; val r = m.remove(\"k\"); val ro = m.consume(); return ro.size }",
        "fun make(): Unit {}\nfun f(): Unit { var a = make(); var b = make(); val old = replace(&a, make()); swap(&a, &b); old }",
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
        (
            "explicit String borrow-result binding",
            "fun view(s: String): borrow String from s = s\n\
             fun f(): Int { val owner = \"a\"; borrow val s = view(owner); return when (s) { \"a\" -> 1; else -> 0 } }",
        ),
        ("no nullable", "fun f(own x: Int): Int = x + 1"),
        (
            "when with a nullable subject",
            "fun f(own x: Int?): Int = when (x) { null -> 0\n else -> 1 }",
        ),
        (
            "jump out of a call argument past a borrowed temporary",
            "class Node()\nfun outer(node: Node, own r: Int): Int = r\n\
             fun f(flag: Boolean): Int = outer(Node(), when (flag) { true -> return 1\n else -> 0 })",
        ),
        (
            "MoveOnly Borrow parameter",
            "fun f(s: String?): Int { if (s != null) { return 1 }; return 0 }",
        ),
        (
            "MoveOnly Inout parameter",
            "fun f(inout s: String): Unit { s = \"a\" }",
        ),
        (
            "whole replacement of a MoveOnly Inout target",
            "class Counter(var n: Int)\nfun f(inout c: Counter): Unit { c = Counter(1) }",
        ),
        (
            "Borrow Rc parameter",
            "class Data(val n: Int)\nfun f(r: Rc<Data>): Int = r.value.n",
        ),
    ] {
        let (_, routed) = route(source);
        assert!(!routed, "{reason}: {source}");
    }
}

#[test]
fn non_nominal_box_payload_is_rejected_before_refinement() {
    // This original routing fixture is invalid under Guide 11. Keep its source as a
    // frontend rejection boundary instead of asking a validated lowering entry to accept it.
    let source = "fun f(own b: Box<Int>?): Int { if (b != null) { return 1 }; return 0 }";
    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_environment, type_environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("name resolution succeeds internally")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("type checking succeeds internally");
    let [diagnostic] = typed.signatures().diagnostics() else {
        panic!("one Box signature rejection: {:?}", typed.diagnostics())
    };
    assert_eq!(diagnostic.code().to_string(), "L0117");
    assert_eq!(
        sources.slice(diagnostic.primary_span()).expect("span"),
        "Int"
    );
    assert!(
        typed.validate().is_err(),
        "invalid Box cannot enter lowering"
    );
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

#[test]
fn outer_non_null_view_with_loop_reassignment_routes_to_legacy() {
    let source = r#"fun f(): Unit {
        var outer: String? = "first"
        if (outer != null) {
            var i = 0
            while (i < 2) {
                outer = "second"
                break
            }
        }
    }"#;
    let (res, routed) = route(source);
    assert!(
        !routed,
        "reassigning an outer-narrowed variable inside a loop must not route to refine"
    );
    assert!(res.is_ok());
}

#[test]
fn sequential_for_temporary_container_scope_has_field_replace_routing_pairs() {
    // Pair 1: Sequential for with temporary container but NO field replacement stays on unit_lower.
    let source_no_replace = r#"
    fun f(): Unit {
        for (x in arrayOf("a", "b")) {
            println(x)
        }
    }"#;
    let (res, routed) = route(source_no_replace);
    assert!(
        !routed,
        "temporary container without field replace remains on legacy lowering: {routed}"
    );
    assert!(res.is_ok());

    // Pair 2: Scope containing field replacement MUST route to refine (unit_lower retired field_replace).
    let source_with_replace = r#"
    class Holder(var text: String)
    fun f(): Unit {
        for (x in arrayOf("a", "b")) {
            println(x)
        }
        val h = Holder("init")
        val old = replace(&h.text, "new")
        println(old)
    }"#;
    let (res, routed) = route(source_with_replace);
    assert!(
        routed,
        "scope with field replace must route to refine: {routed}"
    );
    assert!(res.is_ok());

    // Pair 3: Unrelated field replace in sibling function does not pollute f's routing.
    let source_unrelated = r#"
    class Holder(var text: String)
    fun g(): Unit {
        val h = Holder("init")
        val old = replace(&h.text, "new")
        println(old)
    }
    fun f(): Unit {
        for (x in arrayOf("a", "b")) {
            println(x)
        }
    }"#;
    let (res, routed) = route(source_unrelated);
    assert!(
        !routed,
        "f has no field replace and must stay legacy even if sibling g has replace: {routed}"
    );
    assert!(res.is_ok());
}

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
fn call_has_field_replace_admission_pairs() {
    // Direct call with control transfer in argument tree:
    // When arguments involve field replace, it is admitted to refine.
    let source_replace_call = r#"
    class Holder(var a: String, var b: String)
    fun helper(first: String, second: String): Unit {
        println(first)
        println(second)
    }
    fun f(): Unit {
        val h = Holder("1", "2")
        helper(first = replace(&h.a, "10"), second = replace(&h.b, "20"))
    }"#;
    let (res, routed) = route(source_replace_call);
    assert!(
        routed,
        "call argument field replace must route to refine: {routed}"
    );
    assert!(res.is_ok());
}

#[test]
fn deinit_default_route_equivalent_to_per_family() {
    // Verifies that resource deinit constructs route to refine under default routing
    // and lower cleanly with zero leaks.
    let source = r#"
    class Resource(val name: String)
    fun f(own opt: Resource?): Unit {
        if (opt != null) {
            println(opt.name)
        }
    }"#;
    let (res, routed) = route(source);
    assert!(
        routed,
        "Resource deinit with nullable parameter routes to refine under default routing"
    );
    assert!(res.is_ok());
}

#[test]
fn map_iteration_require_value_range_view_and_control_edges_route_to_refine() {
    let source = r#"
    class Holder(var tag: String, val label: String)

    fun borrow_label(h: Holder): borrow String from h = h.label

    fun head_view(xs: List<Int>, n: Int): View<Int> from xs = rangeView(xs, 0, n)

    fun f(inout count: Int): Int {
        val h = Holder("init", "lbl")
        val old = replace(&h.tag, "next")
        println(old)
        borrow val lbl = borrow_label(h)
        println(lbl)

        var maybe_map: MutableMap<String, Int>? = mutableMapOf<String, Int>()
        if (maybe_map != null) {
            val frozen = maybe_map.consume()
            for ((k, v) in frozen) {
                if (v < 0) {
                    continue
                }
                count = count + v
            }
        }

        val xs = listOf(10, 20, 30)
        borrow val slice = head_view(xs, 2)
        for (item in slice) {
            count = count + item
        }

        var outer: String? = "before"
        if (outer != null) {
            println(outer)
            var i = 0
            while (i < 1) {
                outer = "after"
                break
            }
        }
        return slice.size
    }"#;
    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_environment, mut type_environment) = standard_environments();
    type_environment
        .authorize_range_source(&sources, id)
        .unwrap();
    type_environment
        .authorize_range_extension_source(&sources, id)
        .unwrap();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "p", "f");
    let (res, routed) = observe_routing(|| {
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
    assert!(
        routed.iter().any(|name| name.starts_with("koven.p.f.d")),
        "map iteration, rangeView, borrow_return/carrier_return, and loop view reassignment must route to refine: {routed:?}"
    );
    assert!(res.is_ok(), "{:?}", res.err());
}

#[test]
fn map_for_retired_scalar_families_choose_refine() {
    for source in [
        "fun f(m: Map<Int, Int>): Unit { for ((k, v) in m) { val x = v.inv() } }",
        "fun f(m: Map<Int, Int>): Unit { for ((k, v) in m) { val x = v.toLong() } }",
        "fun f(m: Map<Int, Int>): Unit { val range = 0..<3; for ((k, v) in m) { val x = v + 1 } }",
        "fun f(m: Map<Int, Int>): Unit { for ((k, v) in m) { val x = v + 1 }; val x = 2.inv() }",
    ] {
        let (result, routed) = route(source);
        assert!(
            routed,
            "retired scalar family must not fall back: {source}; result={:?}",
            result.as_ref().map(|_| ())
        );
        result.unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

#[test]
fn map_for_unrelated_scalar_scope_keeps_legacy_routing() {
    for sibling in [
        "",
        "fun other(n: Int): Int = n.inv()",
        "fun other(): IntRange = 0..<3",
    ] {
        let source = format!(
            "{sibling}\nfun f(m: Map<Int, Int>): Unit {{ for ((k, v) in m) {{ val x = v + 1 }} }}"
        );
        let (result, routed) = route(&source);
        assert!(
            !routed,
            "unrelated sibling must not expand Map-for routing: {source}"
        );
        result.expect("original Map-for addition remains supported");
    }
}

#[test]
fn enum_construction_for_borrowed_string_when_and_else_views_route_to_refine() {
    for source in [
        "enum class Shape { Circle(r: Int), Point }\n\
         fun f(): Shape = Shape.Circle(2)",
        "enum class Code { Bad(offset: Int), Other }\n\
         enum class Reply { Ok(text: String), Err(error: Code) }\n\
         fun f(own r: Reply): Int = when (r) {\n\
             is Reply.Ok -> 0\n\
             is Reply.Err -> {\n\
                 val c = r.error\n\
                 when (c) { is Code.Bad -> c.offset; else -> 1 }\n\
             }\n\
         }",
        "fun f(): Int {\n\
             var total = 0\n\
             val xs = listOf(\"a\".clone(), \"b\".clone())\n\
             for (item in xs) {\n\
                 total = total + when (item) { \"a\" -> 1; else -> 2 }\n\
             }\n\
             return total\n\
         }",
        "class Node(val v: Int)\n\
         fun make(): Node? = Node(7)\n\
         fun f(flag: Boolean, own probe: Node?, own n: Node?): Int {\n\
             val chosen = (when (flag) { true, false -> if (probe == null) { n } else { make() } })!!\n\
             return chosen.v\n\
         }",
        "fun observe(s: String): Unit { println(s) }\n\
         fun f(): Unit {\n\
             var m = mutableMapOf<Int, String>()\n\
             m.put(1, \"v\".clone())\n\
             val first = m.remove(1)\n\
             if ((first == null)) { println(\"none\") } else { observe(first) }\n\
         }",
    ] {
        let (res, routed) = route(source);
        assert!(routed, "expected refine routing for:\n{source}");
        res.unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

#[test]
fn deinit_borrow_heap_nullable_if_and_string_field_receiver_route_to_refine() {
    for source in [
        "class Inner(val text: String, val code: Int) { deinit() { println(this.text) } }\n\
         class Outer(val inner: Inner) {\n\
             deinit() {\n\
                 if (this.inner.code == 7) {\n\
                     println(this.inner.text.clone())\n\
                 }\n\
             }\n\
         }\n\
         fun f(): Int { val o = Outer(Inner(\"leaf\", 7)); return 0 }",
        "class Node(val text: String)\n\
         fun inspect(n: Node): Unit { println(n.text) }\n\
         fun f(own n: Node?): Unit {\n\
             if (n != null) { inspect(n) }\n\
         }",
        "class Node(val text: String)\n\
         fun inspect(n: Node): Unit { println(n.text) }\n\
         fun f(own n: Node?): Unit {\n\
             if (n == null) {} else { inspect(n) }\n\
         }",
    ] {
        let (res, routed) = route(source);
        assert!(routed, "expected refine routing for:\n{source}");
        res.unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

#[test]
fn inout_receiver_delegation_object_and_destructuring_route_to_refine() {
    for source in [
        "class Counter(var item: Int) {\n\
             inout fun set(own next: Int): Int {\n\
                 val ignored: Unit = (this).item = next\n\
                 return item\n\
             }\n\
         }\n\
         fun f(): Int {\n\
             var c = Counter(1)\n\
             return c.set(42)\n\
         }",
        "interface Readable { fun read(message: String): Int = 1 }\n\
         class Reader: Readable { override fun read(message: String): Int = 7 }\n\
         class Host(val tag: Int, val delegate: Reader): Readable by delegate {}\n\
         fun f(): Int {\n\
             val host = Host(0, Reader())\n\
             return host.read(\"arg\")\n\
         }",
        "object Registry { fun ping(): Int = 7 }\n\
         fun f(): Int = (Registry).ping()",
        "value class Pair(val a: Int, val b: String)\n\
         fun f(): Int {\n\
             val (b, a) = Pair(10, \"ok\")\n\
             println(b)\n\
             return a\n\
         }",
    ] {
        let (res, routed) = route(source);
        assert!(routed, "expected refine routing for:\n{source}");
        res.unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

#[test]
fn string_clone_value_field_borrow_box_and_when_route_to_refine() {
    for source in [
        "value class Tag(val label: String)\n\
         fun f(t: Tag): String = t.label.clone()",
        "fun f(): String {\n\
             val shared = Rc(\"rc\".clone())\n\
             return shared.value.clone()\n\
         }",
        "fun f(): String {\n\
             val xs = listOf(\"head\".clone(), \"tail\".clone())\n\
             return xs[1].clone()\n\
         }",
        "value class Packet(val label: String)\n\
         fun inspect(p: Packet): Int {\n\
             println(p.label)\n\
             return 7\n\
         }\n\
         fun f(): Int = inspect(Packet(\"msg\".clone()))",
        "enum class Node { Empty, Step(next: Box<Node>) }\n\
         fun f(): Node = Node.Step(Box(Node.Empty))",
        "fun f(x: Int): Int = when (x) {\n\
             1 -> 10\n\
             2, 3 -> 20\n\
             else -> 30\n\
         }",
    ] {
        let (res, routed) = route(source);
        assert!(routed, "expected refine routing for:\n{source}");
        res.unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

#[test]
fn numeric_for_runtime_container_and_map_diverging_keys_route_to_refine() {
    for source in [
        "fun f(r: IntRange): Int {\n\
             var sum = 0\n\
             for (i in r) { sum = sum + i }\n\
             return sum\n\
         }",
        "fun f(): Int {\n\
             val unused = Array<Int>(error(\"size\"), { index -> index })\n\
             return 0\n\
         }",
        "fun f(): Int {\n\
             val unused = List<Int>(1, error(\"initializer\"))\n\
             return 0\n\
         }",
        "fun f(): Int {\n\
             val scale = 7\n\
             return 0\n\
             val unused = Array<Int>(1, { index -> index + scale })\n\
         }",
        "fun f(): Unit {\n\
             var m = mutableMapOf<Int, Int>()\n\
             m.get(return)\n\
         }",
        "fun f(flag: Boolean): Unit {\n\
             var m = mutableMapOf<Int, Int>()\n\
             m.put(if (flag) { return } else { return }, 8)\n\
         }",
    ] {
        let (res, routed) = route(source);
        assert!(routed, "expected refine routing for:\n{source}");
        res.unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

#[test]
fn known_function_unit_container_and_nested_value_projection_route_to_refine() {
    for source in [
        "fun step(index: Int): Int = index + 1\n\
         fun apply(cb: (Int) -> Int): Int = cb(41)\n\
         fun f(): Int = apply(step)",
        "fun step(index: Int): Int = index + 2\n\
         fun f(): Int {\n\
             val xs = Array<Int>(2, step)\n\
             return xs[1]\n\
         }",
        "fun touch(index: Int): Unit {}\n\
         fun f(): Int {\n\
             val xs = Array<Unit>(2, touch)\n\
             return xs.size\n\
         }",
        "value class Inner(val code: Int)\n\
         value class Outer(val inner: Inner)\n\
         fun f(o: Outer): Int = o.inner.code",
    ] {
        let (res, routed) = route(source);
        assert!(routed, "expected refine routing for:\n{source}");
        res.unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

#[test]
fn range_view_borrow_return_and_borrow_val_route_to_refine() {
    for source in [
        "class Boxed(val text: String)\n\
         fun pick(source: Boxed): borrow String from source = source.text\n\
         fun f(): Unit {\n\
             val b = Boxed(\"hi\")\n\
             borrow val text = pick(b)\n\
             println(text)\n\
         }",
        "class Boxed(val text: String)\n\
         fun f(b: Boxed): Unit {\n\
             borrow val text = b.text\n\
             println(text)\n\
         }",
    ] {
        let (res, routed) = route(source);
        assert!(routed, "expected refine routing for:\n{source}");
        res.unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

#[test]
fn mixed_temporary_container_and_map_with_closures_route_to_refine() {
    for source in [
        "fun f(): Int {\n\
             val xs = listOf(10, 20)\n\
             val sz = listOf(1, 2, 3).size\n\
             return xs[0] + sz\n\
         }",
        "fun f(): Int {\n\
             val v = mutableMapOf<Int, Int>().get(0)\n\
             return if (v == null) { 0 } else { v }\n\
         }",
        "fun f(): Int {\n\
             val factor = 3\n\
             val calc: (Int) -> Int = { x -> x * factor }\n\
             val xs = mutableListOf(1, 2)\n\
             xs[0] = calc(4)\n\
             return xs[0]\n\
         }",
        "fun f(): Unit {\n\
             var m = mutableMapOf<String, Int>()\n\
             for (w in arrayOf(\"a\", \"b\", \"a\")) {\n\
                 if (w == \"b\") { continue }\n\
                 val prev = m.get(w)\n\
                 m.put(w.clone(), if (prev == null) { 1 } else { prev + 1 })\n\
             }\n\
         }",
        "fun f(): Unit {\n\
             var xs = mutableListOf<String>()\n\
             for (a in arrayOf(\"x\", \"y\")) { xs.add(a.clone()) }\n\
             for (s in xs) { println(s) }\n\
         }",
        "fun f(): Int {\n\
             var m = mutableMapOf<String, Int>()\n\
             for (w in arrayOf(\"x\", \"y\")) {\n\
                 val prev = m.get(w)\n\
                 m.put(w.clone(), if (prev == null) { 1 } else { prev + 1 })\n\
             }\n\
             var best = 0\n\
             for ((k, v) in m) { if (v > best) { best = v } }\n\
             return best\n\
         }",
    ] {
        let (res, routed) = route(source);
        assert!(routed, "expected refine routing for:\n{source}");
        res.unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

#[test]
fn container_append_admission_does_not_perturb_or_miscompile_sibling_functions() {
    let unrelated_append =
        "fun unrelated_append(): Unit {\n    var xs = mutableListOf<Int>()\n    xs.add(99)\n}\n";
    let cases = [
        // 1. while loop
        "fun f(n: Int): Int {\n    var i = 0\n    var sum = 0\n    while (i < n) {\n        sum = sum + i\n        i = i + 1\n    }\n    return sum\n}",
        // 2. sequential iteration
        "fun f(xs: List<Int>): Int {\n    var sum = 0\n    for (x in xs) {\n        sum = sum + x\n    }\n    return sum\n}",
        // 3. borrow-return
        "fun f(source: String): borrow String from source = source",
        // 4. carrier-return
        "fun f(xs: List<Int>): View<Int> from xs = rangeView(xs, 0, 1)",
        // 5. Map remove
        "fun f(inout m: MutableMap<Int, Int>): Int? = m.remove(1)",
    ];
    for source in cases {
        let (res_without, _) = route(source);
        let with_source = format!("{unrelated_append}{source}");
        let (res_with, _) = route(&with_source);
        match (res_without, res_with) {
            (Ok(prog_without), Ok(prog_with)) => {
                crate::ssa::verify::verify_program(&prog_without)
                    .expect("without-append program must verify");
                crate::ssa::verify::verify_program(&prog_with)
                    .expect("with-append program must verify");

                let f_without = prog_without.modules[0]
                    .functions
                    .iter()
                    .find(|func| func.name.starts_with("koven.p.f."))
                    .expect("f found");
                let f_with = prog_with.modules[0]
                    .functions
                    .iter()
                    .find(|func| func.name.starts_with("koven.p.f."))
                    .expect("f found with sibling");
                assert_eq!(
                    f_without.blocks[0].parameters.len(),
                    f_with.blocks[0].parameters.len()
                );
                let kind_without = prog_without.modules[0].type_kind(f_without.return_types[0]);
                let kind_with = prog_with.modules[0].type_kind(f_with.return_types[0]);
                assert_eq!(format!("{kind_without:?}"), format!("{kind_with:?}"));
                let drops_without = f_without
                    .instructions
                    .iter()
                    .filter(|inst| matches!(inst.operation, Operation::Drop { .. }))
                    .count();
                let drops_with = f_with
                    .instructions
                    .iter()
                    .filter(|inst| matches!(inst.operation, Operation::Drop { .. }))
                    .count();
                assert_eq!(drops_without, drops_with, "drop counts must match");
            }
            (Err(err_without), Err(err_with)) => {
                assert_eq!(err_without.kind, err_with.kind, "rejection kind must match");
            }
            (Ok(_), Err(err)) => {
                panic!("perturbation detected: without succeeded, but with failed: {err:?}");
            }
            (Err(err), Ok(_)) => {
                panic!("perturbation detected: without failed ({err:?}), but with succeeded");
            }
        }
    }
}

#[test]
fn temporary_container_size_and_map_get_single_eval_and_drop_points() {
    let list_size_source = "fun f(): Int {\n\
        val sz = listOf(\"alpha\".clone(), \"beta\".clone()).size\n\
        return sz\n\
    }";
    let (res_list, routed_list) = route(list_size_source);
    assert!(routed_list, "temporary list size routes to refine");
    let prog_list = res_list.expect("temporary list size lowers");
    crate::ssa::verify::verify_program(&prog_list).expect("program verifies");
    let func = prog_list.modules[0]
        .functions
        .iter()
        .find(|f| f.name.starts_with("koven.p.f."))
        .expect("f found");
    let drop_count = func
        .instructions
        .iter()
        .filter(|inst| matches!(inst.operation, Operation::Drop { .. }))
        .count();
    assert!(
        drop_count >= 3,
        "temporary list and elements must emit drops: got {drop_count}"
    );

    let map_get_source = "fun f(): Int {\n\
        val v = mutableMapOf<String, Int>().get(\"k\".clone())\n\
        return if (v == null) 0 else 1\n\
    }";
    let (res_map, routed_map) = route(map_get_source);
    assert!(routed_map, "temporary map get routes to refine");
    let prog_map = res_map.expect("temporary map get lowers");
    crate::ssa::verify::verify_program(&prog_map).expect("program verifies");
    let map_func = prog_map.modules[0]
        .functions
        .iter()
        .find(|f| f.name.starts_with("koven.p.f."))
        .expect("f found");
    let map_drops = map_func
        .instructions
        .iter()
        .filter(|inst| matches!(inst.operation, Operation::Drop { .. }))
        .count();
    assert!(
        map_drops >= 2,
        "temporary map and query key must emit drops: got {map_drops}"
    );
}

#[test]
fn string_borrow_capture_closure_preserves_outer_owner() {
    let source = "fun f(): Unit {\n\
        val s = \"hello_world\".clone()\n\
        val action: () -> Unit = { println(s) }\n\
        action()\n\
        println(s)\n\
    }";
    let (res, routed) = route(source);
    assert!(routed, "string borrow capture routes to refine");
    let prog = res.expect("string borrow capture lowers successfully");
    crate::ssa::verify::verify_program(&prog).expect("program verifies");
    let func = prog.modules[0]
        .functions
        .iter()
        .find(|f| f.name.starts_with("koven.p.f."))
        .expect("f found");
    let drops = func
        .instructions
        .iter()
        .filter(|inst| matches!(inst.operation, Operation::Drop { .. }))
        .count();
    assert!(
        drops >= 1,
        "outer string and closure must emit drops: got {drops}"
    );
}
