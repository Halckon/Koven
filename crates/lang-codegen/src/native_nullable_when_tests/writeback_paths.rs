//! Native Copyable narrowing across writeback and loop exits.
#[test]
fn copyable_nullable_if_keeps_the_proof_on_the_break_path() {
    let source = r#"fun write(inout target: Int?): Boolean {
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
    const SIBLING: &str = r#"
fun unused(left: Int?): Int { return left ?: 0 }
"#;
    const MAP_LOOKUP: &str = r#"
fun lookup(): Int {
    var map = mutableMapOf<Int, Int>()
    map.put(1, 42)
    val value = map.remove(1)
    if (value != null) { return value }
    return 0
}
"#;
    let with_sibling = format!("{source}{SIBLING}");
    let literal_guard = source.replace(
        "if (other || (write(&x) && false)) { break }",
        "if ((!true)) { x = null; break }\n                    if (other) { break }\n                    write(&x)",
    );
    let reached_map = format!("{source}{MAP_LOOKUP}").replace(
        "fun main(): Unit {",
        "fun main(): Unit { if (lookup() == 42) { println(\"map-result\") }",
    );
    let expected = "break-kept-value\nwrote\nwrite-and-return\nouter-branch-skipped-write\n";
    let reached_expected = format!("map-result\n{expected}");
    for (path, text, stdout, map_result) in [
        ("loop-writeback.ko", source, expected, false),
        ("with-sibling.ko", &with_sibling, expected, false),
        ("literal-guard.ko", &literal_guard, expected, false),
        ("with-reached-map.ko", &reached_map, &reached_expected, true),
    ] {
        use super::{SymbolKind, UnitSource};

        let analysis = UnitSource::analyze(path, text).expect("strict Unit admission");
        analysis.assert_frontend_clean();
        let entry = analysis.declaration("main", SymbolKind::Function);
        let run_span = analysis.declaration_span(analysis.declaration("run", SymbolKind::Function));
        let (result, routed) = crate::ssa::observe_routing(|| analysis.lower_entry(entry));
        let (program, _) = result.expect("default lowering verifies");
        let run = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.origin.span() == run_span)
            .expect("source run callable");
        assert!(routed.contains(&run.name), "{path}: {routed:?}");
        if map_result {
            assert!(run.instructions.iter().any(|instruction| matches!(
                instruction.operation,
                super::Operation::MapResultUnwrap { .. }
            )));
        } else {
            assert!(run.instructions.iter().any(|instruction| matches!(
                instruction.operation,
                super::Operation::NullableTake { .. }
            )));
        }
        let output = crate::native_tests::emit_link_and_run(path, text, "main");
        crate::native_tests::boxed_enum_tests::assert_success(&output, stdout.as_bytes());
    }
}

#[test]
fn copyable_nullable_if_joins_independent_proofs_and_rechecks_after_writeback() {
    let source = r#"fun join(x: Int?, flag: Boolean): Int {
    if (flag) {
        if (x == null) { return 0 }
    } else {
        if (x == null) { return 0 }
    }
    return x
}
fun repeat(input: Int?): Int {
    var x = input
    if (x != null) {
        var changed = false
        loop {
            if (x != null && changed) { return x }
            x = 9
            changed = true
        }
    }
    return 0
}
fun ordered(input: Int?, flag: Boolean): Int {
    var x = input
    if (x != null) {
        return x + (if (flag) {
            x = 9
            if (x != null) { x } else { 0 }
        } else { 0 })
    }
    return 0
}
fun main(): Unit {
    if (join(7, true) == 7 && join(8, false) == 8 && join(null, false) == 0) {
        println("join-keeps-both-proofs")
    }
    if (repeat(7) == 9 && repeat(null) == 0) { println("recheck-uses-new-value") }
    if (ordered(7, true) == 16 && ordered(7, false) == 7 && ordered(null, false) == 0) {
        println("left-operand-keeps-original-value")
    }
}
"#;
    assert_join_proof_is_passed_on_both_edges(source);
    let output = crate::native_tests::emit_link_and_run("nullable-proofs.ko", source, "main");
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"join-keeps-both-proofs\nrecheck-uses-new-value\nleft-operand-keeps-original-value\n"
    );
}

fn assert_join_proof_is_passed_on_both_edges(text: &str) {
    use super::{EntityId, Operation, SymbolKind, TerminatorKind, UnitSource, edges};
    use crate::ssa::model::Definition;

    let source = UnitSource::analyze("nullable-proofs.ko", text).expect("strict Unit admission");
    let entry = source.declaration("main", SymbolKind::Function);
    let join_span = source.declaration_span(source.declaration("join", SymbolKind::Function));
    let (program, _) = source
        .lower_entry(entry)
        .expect("default lowering verifies");
    let function = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.origin.span() == join_span)
        .expect("join callable");
    let (block, value) = function
        .blocks
        .iter()
        .filter_map(|block| {
            let term = block.terminator.as_ref()?;
            let TerminatorKind::Return { values } = &term.kind else {
                return None;
            };
            let [value] = values.as_slice() else {
                return None;
            };
            (source.sources().slice(term.origin.span()) == Ok("return x"))
                .then_some((block, *value))
        })
        .next()
        .expect("source return of the joined non-null value");
    let Definition::BlockParameter {
        block: owner,
        index,
    } = function.entity(EntityId::Value(value)).unwrap().definition
    else {
        panic!("the joined value must have its own block parameter");
    };
    assert_eq!(owner, block.id);

    let definitions: Vec<_> = function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .flat_map(|term| edges(&term.kind))
        .filter(|edge| edge.target == block.id)
        .map(|edge| {
            let Definition::InstructionResult { instruction, .. } =
                function.entity(edge.arguments[index]).unwrap().definition
            else {
                panic!("each arm must supply the payload from its own proof");
            };
            assert!(matches!(
                function.instruction(instruction).unwrap().operation,
                Operation::NullableTake { .. }
            ));
            instruction
        })
        .collect();
    let [left, right] = definitions.as_slice() else {
        panic!("both independently proven arms must reach the same return");
    };
    assert_ne!(left, right);
}
