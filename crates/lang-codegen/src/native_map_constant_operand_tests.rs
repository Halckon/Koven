//! Ordinary source to native only; no allocator replacement or SSA mutation.
use super::emit_link_and_run;

#[test]
fn native_map_basic_grouped_local_and_borrow_keys_keep_storage_identity() {
    let source = r#"
fun inspect(numbers: MutableMap<Int, Int>, key: Int, words: MutableMap<String, Int>, word: String): Unit {
    if (numbers.contains((key))) { println("borrow int") }
    if (numbers.get((key)) == 7) { println("borrow get") }
    if (words.contains((word))) { println("borrow string") }
}
fun main(): Unit {
    var numbers = mutableMapOf<Int, Int>()
    numbers.put(1, 7)
    var words = mutableMapOf<String, Int>()
    words.put("key".clone(), 9)
    val key = 1
    val word = "key".clone()
    inspect(numbers, key, words, word)
    if (numbers.contains((key))) { println("local int") }
    if (words.get((word)) == 9) { println("local string") }
    if (numbers.remove((key)) == 7) { println("remove int") }
    if (words.remove((word)) == 9) { println("remove string") }
    println(word)
}
"#;
    let run = emit_link_and_run("map-basic-grouped-keys.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"borrow int\nborrow get\nborrow string\nlocal int\nlocal string\nremove int\nremove string\nkey\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn native_map_constant_grouped_char_and_string_keys_use_materialization() {
    let source = r#"
const val HIT: Char = '中'
const val MISS: Char = 'b'
const val WORD: String = "key"
fun main(): Unit {
    var chars = mutableMapOf<Char, Int>()
    chars.put(HIT, 7)
    if (chars.contains((HIT)) && !chars.contains(MISS)) { println("char contains") }
    if (chars.get((HIT)) == 7 && chars[(HIT)] == 7) { println("char get") }
    if (chars.remove((HIT)) == 7 && !chars.contains(HIT)) { println("char remove") }
    var words = mutableMapOf<String, Int>()
    words.put(WORD, 9)
    if (words.contains((WORD))) { println("string contains") }
    if (words.get((WORD)) == 9) { println("string get") }
    if (words.remove((WORD)) == 9) { println("string remove") }
    println(WORD)
}
"#;
    let run = emit_link_and_run("map-constant-grouped-keys.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"char contains\nchar get\nchar remove\nstring contains\nstring get\nstring remove\nkey\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
}
