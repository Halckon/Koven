//! Exact main429 writeback source through SSA/LLVM only; no native execution.
#[test]
fn copyable_writeback_and_literal_reachability_preserve_verified_lowering() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;
    let original = r#"fun write(inout target: Int?): Boolean {
    target = null
    println("wrote")
    return true
}

fun run(left: Boolean?, flag: Boolean, other: Boolean): Int {
    var x: Int? = 7
    if (x != null && (if (flag) {
        loop {
            if (left == null) {
                if (flag) {
                    if (other || (write(&x) && false)) { break }
                    return 0
                } else { break }
            }
            break
        }
        true
    } else { true })) {
        if (other) { if (x != null) {} }
        return x
    }
    return 0
}

fun main(): Unit {
    if (run(null, true, true) == 7) { println("break-kept-value") } else { println("bad-break") }
    if (run(null, true, false) == 0) { println("write-and-return") } else { println("bad-return") }
    if (run(true, true, false) == 7) { println("outer-branch-skipped-write") } else { println("bad-skip") }
}
"#;
    let literal = original.replace(
        "if (other || (write(&x) && false)) { break }",
        "if ((!true)) { x = null; break }; if (other) { break }; write(&x)",
    );
    for text in [original, literal.as_str()] {
        let source =
            UnitSource::analyze("main429.ko", text).expect("same real frontend ownership product");
        source.assert_frontend_clean();
        let entry = source.declaration("main", SymbolKind::Function);
        let (program, function) = source
            .lower_entry(entry)
            .expect("main429 source remains accepted");
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, function).unwrap();
    }
}
