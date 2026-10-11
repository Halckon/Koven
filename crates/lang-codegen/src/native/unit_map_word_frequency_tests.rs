//! SPEC-0295: ordinary-source Map owners through borrowed argv-style loops.
use super::resource_exchange_tests::run_unit;
use crate::native_tests::boxed_enum_tests::assert_success;

#[test]
fn unit_map_word_frequency_two_passes_keep_first_occurrence_order() {
    let provider = r#"package p
fun printWord(word: String, count: Int): Unit {
    println(word)
    if (count == 1) { println("one") } else { println("three") }
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
