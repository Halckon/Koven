//! A fresh If proof belongs to an iteration and ends before the next loop edge.
use super::*;

#[test]
fn nullable_when_writeback_then_reborrow_in_a_while_body() {
    const PATH: &str = "nullable-when-writeback-while.ko";
    const TEXT: &str = r#"
class Node {}
class Guard(val label: String) { deinit() { println(label) } }
fun observe(node: Node): Unit { println("observe") }
fun makeReplacement(present: Boolean): Node? {
    if (present) { return Node() }
    return null
}
fun visit(own mode: Int): Unit {
    val outer = Guard("drop:outer")
    var slot: Node? = Node()
    var step = 0
    while (step < 2) {
        step += 1
        val iteration = Guard("drop:iteration")
        when (slot) {
            null -> { println("slot:null") }
            else -> {
                println("replace")
                slot = makeReplacement(mode != 4)
                if (slot != null) {
                    observe(slot)
                    if (mode == 0) { println("return"); return }
                    if (mode == 1 && step == 1) { println("continue"); continue }
                    if (mode == 2) { println("break"); break }
                }
            }
        }
        println("tail")
    }
    println("after")
}
fun main(): Unit {
    println("case:return"); visit(0)
    println("case:continue"); visit(1)
    println("case:break"); visit(2)
    println("case:normal"); visit(3)
    println("case:null"); visit(4)
    println("done")
}
"#;
    let source = checked(PATH, TEXT);
    let entry = source.declaration("main", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.unwrap_or_else(|error| {
        let location = error.span.and_then(|span| source.sources.slice(span).ok());
        panic!("loop-scoped fresh proof must lower: {error:?} {location:?}");
    });
    assert!(!routed.contains(&function(&source, &program, "visit").name));
    verify_program(&program).expect("loop edges cannot carry a stale If proof");
    let output = crate::native_tests::emit_link_and_run(PATH, TEXT, "main");
    let expected = concat!(
        "case:return\n",
        "replace\n",
        "observe\n",
        "return\n",
        "drop:iteration\n",
        "drop:outer\n",
        "case:continue\n",
        "replace\n",
        "observe\n",
        "continue\n",
        "drop:iteration\n",
        "replace\n",
        "observe\n",
        "tail\n",
        "drop:iteration\n",
        "after\n",
        "drop:outer\n",
        "case:break\n",
        "replace\n",
        "observe\n",
        "break\n",
        "drop:iteration\n",
        "after\n",
        "drop:outer\n",
        "case:normal\n",
        "replace\n",
        "observe\n",
        "tail\n",
        "drop:iteration\n",
        "replace\n",
        "observe\n",
        "tail\n",
        "drop:iteration\n",
        "after\n",
        "drop:outer\n",
        "case:null\n",
        "replace\n",
        "tail\n",
        "drop:iteration\n",
        "slot:null\n",
        "tail\n",
        "drop:iteration\n",
        "after\n",
        "drop:outer\n",
        "done\n",
    );
    crate::native_tests::boxed_enum_tests::assert_success(&output, expected.as_bytes());
}
