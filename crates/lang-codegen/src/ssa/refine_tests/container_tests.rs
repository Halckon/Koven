use super::*;

#[test]
fn sequential_containers_lower_in_refine() {
    let source = r#"class Tag(val n: Int)
    fun inspect(tag: Tag): Int = tag.n
    fun read_tag(items: List<Tag>, idx: Int): Int = inspect(items[idx])
    fun main(): Unit {
        val arr = arrayOf(1, 2, 3)
        arr[0] = 10
        var nums = mutableListOf(4, 5)
        nums.add(6)
        nums.insertAt(1, 20)
        val first = nums.removeFirst()
        val last = nums.removeLast()
        val mid = nums.removeAt(0)
        nums.clear()
        val tags = mutableListOf(Tag(7), Tag(8))
        tags[0] = Tag(9)
        val t = read_tag(listOf(Tag(11)), 0)
        if (arr.size == 3 && arr[0] == 10 && first == 4 && last == 6 && mid == 20 && nums.size == 0 && inspect(tags[0]) == 9 && t == 11) {
            println("containers-ok")
        }
    }"#;
    let (_, routed) = super::super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("sequential containers lower in ssa::refine")
    });
    for function in ["koven.p.main.d", "koven.p.read_tag.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(run(source), ("containers-ok\n".to_owned(), Some(0)));
}

#[test]
fn container_for_and_runtime_generate_lower_in_refine() {
    let source = r#"value class Pair(val first: Int, val second: String)
    fun sum_borrowed(xs: List<Int>, init: (Int) -> Int): Int {
        val gen = Array<Int>(xs.size, init)
        var acc = 0
        for (x in gen) {
            if (x == 2) { continue }
            if (x == 6) { break }
            acc = acc + x
        }
        return acc
    }
    fun early_return_pair(pairs: Array<Pair>, target: Int): Int {
        var seen = 0
        for ((n, label) in pairs) {
            if (n == 1) { continue }
            println(label)
            seen = seen + n
            if (n == target) { return seen }
            if (n == 9) { break }
        }
        return seen
    }
    fun main(): Unit {
        val base = 10
        val list = List<Int>(4, { i -> i + base })
        val total = sum_borrowed(list, { i -> i * 2 })
        val words = arrayOf("skip", "keep", "stop", "after")
        for (w in words) {
            if (w == "skip") { continue }
            println(w)
            if (w == "stop") { break }
        }
        val pairs1 = arrayOf(Pair(1, "skip-p"), Pair(3, "hit-ret"), Pair(9, "unreached"))
        val r1 = early_return_pair(pairs1, 3)
        val pairs2 = arrayOf(Pair(1, "skip-p"), Pair(4, "keep-p"), Pair(9, "hit-brk"), Pair(5, "after-brk"))
        val r2 = early_return_pair(pairs2, 99)
        if (total == 4 && r1 == 3 && r2 == 13) {
            println("for-gen-ok")
        }
    }"#;
    let (_, routed) = super::super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("container for and runtime generate lower in ssa::refine")
    });
    for function in [
        "koven.p.main.d",
        "koven.p.sum_borrowed.d",
        "koven.p.early_return_pair.d",
    ] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        run(source),
        (
            "keep\nstop\nhit-ret\nkeep-p\nhit-brk\nfor-gen-ok\n".to_owned(),
            Some(0)
        )
    );
}

#[test]
fn box_and_map_lower_in_refine() {
    let source = r#"value class Wrapped(val n: Int)
    enum class Expr { Num(item: Int), Add(left: Box<Expr>, right: Box<Expr>) }
    fun keep_expr(expr: Expr): Int = 1
    fun check_map(m: MutableMap<String, Int>, key: String): Boolean = m.contains(key)
    fun main(): Unit {
        val left = Box(Expr.Num(2))
        val right = Box(Expr.Num(3))
        val tree = Expr.Add(left, right)
        val ok_tree = keep_expr(tree)
        val b: Box<Wrapped>? = Box(Wrapped(42))
        val ok_box = if (b != null) 1 else 0
        var m = mutableMapOf<String, Int>()
        m.put("k1", 10)
        m.put("k2", 20)
        val has_k1 = check_map(m, "k1")
        if (ok_tree == 1 && ok_box == 1 && m.size == 2 && has_k1 && !m.contains("k3")) {
            println("box-map-ok")
        }
    }"#;
    let (_, routed) = super::super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("box and map lower in ssa::refine")
    });
    for function in [
        "koven.p.main.d",
        "koven.p.keep_expr.d",
        "koven.p.check_map.d",
    ] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(run(source), ("box-map-ok\n".to_owned(), Some(0)));
}

#[test]
fn class_fields_rc_and_smart_cast_receivers_lower_in_refine() {
    let source = r#"class Data(val n: Int) { fun get(): Int = n }
    class Bag(var items: List<Int>, val label: String, val shared: Rc<Data>)
    fun sum_list(xs: List<Int>): Int = xs.size
    fun use_rc(own r: Rc<Data>?, own s: String?, early: Boolean): Int {
        val has_s = s != null
        val shared = Rc("rc-text")
        println(shared.value)
        if (r != null) {
            val copy = r.share()
            if (early) {
                return copy.value.n + 100
            }
        }
        val n = if (r != null) r.value.n else 0
        return when (has_s) {
            true -> n + 10
            false -> n
        }
    }
    fun rc_loop_exits(own r: Rc<Data>?): Int {
        var i = 0
        var acc = 0
        while (i < 4) {
            i = i + 1
            if (r != null) {
                if (i == 1) { continue }
                acc = acc + r.value.n
                if (i == 3) { break }
            }
        }
        return acc
    }
    fun main(): Unit {
        var bag = Bag(listOf(1, 2, 3), "bag", Rc(Data(9)))
        val len1 = sum_list(bag.items)
        bag.items = listOf(4, 5)
        val len2 = sum_list(bag.items)
        val d: Data? = Data(7)
        val d_null: Data? = null
        val got1 = if (d != null) d.get() else 0
        val got2 = if (d_null != null) d_null.get() else 0
        val rc_res1 = use_rc(Rc(Data(5)), "present", false)
        val rc_res2 = use_rc(null, null, false)
        val rc_res3 = use_rc(Rc(Data(6)), "early", true)
        val rc_loop1 = rc_loop_exits(Rc(Data(4)))
        val rc_loop2 = rc_loop_exits(null)
        if (len1 == 3 && len2 == 2 && got1 == 7 && got2 == 0 && rc_res1 == 15 && rc_res2 == 0 && rc_res3 == 106 && rc_loop1 == 8 && rc_loop2 == 0) {
            println("class-rc-ok")
        }
    }"#;
    let (_, routed) = super::super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("class fields, Rc, and smart-cast receivers lower in ssa::refine")
    });
    for function in [
        "koven.p.main.d",
        "koven.p.use_rc.d",
        "koven.p.rc_loop_exits.d",
        "koven.p.sum_list.d",
    ] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        run(source),
        (
            "rc-text\nrc-text\nrc-text\nclass-rc-ok\n".to_owned(),
            Some(0)
        )
    );
}

#[test]
fn mid_construction_exits_and_pending_cleanup_lower_in_refine() {
    let source = r#"class Data(val n: Int)
    class Holder(val text: String, val inner: Data?, val tag: Int)
    value class Labeled(val label: String, val code: Int)
    enum class Expr { Num(item: Int), Add(left: Box<Expr>, right: Box<Expr>) }
    fun build_holder(exit_early: Boolean): Int {
        val named = "named-holder"
        val h1 = Holder(named, Data(5), if (exit_early) { return 11 } else { 22 })
        val h2 = Holder("temp-holder", Data(6), if (exit_early) { return 99 } else { h1.tag })
        return h2.tag
    }
    fun build_labeled(exit_early: Boolean): Int {
        val s = "pending-labeled"
        val l = Labeled(s, if (exit_early) { return 33 } else { 44 })
        val (code, label) = l
        println(label)
        return code
    }
    fun build_expr_loop(): Int {
        var i = 0
        var count = 0
        while (i < 3) {
            i = i + 1
            val left = Box(Expr.Num(i))
            val tree = Expr.Add(
                left,
                if (i == 1) {
                    continue
                } else if (i == 3) {
                    break
                } else {
                    Box(Expr.Num(10))
                }
            )
            count = count + 1
        }
        return count
    }
    fun main(): Unit {
        val h1 = build_holder(true)
        val h2 = build_holder(false)
        val l1 = build_labeled(true)
        val l2 = build_labeled(false)
        val c = build_expr_loop()
        val tag: Int? = if (h1 == 11 && h2 == 22 && l1 == 33 && l2 == 44 && c == 1) 1 else null
        if (tag != null) {
            println("mid-construct-ok")
        }
    }"#;
    let (_, routed) = super::super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("mid-construction exits and pending cleanup lower in ssa::refine")
    });
    for function in [
        "koven.p.main.d",
        "koven.p.build_holder.d",
        "koven.p.build_labeled.d",
        "koven.p.build_expr_loop.d",
    ] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        run(source),
        ("pending-labeled\nmid-construct-ok\n".to_owned(), Some(0))
    );
}

#[test]
fn map_cross_function_routing_is_isolated() {
    let mut sources = SourceMap::new();
    let (helper_id, helper_file) = parsed(
        &mut sources,
        "p/helper.ko",
        "package p\n\
         fun unmigrated_lookup(m: Map<String, Int>): Int {\n\
             var total = 0\n\
             for ((k, v) in m) { total = total + v }\n\
             return total\n\
         }",
    );
    let (main_id, main_file) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         fun bump_with_map(inout acc: Int, m: MutableMap<String, Int>): Unit {\n\
             if (m.contains(\"k\")) { acc = acc + m.size }\n\
         }\n\
         fun main(): Unit {\n\
             var m = mutableMapOf<String, Int>()\n\
             m.put(\"k\", 7)\n\
             var acc = 10\n\
             bump_with_map(&acc, m)\n\
             val ro = mapOf<String, Int>()\n\
             val looked = unmigrated_lookup(ro)\n\
             if (acc == 11 && looked == 0) { println(\"map-isolated-ok\") }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/helper.ko", helper_id, &helper_file),
        SourceUnitInput::new("root", "p/main.ko", main_id, &main_file),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "p", "main");
    let (result, routed) = super::super::refine::observe_routing(|| {
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
    result.expect("inout and map functions succeed alongside unmigrated map iteration function");
    for function in ["koven.p.main.d", "koven.p.bump_with_map.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function} should route to ssa::refine: {routed:?}"
        );
    }
    assert!(
        !routed
            .iter()
            .any(|name| name.starts_with("koven.p.unmigrated_lookup.d")),
        "unmigrated_lookup should stay on unit_lower: {routed:?}"
    );
}

#[test]
fn move_only_reassignment_and_move_enum_when_lower_in_refine() {
    let source = r#"enum class Reply<T> { Full(value: T, code: Int), Empty }
    fun inspect_reply(own item: Reply<String>): Int {
        when (item) {
            is Reply.Full -> println(item.value)
            is Reply.Empty -> println("empty")
        }
        return when (item) {
            is Reply.Full -> item.code
            is Reply.Empty -> 0
        }
    }
    fun tag_only(own item: Reply<String>): Int = when (item) {
        is Reply.Full -> 1
        else -> 0
    }
    fun main(): Unit {
        var s: String? = null
        s = "first"
        if (s != null) { println(s) }
        s = "second"
        if (s != null) { println(s) }
        val r1 = inspect_reply(Reply.Full("payload", 42))
        val r2 = inspect_reply(Reply.Empty)
        val t1 = tag_only(Reply.Full("unused", 9))
        val t2 = tag_only(Reply.Empty)
        if (r1 == 42 && r2 == 0 && t1 == 1 && t2 == 0) {
            println("reassign-enum-ok")
        }
    }"#;
    let (_, routed) = super::super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("MoveOnly reassignment and MoveOnly enum when lower in ssa::refine")
    });
    for function in [
        "koven.p.main.d",
        "koven.p.inspect_reply.d",
        "koven.p.tag_only.d",
    ] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        run(source),
        (
            "first\nsecond\npayload\nempty\nreassign-enum-ok\n".to_owned(),
            Some(0)
        )
    );
}

#[test]
fn deinit_nested_fields_box_payloads_and_move_receivers_lower_in_refine() {
    let source = r#"class Leaf(val name: String, val code: Int) {
        deinit() {
            println(this.name)
        }
    }
    class Guard(val first: Leaf, val second: Leaf) {
        deinit() {
            println(this.first.name)
            println(this.second.name)
        }
    }
    enum class Owned {
        Empty, Full(text: String);
        own fun score(): Int = 9
    }
    object Registry {
        fun ping(): Int = 7
    }
    interface Readable {
        fun read(message: String): Int = 1
    }
    class Reader: Readable {
        override fun read(message: String): Int = 7
    }
    class Host(val tag: Int, val delegate: Reader): Readable by delegate {}
    class Relay(val id: Int) {
        fun answer(): Int = id
        own fun relay(): Int = answer()
        own fun pass(): Relay = this
    }
    value class Wrapped(val n: Int)
    value class Entry(val label: String, val items: List<Int>)
    value class InlineCounter(var item: Int) {
        inout fun set(next: Int): Unit { item = next }
        inout fun read(): Int = item
    }
    value class InlineResource(val marker: Int, var item: String) {
        inout fun set(own next: String): Unit { item = next }
    }
    interface Finishable {
        own fun finish(): Int = 40
        own fun relay(): Int = finish()
    }
    class FinResource: Finishable {}
    class FieldHolder(var text: String, val tag: Int)
    fun bump_wrapped(inout w: Wrapped, delta: Int): Unit {
        w = Wrapped(w.n + delta)
    }
    fun clone_nested(g: Guard): String = g.first.name.clone()
    fun read_nested_code(g: Guard): Int = g.second.code
    fun main(): Unit {
        val b_entry: Box<Entry>? = Box(Entry("boxed", listOf(1, 2)))
        val b_owned: Box<Owned>? = Box(Owned.Full("boxed-enum"))
        val has_boxes = if (b_entry != null && b_owned != null) 1 else 0
        var w = Wrapped(10)
        bump_wrapped(&w, 5)
        val owned_score = (Owned.Full("payload")).score() + Owned.Empty.score()
        val obj_ping = (Registry).ping()
        val del_read = Host(0, Reader()).read("argument")
        val relayed = Relay(41).pass().relay()
        var m = mutableMapOf<String, String>()
        m.put("k", "map-val")
        val hit = m.withValue("k", { v -> println(v) })
        val g = Guard(Leaf("L1", 11), Leaf("L2", 22))
        val cloned = clone_nested(g)
        val code = read_nested_code(g)
        println(cloned)
        var ic = InlineCounter(1)
        ic.set(7)
        val ic_val = ic.read()
        var ir = InlineResource(1, "old-res")
        ir.set("new-res")
        val fin = FinResource().relay()
        val holder = FieldHolder("old-field", 9)
        val old_field = replace(&holder.text, "new-field")
        println(old_field)
        println(holder.text)
        if (has_boxes == 1 && w.n == 15 && owned_score == 18 && obj_ping == 7 && del_read == 7 && relayed == 41 && hit && code == 22 && ic_val == 7 && fin == 40) {
            println("deinit-nested-box-ok")
        }
    }"#;
    let (_, routed) = super::super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("deinit, nested fields, Box payloads, and MoveOnly receivers lower in ssa::refine")
    });
    for function in [
        "koven.p.main.d",
        "koven.p.Leaf.deinit.d",
        "koven.p.Guard.deinit.d",
        "koven.p.Owned.score.s",
        "koven.p.Relay.relay.s",
        "koven.p.Relay.pass.s",
        "koven.p.InlineCounter.set.s",
        "koven.p.InlineCounter.read.s",
        "koven.p.InlineResource.set.s",
        "koven.p.Finishable.finish.s",
        "koven.p.Finishable.relay.s",
        "koven.p.bump_wrapped.d",
        "koven.p.clone_nested.d",
        "koven.p.read_nested_code.d",
    ] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        run(source),
        (
            "map-val\nL1\nold-field\nnew-field\ndeinit-nested-box-ok\nL1\nL2\nL2\nL1\n".to_owned(),
            Some(0)
        )
    );
}

#[test]
fn map_ops_general_values_and_unit_root_primitives_lower_in_refine() {
    let source = r#"class Tracked(val tag: String, val code: Int) {
        deinit() {
            println(this.tag)
        }
    }
    value class Score(val pts: Int)
    fun make_unit(): Unit {
        println("unit-rhs")
    }
    fun quiet_unit(): Unit {}
    fun unit_primitives(early: Boolean): Int {
        var left = quiet_unit()
        var right = quiet_unit()
        val old_u = replace(&left, make_unit())
        swap(&left, &right)
        val prev = replace(&left, if (early) { return 77 } else { quiet_unit() })
        old_u
        prev
        return 88
    }
    fun inspect_readonly(m: Map<String, Tracked>): Int {
        val has_b = m.withValue("b", { item -> println(item.tag) })
        return if (has_b) m.size else 0
    }
    fun main(): Unit {
        var scores = mutableMapOf<String, Score>()
        scores["first"] = Score(10)
        scores.put("second", Score(20))
        val got_first = scores.get("first")
        val got_missing = scores["missing"]
        val first_ok = got_first != null && got_first.pts == 10
        val missing_ok = got_missing == null
        val removed_score = scores.remove("first")
        val rem_present = ((removed_score)) != null
        val rem_ok = rem_present && removed_score != null && removed_score.pts == 10

        var nums = mutableMapOf<String, Int>()
        nums["k"] = 42
        val eq_ok = nums.get("k") == 42 && nums["none"] == null

        var tracked = mutableMapOf<String, Tracked>()
        tracked["a"] = Tracked("drop-a1", 1)
        tracked["a"] = Tracked("drop-a2", 2)
        tracked.put("b", Tracked("drop-b", 3))
        val removed_a = tracked.remove("a")
        val removed_none = tracked.remove("none")
        val a_code = if (removed_a != null) removed_a.code else 0
        val none_ok = removed_none == null
        val frozen = tracked.consume()
        val ro_count = inspect_readonly(frozen)

        val u1 = unit_primitives(true)
        val u2 = unit_primitives(false)
        if (first_ok && missing_ok && rem_ok && eq_ok && a_code == 2 && none_ok && ro_count == 1 && u1 == 77 && u2 == 88) {
            println("map-unit-ops-ok")
        }
    }"#;
    let (_, routed) = super::super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("map ops, general map values, and unit root primitives lower in ssa::refine")
    });
    for function in [
        "koven.p.main.d",
        "koven.p.Tracked.deinit.d",
        "koven.p.unit_primitives.d",
        "koven.p.inspect_readonly.d",
    ] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        run(source),
        (
            "drop-a1\ndrop-b\nunit-rhs\nunit-rhs\nmap-unit-ops-ok\ndrop-b\ndrop-a2\n".to_owned(),
            Some(0)
        )
    );
}

#[test]
fn map_iteration_require_value_range_view_and_control_edges_lower_in_refine() {
    let source = r#"
    class Holder(var tag: String, val label: String)

    fun borrow_label(h: Holder): borrow String from h = h.label

    fun head_view(xs: List<Int>, n: Int): View<Int> from xs = rangeView(xs, 0, n)

    fun sum_view(v: View<Int>): Int {
        var acc = 0
        for (x in v) {
            acc = acc + x
        }
        return acc
    }

    fun operand_early_exits(flag: Boolean, inout marker: Int): Int {
        var xs = mutableListOf(1, 2)
        if (flag) {
            xs[if (flag) { marker = 11; return 91 } else 0] = 99
        }
        var m = mutableMapOf<String, Int>()
        m["a"] = if (!flag) { marker = 22; return 92 } else 1
        return xs.size + m.size
    }

    fun main(): Unit {
        val h = Holder("init", "header")
        val old_tag = replace(&h.tag, "updated")
        borrow val lbl = borrow_label(h)
        println(old_tag)
        println(lbl)

        var raw: MutableMap<String, Int>? = mutableMapOf<String, Int>()
        var m = raw!!
        m["a"] = 10
        m["b"] = 20
        m["c"] = 30
        var opt_m: MutableMap<String, Int>? = m
        var iter_sum = 0
        var req_val = 0
        if (opt_m != null) {
            val sz = opt_m.size
            val frozen = opt_m.consume()
            borrow val required_b = frozen.requireValue("b")
            req_val = required_b + sz
            for ((k, v) in frozen) {
                if (k == "a") {
                    continue
                }
                iter_sum = iter_sum + v
                if (k == "c") {
                    break
                }
            }
        }

        val xs = listOf(5, 15, 25, 35)
        borrow val v1 = head_view(xs, 3)
        borrow val v2 = v1
        val v_sum = sum_view(v2) + sum_view(rangeView(xs, 1, 3)) + v1.size
        var loop_view_sum = 0
        for (item in rangeView(xs, 0, 2)) {
            loop_view_sum = loop_view_sum + item
        }

        var outer: String? = "outer-before".clone()
        if (outer != null) {
            println(outer)
            var idx = 0
            while (idx < 2) {
                outer = "outer-after".clone()
                break
            }
            if (outer != null) {
                println(outer)
            }
        }

        var marker1 = 0
        var marker2 = 0
        val e1 = operand_early_exits(true, &marker1)
        val e2 = operand_early_exits(false, &marker2)
        if (req_val == 23 && iter_sum == 50 && v_sum == 88 && loop_view_sum == 20 && e1 == 91 && marker1 == 11 && e2 == 92 && marker2 == 22) {
            println("e3-items-123-ok")
        }
    }"#;
    let ((program, entry), routed) = super::super::refine::observe_routing(|| {
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
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("E3 items 1-3 lower in ssa::refine")
    });
    for function in [
        "koven.p.main.d",
        "koven.p.borrow_label.d",
        "koven.p.head_view.d",
        "koven.p.sum_view.d",
        "koven.p.operand_early_exits.d",
    ] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    let llvm =
        crate::llvm::render_verified_program_with_entry(&program, entry).expect("verified LLVM");
    let output = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 8);
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"init\nheader\nouter-before\nouter-after\ne3-items-123-ok\n",
    );
}
