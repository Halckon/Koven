//! SPEC-0295: ordinary-source Map owners through borrowed argv-style loops.
use super::resource_exchange_tests::run_unit;
use crate::native_tests::boxed_enum_tests::assert_success;

#[test]
fn unit_map_word_frequency_two_passes_keep_first_occurrence_order() {
    let provider = r#"package p
fun printWord(word: String, count: Int): Unit {
    println(word)
    if (count == 1) { println("one") } else if (count == 3) { println("three") } else { error("unexpected count") }
}
fun countWords(args: Array<String>): Unit {
    var counts = mutableMapOf<String, Int>()
    for (word in args) {
        val previous = counts.get(word)
        val count = if (previous == null) { 1 } else { previous + 1 }
        counts.put(word.clone(), count)
    }
    for (word in args) {
        val count = counts.remove(word)
        if (count != null) { printWord(word, count) }
    }
}
"#;
    let consumer = r#"package q
fun entry(): Unit {
    p.countWords(arrayOf<String>())
    p.countWords(arrayOf("beta", "alpha", "beta", "界", "beta"))
    println("after")
}
"#;
    for constants in [false, true] {
        let (run, _) = run_unit(provider, consumer, constants);
        assert_success(&run, "beta\nthree\nalpha\none\n界\none\nafter\n".as_bytes());
    }
}

const NARROWED_BORROW_SOURCE: &str = r#"
value class Count(val n: Int)
fun zero(value: Int): Unit { if (value == 0) { println("zero") } else { error("wrong zero") } }
fun flag(value: Boolean): Unit { if (!value) { println("false") } else { error("wrong flag") } }
fun count(value: Count): Unit { if (value.n == 7) { println("seven") } else { error("wrong count") } }
fun exercise(): Unit {
    var numbers = mutableMapOf<Int, Int>()
    numbers.put(1, 0)
    val gotZero = numbers.get(1)
    if (gotZero != null) { zero(gotZero); zero((gotZero)) }
    val removedZero = numbers.remove(1)
    if (removedZero != null) { zero(removedZero) }
    var flags = mutableMapOf<Int, Boolean>()
    flags.put(1, false)
    val gotFlag = flags.get(1)
    if (gotFlag != null) { flag(gotFlag); flag((gotFlag)) }
    val removedFlag = flags.remove(1)
    if (removedFlag != null) { flag(removedFlag) }
    if (flags.get(1) != null) { error("flag still present") }
    var counts = mutableMapOf<Boolean, Count>()
    counts.put(true, Count(7))
    val gotCount = counts.get(true)
    if (gotCount != null) { count(gotCount); count((gotCount)) }
    val removedCount = counts.remove(true)
    if (removedCount != null) { count(removedCount) }
    if (counts.get(true) != null) { error("count still present") }
}
"#;

#[test]
fn unit_map_word_frequency_narrowed_borrow_is_generic_for_get_and_remove() {
    let provider = format!("package p\n{NARROWED_BORROW_SOURCE}");
    let consumer = "package q\nfun entry(): Unit { p.exercise() }";
    for constants in [false, true] {
        let (run, _) = run_unit(&provider, consumer, constants);
        assert_success(
            &run,
            b"zero\nzero\nzero\nfalse\nfalse\nfalse\nseven\nseven\nseven\n",
        );
    }
}

#[test]
fn single_map_word_frequency_narrowed_borrow_is_generic_for_get_and_remove() {
    let source = format!("{NARROWED_BORROW_SOURCE}\nfun main(): Unit {{ exercise() }}");
    let run = crate::native_tests::emit_link_and_run("map-narrowed-borrow.ko", &source, "main");
    assert_success(
        &run,
        b"zero\nzero\nzero\nfalse\nfalse\nfalse\nseven\nseven\nseven\n",
    );
}

#[test]
fn unit_map_word_frequency_owner_carry_remains_generic() {
    let provider = r#"package p
class Resource(val word: String) { deinit() { println(this.word) } }
fun exercise(args: Array<String>): Unit {
    var texts = mutableMapOf<String, String>()
    for (word in args) { texts.put(word.clone(), word.clone()) }
    val value = texts.remove("key")!!
    println(value)
    var resources = mutableMapOf<String, Resource>()
    for (word in args) { resources.put(word.clone(), Resource(word.clone())) }
    val removed = resources.remove("key")
    println("done")
}
"#;
    let consumer = "package q\nfun entry(): Unit { p.exercise(arrayOf(\"key\", \"key\")) }";
    for constants in [false, true] {
        let (run, _) = run_unit(provider, consumer, constants);
        assert_success(&run, b"key\nkey\ndone\nkey\n");
    }
}
