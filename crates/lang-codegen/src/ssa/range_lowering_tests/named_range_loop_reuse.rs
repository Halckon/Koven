use super::*;

pub(super) const SOURCE: &str = r#"
fun observe(part: View<Int>): Unit {
    if (part.size == 1) { println("still live") }
}
fun consume(own xs: List<Int>): Unit {
    if (xs.size == 2) { println("owner") }
}
fun main(): Unit {
    val xs = listOf(7, 9)
    borrow val part = take(xs, 1)
    for (item in part) {
        if (item == 7) { println("element") }
    }
    observe(part)
    consume(xs)
}
"#;

#[test]
fn named_range_loop_reuse_native_keeps_backing_live_until_owner_consumer() {
    assert_native_backing_lifetime(SOURCE, "element\nstill live\nconsume entered\n");
}

#[test]
fn named_range_loop_reuse_native_zero_iterations_keeps_backing_live() {
    let source = SOURCE
        .replace("take(xs, 1)", "take(xs, 0)")
        .replace("part.size == 1", "part.size == 0");
    assert_native_backing_lifetime(&source, "still live\nconsume entered\n");
}

fn assert_native_backing_lifetime(source: &str, live_prefix: &str) {
    use super::{memory_unit_with_entry, unit_with_entry};
    use crate::native_tests::{TestDirectory, boxed_enum_tests::write_counted_allocations};
    use std::{fs::OpenOptions, io::Write, process::Command};

    // Leave the exact caller unchanged; the consumer announces entry while
    // the real read of xs.size is still ahead. Its final print may follow drop.
    let source = source.replace(
        "fun consume(own xs: List<Int>): Unit {",
        "fun consume(own xs: List<Int>): Unit { println(\"consume entered\")",
    );
    for rooted in [false, true] {
        let (program, entry) = if rooted {
            unit_with_entry(&format!(
                "package app\nimport koven.algorithms.take\n{source}"
            ))
        } else {
            memory_unit_with_entry(&source)
        };
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let directory = TestDirectory::create();
        let ir = directory.join("range-reuse.ll");
        let counter = directory.join("counter.c");
        let executable = directory.join("range-reuse");
        // The Ints and Range metadata are inline. The only physical allocation
        // is the original List backing, identity 0, live through consumer entry.
        assert!(llvm.contains("@write("));
        write_counted_allocations(
            &llvm.replace("@write(", "@counted_write("),
            1,
            Some(&[0]),
            &ir,
            &counter,
        );
        OpenOptions::new()
            .append(true)
            .open(&counter)
            .unwrap()
            .write_all(
                br#"
#include <unistd.h>
static size_t observable_bytes;
ssize_t counted_write(int fd, const void *bytes, size_t size) {
    if (fd == STDOUT_FILENO && size) {
        if (observable_bytes < EXPECTED_LIVE_PREFIX) {
            assert(allocations == 1);
            assert(releases == 0);
            assert(live[0]);
        }
        observable_bytes += size;
    }
    return write(fd, bytes, size);
}
"#,
            )
            .unwrap();
        let linked = Command::new(crate::test_support::ir_clang())
            .arg(ir)
            .arg(counter)
            .arg(format!("-DEXPECTED_LIVE_PREFIX={}", live_prefix.len()))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        crate::ssa::native_bounded::execute_binary(
            &executable,
            format!("{live_prefix}owner\n").as_bytes(),
        );
    }
}

#[test]
fn named_range_loop_reuse_survives_provider_exhaustion_for_a_second_consumer() {
    let source = SOURCE;
    let mut failures = Vec::new();
    for rooted in [false, true] {
        let result = std::panic::catch_unwind(|| {
            if rooted {
                unit_with_entry(&format!(
                    "package app\nimport koven.algorithms.take\n{source}"
                ))
            } else {
                memory_unit_with_entry(source)
            }
        });
        let (program, entry) = match result {
            Ok(product) => product,
            Err(_) => {
                failures.push(rooted);
                continue;
            }
        };
        verify_program(&program).unwrap();
        let module = program.module(entry.module()).unwrap();
        let main = module.function(entry).unwrap();
        let call = |name| {
            main.instructions
                .iter()
                .find_map(|i| match &i.operation {
                    Operation::DirectCall {
                        callee, arguments, ..
                    } if module.function(*callee).unwrap().name.contains(name) => {
                        Some((i.id, arguments.as_slice()))
                    }
                    _ => None,
                })
                .unwrap()
        };
        let (observe, [EntityId::Loan(observer)]) = call("observe") else {
            panic!("observer must receive a loan")
        };
        let (consume, [EntityId::Value(_)]) = call("consume") else {
            panic!("consumer must receive an owned value")
        };
        let range_ends: Vec<_> = main
            .instructions
            .iter()
            .filter_map(|i| match i.operation {
                Operation::RangeEnd { view, source } => Some((i.id, view, source)),
                _ => None,
            })
            .collect();
        let [(end_range, _, range_source)] = range_ends.as_slice() else {
            panic!("expected one descriptor retirement")
        };
        let block = main
            .blocks
            .iter()
            .find(|b| b.instructions.contains(&observe))
            .unwrap();
        let position = |id| {
            block
                .instructions
                .iter()
                .position(|&actual| actual == id)
                .unwrap()
        };
        let end_observer = main
            .instructions
            .iter()
            .find(|i| matches!(i.operation, Operation::BorrowEnd { loan } if loan == *observer))
            .unwrap()
            .id;
        assert!(position(observe) < position(end_observer));
        assert!(position(end_observer) < position(*end_range));
        assert!(position(*end_range) < position(consume));
        assert_ne!(observer, range_source);
        assert!(
            main.instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::RangeElementPlace { .. }))
        );
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    }
    assert!(
        failures.is_empty(),
        "ordinary Unit lowering failed: rooted={failures:?}"
    );
}

#[test]
fn adjacent_named_range_loop_blocks_both_lower_and_verify() {
    let source = r#"
fun observe(part: View<Int>): Unit {
    if (part.size == 1) { println("still live") }
}
fun consume(own xs: List<Int>): Unit {
    if (xs.size == 2) { println("owner") }
}
fun main(): Unit {
    block {
        val xs1 = listOf(7, 9)
        borrow val part1 = take(xs1, 1)
        for (item in part1) {
            if (item == 7) { println("first") }
        }
        observe(part1)
        consume(xs1)
    }
    block {
        val xs2 = listOf(7, 9)
        borrow val part2 = take(xs2, 1)
        for (item in part2) {
            if (item == 7) { println("second") }
        }
        observe(part2)
        consume(xs2)
    }
}
"#;
    for rooted in [false, true] {
        let (program, entry) = if rooted {
            unit_with_entry(&format!(
                "package app\nimport koven.algorithms.take\n{source}"
            ))
        } else {
            memory_unit_with_entry(source)
        };
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    }
}

#[test]
fn named_range_loop_reuse_ast_verification_rejects_mismatched_if_and_observer() {
    use crate::ssa::{
        borrow_result_support::verified_named_range_loop_reuse_control,
        lower_frontend_tests::map_result_tests::loop_owner_tests::unit_identity::UnitSource,
    };
    use lang_frontend::{
        ownership_checking::UnitPreflightFacts, parser::Expression, type_checking::UnitExpressionId,
    };

    let text = format!(
        "{}\n{SOURCE}",
        include_str!("../../../../lang-std/koven/algorithms/ranges.ko")
            .strip_prefix("package koven.algorithms\n")
            .unwrap()
    );
    let analysis =
        UnitSource::analyze_with_type_environment("trusted.ko", &text, |sources, source, types| {
            types.authorize_range_source(sources, source).unwrap();
            types
                .authorize_range_extension_source(sources, source)
                .unwrap();
        })
        .unwrap();
    analysis.assert_frontend_clean();
    let file = analysis.parsed();
    let [source] = analysis.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    let unit = source.id();
    let controls: Vec<_> = analysis
        .ownership()
        .named_range_loop_reuse_controls()
        .into_values()
        .collect();
    let [receipt] = controls.as_slice() else {
        panic!("expected one pre-checked named Range loop reuse receipt");
    };
    let receipt = *receipt;

    assert_eq!(
        verified_named_range_loop_reuse_control(
            file,
            unit,
            receipt.loop_statement(),
            receipt.body_control(),
            receipt.end_statement(),
            receipt.observer(),
        ),
        Some(receipt.body_control().expression().index())
    );

    // Swapping body_control to another `if` in the same source file must be rejected.
    let other_if = file
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (matches!(node.payload(), Expression::If { .. })
                && id != receipt.body_control().expression())
            .then_some(UnitExpressionId::new(unit, id))
        })
        .expect("another if expression in the same source file");
    assert_eq!(
        verified_named_range_loop_reuse_control(
            file,
            unit,
            receipt.loop_statement(),
            other_if,
            receipt.end_statement(),
            receipt.observer(),
        ),
        None
    );

    // Swapping observer to another call expression in the same source file must be rejected.
    let other_call = file
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (matches!(node.payload(), Expression::Call { .. })
                && id != receipt.observer().expression())
            .then_some(UnitExpressionId::new(unit, id))
        })
        .expect("another call expression in the same source file");
    assert_eq!(
        verified_named_range_loop_reuse_control(
            file,
            unit,
            receipt.loop_statement(),
            receipt.body_control(),
            receipt.end_statement(),
            other_call,
        ),
        None
    );
}
