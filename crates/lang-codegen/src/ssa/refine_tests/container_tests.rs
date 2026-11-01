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
        val (text, code) = l
        println(text)
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
             val v = m.get(\"k\")\n\
             return if (v != null) v else 0\n\
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
    result.expect("inout and map functions succeed alongside unmigrated map.get function");
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
