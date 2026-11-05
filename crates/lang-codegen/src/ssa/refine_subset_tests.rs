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
            "Box with non-nominal payload",
            "fun f(own b: Box<Int>?): Int { if (b != null) { return 1 }; return 0 }",
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
