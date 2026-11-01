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
            if (x == 1) { continue }
            if (x > 50) { break }
            acc = acc + x
        }
        return acc
    }
    fun main(): Unit {
        val base = 10
        val list = List<Int>(4, { i -> i + base })
        val total = sum_borrowed(list, { i -> i * 2 })
        val pairs = arrayOf(Pair(3, "a"), Pair(4, "b"))
        var psum = 0
        for ((n, label) in pairs) {
            println(label)
            psum = psum + n
        }
        if (total == 12 && psum == 7) {
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
    for function in ["koven.p.main.d", "koven.p.sum_borrowed.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(run(source), ("a\nb\nfor-gen-ok\n".to_owned(), Some(0)));
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
