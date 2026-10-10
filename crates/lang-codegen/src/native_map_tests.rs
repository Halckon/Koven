//! SPEC-0288 Map/MutableMap 原生可执行文件运行测试。

use super::emit_link_and_run;

#[test]
fn native_map_owned_remove_strings_and_inline_values_have_conditional_drops() {
    let source = r#"
value class Text(val text: String)
fun entry(): Unit {
 var strings = mutableMapOf<String, String>()
 strings.put("key".clone(), "value".clone())
 val key = "key".clone()
 val removed = strings.remove(key)
 if (removed != null) { println("string present") }
 val taken = removed!!
 println(taken)
 strings.remove(key)
 strings.put("unused".clone(), "unused value".clone())
 val unused = strings.remove("unused")
 var texts = mutableMapOf<Int, Text>()
 texts.put(1, Text("inline".clone()))
 val inline = texts.remove(1)!!
 println("inline taken")
 texts.remove(1)
 texts.put(2, Text("unused inline".clone()))
 val unusedInline = texts.remove(2)
 println(key)
}
"#;
    let llvm = super::boxed_enum_tests::lower_to_llvm("map_owned_remove.ko", source);
    let run = super::boxed_enum_tests::run_counted_allocations(&llvm, 9);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"string present\nvalue\ninline taken\nkey\n");
}

#[test]
fn native_map_owned_remove_resource_preserves_lexical_cleanup() {
    let source = r#"
class Resource(val n: Int) {
 deinit() { if (n == 1) { println("one") } else { println("two") } }
}


fun entry(): Unit {
 var m = mutableMapOf<Int, Resource>()
 m.put(1, Resource(1))
 val removed = m.remove(1)
 m.put(2, Resource(2))
 val missing = m.remove(99)
 println("before block exit")
}
"#;
    let llvm = super::boxed_enum_tests::lower_to_llvm("map_resource_remove.ko", source);
    let run = super::boxed_enum_tests::run_counted_allocations(&llvm, 3);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"before block exit\none\ntwo\n");
}

#[test]
fn native_map_owned_remove_missing_assertion_aborts() {
    let run = emit_link_and_run(
        "map_owned_missing.ko",
        "fun main(): Unit {\n var m = mutableMapOf<Int, String>()\n val absent = m.remove(1)!!\n println(absent)\n}",
        "main",
    );
    assert!(!run.status.success(), "{run:?}");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(run.status.signal(), Some(6));
    }
}

#[test]
fn native_map_put_preserves_copyable_key_and_value_bindings() {
    let text = r#"
value class Count(val n: Int)
fun main(): Unit {
 var numbers = mutableMapOf<Int, Int>()
 val key = 1
 val value = 0
 numbers.put(key, value)
 if (numbers[key] == value && key == 1) { println("put reused") }
 numbers[key] = value
 if (numbers[key]!! == value) { println("assignment reused") }
 var counts = mutableMapOf<Int, Count>()
 val count = Count(0)
 counts.put(key, count)
 val queried = counts[key]!!
 if (count.n == 0 && queried.n == 0) { println("aggregate reused") }
}
"#;
    let run = emit_link_and_run("map_copyable_put_bindings.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"put reused\nassignment reused\naggregate reused\n"
    );
}

#[test]
fn native_map_copyable_queries_preserve_missing_zero_and_false() {
    let text = r#"
fun main(): Unit {
 var numbers = mutableMapOf<String, Int>()
 numbers.put("zero".clone(), 0)
 val key = "zero".clone()
 val hit = numbers[key]
 val missing = numbers.get("absent")
 val otherMissing = numbers["other"]
 if (hit != null && hit == 0 && missing == null) { println("zero present") }
 if (missing == otherMissing && hit != missing && 0 == hit) { println("nullable equality") }
 if (hit != null) { if (hit + 1 == 1) { println("narrowed") } }
 if (hit!! == 0 && hit!! == 0) { println("copy assertion") }
 if (numbers.remove(key) == 0 && numbers.remove(key) == null) { println("remove") }
 println(key)
 var flags = mutableMapOf<Boolean, Boolean>()
 flags.put(true, false)
 val flag = flags[true]
 val absent = flags[false]
 val assertedFlag = flags[true]!!
 if (flag != null && flag == false && !assertedFlag && absent == null) { println("false present") }
}
"#;
    let run = emit_link_and_run("map_copyable_nullable.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"zero present\nnullable equality\nnarrowed\ncopy assertion\nremove\nzero\nfalse present\n"
    );
}

#[test]
fn native_map_copyable_value_class_queries_keep_generic_payload_layout() {
    let text = r#"
value class Count(val n: Int)
fun main(): Unit {
 var m = mutableMapOf<Int, Count>()
 m.put(1, Count(0))
 val result = m[1]
 if (m[2] == null && result != null) { println("present count") }
 val payload = result!!
 if (payload.n == 0) { println("payload zero") }
 val removed = m.remove(1)
 val taken = removed!!
 if (taken.n == 0 && m[1] == null) { println("removed count") }
}
"#;
    let run = emit_link_and_run("map_value_class_nullable.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"present count\npayload zero\nremoved count\n");
}

#[test]
fn native_map_copyable_missing_assertion_aborts() {
    let run = emit_link_and_run(
        "map_missing_assertion.ko",
        "fun main(): Unit {\n val m = mapOf<Int, Int>()\n m[1]!!\n}",
        "main",
    );
    assert!(!run.status.success(), "{run:?}");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(run.status.signal(), Some(6));
    }
}

#[test]
fn native_map_borrows_reusable_string_query_key() {
    let text = "fun check(m: Map<String, Int>, key: String): Unit {\n m.contains(key)\n m.contains(key)\n println(key)\n}\nfun main(): Unit {\n val m = mapOf<String, Int>()\n val key = \"query\".clone()\n check(m, key)\n println(key)\n}";
    let run = emit_link_and_run("map_key_borrow.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"query\nquery\n");
}

#[test]
fn native_map_drops_move_only_entries_on_overwrite_and_scope_exit() {
    let text = "class Resource(val n: Int) {\n deinit() { if (n == 1) { println(\"1\") }\n if (n == 2) { println(\"2\") }\n if (n == 3) { println(\"3\") } }\n}\nfun main(): Unit {\n var m = mutableMapOf<String, Resource>()\n m.put(\"a\".clone(), Resource(1))\n m.put(\"a\".clone(), Resource(2))\n m.put(\"b\".clone(), Resource(3))\n}";
    let run = emit_link_and_run("map_drop.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    let output = String::from_utf8(run.stdout).unwrap();
    assert!(
        output.starts_with("1\n"),
        "overwritten value must drop at replacement: {output}"
    );
    let mut lines = output.lines().collect::<Vec<_>>();
    lines.sort();
    assert_eq!(lines, ["1", "2", "3"]);
    let counted_source = text.replace("fun main()", "fun entry()");
    let llvm = super::boxed_enum_tests::lower_to_llvm("map_drop.ko", &counted_source);
    let counted = super::boxed_enum_tests::run_counted_allocations(&llvm, 7);
    assert!(counted.status.success(), "{counted:?}");
}

#[test]
fn native_map_string_values_drop_without_copying() {
    let text = "fun entry(): Unit {\n var m = mutableMapOf<String, String>()\n m.put(\"key\".clone(), \"first\".clone())\n m.put(\"key\".clone(), \"second\".clone())\n if (m.contains(\"key\")) { println(\"present\") }\n}";
    let llvm = super::boxed_enum_tests::lower_to_llvm("map_string_values.ko", text);
    let counted = super::boxed_enum_tests::run_counted_allocations(&llvm, 5);
    assert!(counted.status.success(), "{counted:?}");
    assert_eq!(counted.stdout, b"present\n");
}

#[test]
fn native_map_remove_delivers_owned_value_and_preserves_colliding_entry() {
    let text = "class Payload(val n: Int)\nfun entry(): Unit {\n var m = mutableMapOf<Int, Payload>()\n m.put(1, Payload(1))\n m.put(17, Payload(17))\n val removed = m.remove(1)\n if (removed != null) { println(\"removed\") }\n if (m.contains(17)) { println(\"collision survives\") }\n m.put(1, Payload(2))\n if (m.size == 2) { println(\"tombstone reused\") }\n m.remove(99)\n}";
    let llvm = super::boxed_enum_tests::lower_to_llvm("map_remove_owner.ko", text);
    let counted = super::boxed_enum_tests::run_counted_allocations(&llvm, 4);
    assert!(counted.status.success(), "{counted:?}");
    assert_eq!(
        counted.stdout,
        b"removed\ncollision survives\ntombstone reused\n"
    );
}

#[test]
fn native_map_churn_reclaims_deleted_slots_without_hanging() {
    let mut text = String::from(
        "class Payload {}\nfun entry(): Unit {\n var m = mutableMapOf<Int, Payload>()\n",
    );
    for key in 0..32 {
        text.push_str(&format!(" m.put({key}, Payload())\n m.remove({key})\n"));
    }
    text.push_str(" if (m.size == 0) { println(\"empty\") }\n if (!m.contains(99)) { println(\"missing\") }\n}");
    let llvm = super::boxed_enum_tests::lower_to_llvm("map_tombstones.ko", &text);
    // 32 payloads, the initial buffer and one rehash at 12 used/deleted slots.
    let counted = super::boxed_enum_tests::run_counted_allocations(&llvm, 34);
    assert!(counted.status.success(), "{counted:?}");
    assert_eq!(counted.stdout, b"empty\nmissing\n");
}

#[test]
fn native_map_reused_tombstones_do_not_trigger_spurious_growth() {
    let mut text = String::from(
        "class Payload {}\nfun entry(): Unit {\n var m = mutableMapOf<Int, Payload>()\n",
    );
    for _ in 0..32 {
        text.push_str(" m.put(1, Payload())\n m.remove(1)\n");
    }
    text.push_str("}\n");
    let llvm = super::boxed_enum_tests::lower_to_llvm("map_repeated_tombstone.ko", &text);
    let counted = super::boxed_enum_tests::run_counted_allocations(&llvm, 33);
    assert!(counted.status.success(), "{counted:?}");
}

#[test]
fn native_map_growth_moves_owners_without_dropping_entries() {
    let mut text = String::from(
        "class Payload {}\nfun entry(): Unit {\n var m = mutableMapOf<Int, Payload>()\n",
    );
    for key in 0..20 {
        text.push_str(&format!(" m.put({key}, Payload())\n"));
    }
    for key in 0..20 {
        text.push_str(&format!(
            " if (m.contains({key})) {{ println(\"present\") }}\n"
        ));
    }
    text.push_str(" if (m.size == 20) { println(\"size 20\") }\n}");
    let llvm = super::boxed_enum_tests::lower_to_llvm("map_growth_owners.ko", &text);
    let counted = super::boxed_enum_tests::run_counted_allocations(&llvm, 22);
    assert!(counted.status.success(), "{counted:?}");
    assert_eq!(
        counted.stdout,
        format!("{}size 20\n", "present\n".repeat(20)).as_bytes()
    );
}

#[test]
fn native_map_boolean_and_char_keys_use_exact_equality() {
    let text = "const val ASCII: Char = 'a'\nconst val UNICODE: Char = '中'\nconst val ABSENT: Char = 'b'\nfun main(): Unit {\n var flags = mutableMapOf<Boolean, Int>()\n flags.put(true, 1)\n flags.put(false, 2)\n if (flags.size == 2 && flags.contains(true) && flags.contains(false)) { println(\"flags\") }\n var chars = mutableMapOf<Char, String>()\n chars.put(ASCII, \"ascii\".clone())\n chars.put(UNICODE, \"unicode\".clone())\n if (chars.size == 2 && chars.contains(UNICODE) && !chars.contains(ABSENT)) { println(\"chars\") }\n}";
    let run = emit_link_and_run("map_scalar_keys.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"flags\nchars\n");
}

#[test]
fn native_map_short_circuit_queries_clean_up_both_exits() {
    let mut text = String::new();
    for (name, condition, marker) in [
        ("andSkip", "m.contains(2) && m.contains(1)", "unexpected"),
        ("andEvaluate", "m.contains(1) && m.contains(1)", "and"),
        ("orSkip", "m.contains(1) || m.contains(2)", "or skip"),
        (
            "orEvaluate",
            "m.contains(2) || m.contains(1)",
            "or evaluate",
        ),
    ] {
        text.push_str(&format!("fun {name}(): Unit {{\n var m = mutableMapOf<Int, String>()\n m.put(1, \"value\".clone())\n if ({condition}) {{ println(\"{marker}\") }}\n}}\n"));
    }
    text.push_str("fun entry(): Unit {\n andSkip()\n andEvaluate()\n orSkip()\n orEvaluate()\n}");
    let llvm = super::boxed_enum_tests::lower_to_llvm("map_short_circuit.ko", &text);
    let counted = super::boxed_enum_tests::run_counted_allocations(&llvm, 8);
    assert!(counted.status.success(), "{counted:?}");
    assert_eq!(counted.stdout, b"and\nor skip\nor evaluate\n");
}

#[test]
fn native_map_empty_construction_and_size() {
    let text = r#"
        fun main(): Unit {
            val m: Map<Int, Int> = mapOf<Int, Int>()
            if (m.size == 0) {
                println("size 0")
            }
            if (!m.contains(42)) {
                println("not contains")
            }
        }
    "#;
    let run = emit_link_and_run("map_empty.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    let stdout = String::from_utf8(run.stdout).expect("utf8 stdout");
    assert_eq!(stdout, "size 0\nnot contains\n");
}

#[test]
fn native_mutable_map_put_and_lookup() {
    let text = r#"
        fun main(): Unit {
            var mm: MutableMap<Int, Int> = mutableMapOf<Int, Int>()
            mm.put(1, 100)
            mm.put(2, 200)
            if (mm.size == 2) {
                println("size 2")
            }
            if (mm.contains(1)) {
                println("has 1")
            }
            if (!mm.contains(99)) {
                println("no 99")
            }
            val v1: Int? = mm[1]
            if (v1 == 100) {
                println("val1 100")
            }
            mm[3] = 300
            if (mm.size == 3) {
                println("size 3")
            }
            val v3: Int? = mm[3]
            if (v3 == 300) {
                println("val3 300")
            }
        }
    "#;
    let run = emit_link_and_run("map_put.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    let stdout = String::from_utf8(run.stdout).expect("utf8 stdout");
    assert_eq!(stdout, "size 2\nhas 1\nno 99\nval1 100\nsize 3\nval3 300\n");
}

#[test]
fn native_mutable_map_overwrite_key() {
    let text = r#"
        fun main(): Unit {
            var mm: MutableMap<Int, Int> = mutableMapOf<Int, Int>()
            mm.put(1, 10)
            mm.put(1, 20)
            if (mm.size == 1) {
                println("size 1")
            }
            val v: Int? = mm[1]
            if (v == 20) {
                println("val 20")
            }
        }
    "#;
    let run = emit_link_and_run("map_overwrite.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    let stdout = String::from_utf8(run.stdout).expect("utf8 stdout");
    assert_eq!(stdout, "size 1\nval 20\n");
}

#[test]
fn native_mutable_map_remove_key() {
    let text = r#"
        fun main(): Unit {
            var mm: MutableMap<Int, Int> = mutableMapOf<Int, Int>()
            mm.put(1, 10)
            mm.put(2, 20)
            mm.remove(1)
            if (mm.size == 1) {
                println("size 1 after rem")
            }
            if (!mm.contains(1)) {
                println("no 1")
            }
            if (mm.contains(2)) {
                println("has 2")
            }
            // 墓碑复用：重新插入 1
            mm.put(1, 100)
            if (mm.size == 2) {
                println("size 2 re-insert")
            }
            val v1: Int? = mm[1]
            if (v1 == 100) {
                println("val1 100")
            }
        }
    "#;
    let run = emit_link_and_run("map_remove.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    let stdout = String::from_utf8(run.stdout).expect("utf8 stdout");
    assert_eq!(
        stdout,
        "size 1 after rem\nno 1\nhas 2\nsize 2 re-insert\nval1 100\n"
    );
}

#[test]
fn native_mutable_map_growth_and_rehash() {
    let text = r#"
        fun main(): Unit {
            var mm: MutableMap<Int, Int> = mutableMapOf<Int, Int>()
            // 插入 20 个条目以触发超过 16 初始容量的扩容与重哈希
            mm.put(0, 0)
            mm.put(1, 10)
            mm.put(2, 20)
            mm.put(3, 30)
            mm.put(4, 40)
            mm.put(5, 50)
            mm.put(6, 60)
            mm.put(7, 70)
            mm.put(8, 80)
            mm.put(9, 90)
            mm.put(10, 100)
            mm.put(11, 110)
            mm.put(12, 120)
            mm.put(13, 130)
            mm.put(14, 140)
            mm.put(15, 150)
            mm.put(16, 160)
            mm.put(17, 170)
            mm.put(18, 180)
            mm.put(19, 190)

            if (mm.size == 20) {
                println("size 20")
            }
            val v0: Int? = mm[0]
            val v10: Int? = mm[10]
            val v19: Int? = mm[19]
            if (v0 == 0) {
                println("v0 ok")
            }
            if (v10 == 100) {
                println("v10 ok")
            }
            if (v19 == 190) {
                println("v19 ok")
            }
        }
    "#;
    let run = emit_link_and_run("map_grow.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    let stdout = String::from_utf8(run.stdout).expect("utf8 stdout");
    assert_eq!(stdout, "size 20\nv0 ok\nv10 ok\nv19 ok\n");
}

#[test]
fn native_mutable_map_string_keys() {
    let text = r#"
        fun main(): Unit {
            var sm: MutableMap<String, Int> = mutableMapOf<String, Int>()
            sm.put("hello", 1)
            sm.put("world", 2)
            if (sm.size == 2) {
                println("str size 2")
            }
            if (sm.contains("hello")) {
                println("has hello")
            }
            if (!sm.contains("foo")) {
                println("no foo")
            }
            val h: Int? = sm["hello"]
            if (h == 1) {
                println("hello is 1")
            }
            sm.remove("hello")
            if (!sm.contains("hello")) {
                println("hello removed")
            }
            if (sm.size == 1) {
                println("str size 1")
            }
        }
    "#;
    let run = emit_link_and_run("map_string.ko", text, "main");
    assert!(run.status.success(), "{run:?}");
    let stdout = String::from_utf8(run.stdout).expect("utf8 stdout");
    assert_eq!(
        stdout,
        "str size 2\nhas hello\nno foo\nhello is 1\nhello removed\nstr size 1\n"
    );
}

#[test]
fn native_map_operands_follow_argument_control_flow() {
    let run = emit_link_and_run(
        "map_argument_cfg.ko",
        r#"
fun main(): Unit {
 var m = mutableMapOf<Int, Int>()
 m.put(if (true) { 1 } else { 2 }, if (false) { 4 } else { 3 })
 if (m.size == 1) { println("put") }
 if (m.get(if (true) { 1 } else { 2 }) == 3) { println("get") }
 if (m.contains(if (false) { 2 } else { 1 })) { println("contains") }
 if (m.remove(if (true) { 1 } else { 2 }) == 3) { println("remove") }
 if (m.size == 0) { println("empty") }
 var strings = mutableMapOf<String, Int>()
 val key = "key".clone()
 strings.put(key, if (true) { 3 } else { 4 })
 strings.put("key".clone(), if (false) { 4 } else { 5 })
 if (strings.size == 1 && strings.get("key") == 5) { println("string key") }
}
"#,
        "main",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"put\nget\ncontains\nremove\nempty\nstring key\n"
    );
}

#[test]
fn native_map_mutation_collects_drop_glue_before_abort() {
    for (declarations, body) in [
        (
            "class Resource(val n: Int) { deinit() { println(\"drop\") } }",
            "var m = mutableMapOf<Int, Resource>()\n m.put(1, Resource(1))\n error(\"stop\")",
        ),
        (
            "",
            "var m = mutableMapOf<String, Int>()\n val key = \"key\".clone()\n m.remove(key)\n error(key)",
        ),
    ] {
        let text = format!("{declarations}\nfun main(): Unit {{\n {body}\n}}");
        let run = emit_link_and_run("map_abort_drop_glue.ko", &text, "main");
        assert!(!run.status.success(), "{run:?}");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(run.status.signal(), Some(6));
        }
    }
}
