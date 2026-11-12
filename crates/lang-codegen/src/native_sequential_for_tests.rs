use crate::test_support::diagnostics::diagnostic;
mod atomic_tests {
use super::super::{NativeObjectErrorKind, SymbolKind, TestDirectory, emit_link_and_run};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};
use std::{collections::BTreeMap, fs};

fn snapshot(path: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().into_string().unwrap(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn actual_single_file_for_rejects_foreign_sources_and_facts_without_touching_directory() {
    let source = "fun entry(): Unit { for (item in arrayOf(1, 2, 3)) { println(\"visit\") } }";
    let first = UnitSource::analyze("single-for.ko", source)
        .expect("original source must pass strict Unit admission");
    let second = UnitSource::analyze("single-for.ko", source)
        .expect("original source must pass strict Unit admission");
    for present in [false, true] {
        let directory = TestDirectory::create();
        let output = directory.join("program.o");
        fs::write(directory.join("keep"), b"sibling").unwrap();
        if present {
            fs::write(&output, b"old object").unwrap();
        }
        for (foreign_sources, typed, owned) in [
            (Some(&second), &first, &first),
            (None, &second, &second),
            (None, &first, &second),
        ] {
            let before = snapshot(&directory.0);
            let guard =
                crate::llvm::emission_failure::Guard::new(directory.join("keep").join("object.o"));
            let entry = first.declaration("entry", SymbolKind::Function);
            let error = match foreign_sources {
                Some(sources) => first.emit_with_sources_from(sources, entry, &output),
                None => first.emit_with_products_from(typed, owned, entry, &output),
            }
            .unwrap_err();
            assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
            assert_eq!(error.span(), None);
            assert_eq!(guard.calls(), 0);
            assert_eq!(snapshot(&directory.0), before);
            drop(guard);
        }
        first
            .emit(first.declaration("entry", SymbolKind::Function), &output)
            .unwrap();
        crate::test_support::assert_native_object(&fs::read(output).unwrap());
    }
    let run = emit_link_and_run("single-for.ko", source, "entry");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(run.stdout, b"visit\nvisit\nvisit\n");
    assert!(run.stderr.is_empty());
}

#[test]
fn independent_analysis_chains_render_identical_for_ssa_and_verified_llvm() {
    let source = "fun entry(): Unit { for (item in listOf(1, 2, 3)) { if (item == 2) { continue }; if (item == 3) { break }; println(\"visit\") } }";
    let mut expected = None;
    for _ in 0..3 {
        let analysis = UnitSource::analyze("deterministic-for.ko", source)
            .expect("original source must pass strict Unit admission");
        let (program, entry) = analysis
            .lower_entry(analysis.declaration("entry", SymbolKind::Function))
            .unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        let rendered = (
            crate::ssa::render_program(&program),
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap(),
        );
        if let Some(expected) = &expected {
            assert_eq!(&rendered, expected);
        } else {
            expected = Some(rendered);
        }
    }
}

#[test]
fn resource_and_non_source_owner_replacement_emission_failure_preserves_output() {
    // CI-only whole: both original legal sources and all four output states stay together.
    // Reaching the injected emission failure proves atomic failure, not native success.
    for source in [
        "class Leaf(val name: String) { deinit() { println(this.name) } }\nfun scan(flag: Boolean): Unit { var guard = Leaf(\"initial\"); for (item in arrayOf(1, 2)) { if (flag) { guard = Leaf(\"replacement\") } } }\nfun entry(): Unit { scan(true) }",
        "fun scan(flag: Boolean): Unit { var text = \"initial\"; for (item in arrayOf(1, 2)) { if (flag) { text = \"replacement\" } }; println(text) }\nfun entry(): Unit { scan(true) }",
    ] {
        let analysis = UnitSource::analyze("bounded-presence.ko", source)
            .expect("original source must pass strict Unit admission");
        for present in [false, true] {
            let directory = TestDirectory::create();
            let output = directory.join("program.o");
            fs::write(directory.join("keep"), b"sibling").unwrap();
            if present {
                fs::write(&output, b"old object").unwrap();
            }
            let before = snapshot(&directory.0);
            let guard = crate::llvm::emission_failure::Guard::new(directory.join("keep/object.o"));
            let error = analysis
                .emit(analysis.declaration("entry", SymbolKind::Function), &output)
                .unwrap_err();
            assert_eq_diagnostic!(error.kind(), NativeObjectErrorKind::Backend, "{error}");
            assert_eq_diagnostic!(error.span(), None, "{error}");
            assert_diagnostic!(error.diagnostic().is_none(), "{error}");
            assert_eq!(guard.calls(), 1, "the real emission hook must be reached");
            assert_eq!(snapshot(&directory.0), before);
        }
    }
}
}
#[path = "native_sequential_for_tests/cleanup_tests.rs"]
pub(crate) mod cleanup_tests;
mod owned_source_tests {
//! SPEC-0182 owned/Borrow sources: independent native stdout across 72 cells.
use super::super::{SymbolKind, UnitSource, boxed_enum_tests, emit_link_and_run};

#[test]
fn owned_and_borrow_sources_cover_three_containers_sizes_and_exits() {
    for (container, factory) in [
        ("Array", "arrayOf"),
        ("List", "listOf"),
        ("MutableList", "mutableListOf"),
    ] {
        for borrowed in [false, true] {
            for size in [0, 1, 3] {
                for jump in ["", "continue", "break", "return"] {
                    let values = (0..size).map(|_| "1").collect::<Vec<_>>().join(", ");
                    let make = format!("{factory}<Int>({values})");
                    let body = format!(
                        "for (item in xs) {{ if (item == 1) {{ println(\"visit\") }}; {jump} }}"
                    );
                    let source = if borrowed {
                        format!(
                            "fun scan(xs: {container}<Int>): Unit {{ {body} }}\nfun entry(): Unit {{ scan({make}); println(\"done\") }}"
                        )
                    } else {
                        format!(
                            "fun scan(): Unit {{ val xs = {make}; {body} }}\nfun entry(): Unit {{ scan(); println(\"done\") }}"
                        )
                    };
                    let count = if matches!(jump, "break" | "return") {
                        size.min(1)
                    } else {
                        size
                    };
                    let expected = format!("{}done\n", "visit\n".repeat(count));
                    eprintln!("{container} Borrow={borrowed} size={size} exit={jump}");
                    let output = emit_link_and_run("owned-source.ko", &source, "entry");
                    boxed_enum_tests::assert_success(&output, expected.as_bytes());
                }
            }
        }
    }
}

#[test]
fn borrow_cell_projection_copies_int_and_borrows_string_without_moving_root() {
    let source = r#"
class Cell(val number: Int, val text: String)
fun inspect(text: String): Unit { println(text) }
fun scan(xs: Array<Cell>): Unit {
    for (cell in xs) { if (cell.number == 7) { inspect(cell.text) }; if (cell.number == 7) { println("root alive") } }
}
fun entry(): Unit { scan(arrayOf(Cell(7, "cell"))) }
"#;
    boxed_enum_tests::assert_success(
        &emit_link_and_run("borrow-cell.ko", source, "entry"),
        b"cell\nroot alive\n",
    );
}

#[test]
fn source_factory_is_called_once_and_last_use_needs_no_readback() {
    for (factory, ty) in [
        ("arrayOf", "Array"),
        ("listOf", "List"),
        ("mutableListOf", "MutableList"),
    ] {
        let source = format!(
            "fun source(): {ty}<Int> {{ println(\"source\"); return {factory}(1, 2, 3) }}\nfun entry(): Unit {{ for (item in source()) {{ println(\"visit\") }}; val empty = {factory}<Int>(); for (item in empty) {{ println(\"wrong\") }}; println(\"done\") }}"
        );
        boxed_enum_tests::assert_success(
            &emit_link_and_run("source-once.ko", &source, "entry"),
            b"source\nvisit\nvisit\nvisit\ndone\n",
        );
    }
}

#[test]
fn direct_resource_elements_drop_in_reverse_order_and_release_distinct_storage() {
    for factory in ["arrayOf", "listOf", "mutableListOf"] {
        for size in [0, 1, 3] {
            for jump in ["", "continue", "break", "return"] {
                let values = (0..size)
                    .map(|i| format!("Leaf(\"{i}\")"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let source = format!(
                    "class Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}\nfun scan(): Unit {{ val xs = {factory}<Leaf>({values}); for (item in xs) {{ println(\"visit\"); {jump} }} }}\nfun entry(): Unit {{ scan(); println(\"done\") }}"
                );
                let visits = if matches!(jump, "break" | "return") {
                    size.min(1)
                } else {
                    size
                };
                let drops = (0..size)
                    .rev()
                    .map(|i| format!("{i}\n"))
                    .collect::<String>();
                let expected = format!("{}{drops}done\n", "visit\n".repeat(visits));
                boxed_enum_tests::assert_success(
                    &emit_link_and_run("resource-elements.ko", &source, "entry"),
                    expected.as_bytes(),
                );
                let analysis = UnitSource::analyze("resource-elements.ko", &source)
                    .expect("original source must pass strict Unit admission");
                analysis.assert_frontend_clean();
                let (program, entry) = analysis
                    .lower_entry(analysis.declaration("entry", SymbolKind::Function))
                    .unwrap();
                let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                let counted =
                    boxed_enum_tests::run_counted_allocations(&ir, size + usize::from(size > 0));
                boxed_enum_tests::assert_success(&counted, expected.as_bytes());
            }
        }
    }
}

#[test]
fn resource_value_components_and_nested_providers_keep_roots_until_children_end() {
    for factory in ["arrayOf", "listOf", "mutableListOf"] {
        for empty in [false, true] {
            for jump in ["", "continue", "break", "return"] {
                for pattern in ["(number, leaf, text)", "(number, _, text)", "(_, leaf, _)"] {
                    let values = if empty {
                        ""
                    } else {
                        "Parts(7, Leaf(\"leaf\"), \"text\")"
                    };
                    let uses = if pattern.contains("number, leaf, text") {
                        "if (number == 7) { inspect(leaf); println(text) }"
                    } else if pattern.starts_with("(number") {
                        "if (number == 7) { println(text) }"
                    } else {
                        "inspect(leaf)"
                    };
                    let source = format!(
                        "class Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}\nvalue class Parts(val number: Int, val leaf: Leaf, val text: String)\nfun inspect(leaf: Leaf): Unit {{ println(\"inspect\") }}\nfun scan(): Unit {{ val xs = {factory}<Parts>({values}); for ({pattern} in xs) {{ for (inner in arrayOf(1)) {{ {uses} }}; {jump} }} }}\nfun entry(): Unit {{ scan(); println(\"done\") }}"
                    );
                    let expected = if empty {
                        "done\n".to_owned()
                    } else {
                        format!(
                            "{}{}leaf\ndone\n",
                            if uses.contains("inspect") {
                                "inspect\n"
                            } else {
                                ""
                            },
                            if uses.contains("println(text)") {
                                "text\n"
                            } else {
                                ""
                            }
                        )
                    };
                    boxed_enum_tests::assert_success(
                        &emit_link_and_run("resource-components.ko", &source, "entry"),
                        expected.as_bytes(),
                    );
                }
            }
        }
    }
}
}
mod unit_storage_boundary_tests {
//! Unit storage does not weaken the native analysis-identity/publication boundary.

use super::super::{NativeObjectErrorKind, SymbolKind, TestDirectory};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;

#[test]
fn unit_storage_mixed_analysis_preserves_existing_object() {
    let first = UnitSource::analyze(
        "unit-capability.ko",
        r#"
        fun unit(): Unit { println("first") }
        fun main(): Unit { for (item in arrayOf<Unit>(unit())) { val copy: Unit = item } }
        "#,
    )
    .expect("original source must pass strict Unit admission");
    let second = UnitSource::analyze(
        "unit-capability.ko",
        r#"
        fun unit(): Unit { println("other") }
        fun main(): Unit { for (item in arrayOf<Unit>(unit())) { val copy: Unit = item } }
        "#,
    )
    .expect("original source must pass strict Unit admission");
    for analysis in [&first, &second] {
        analysis.assert_frontend_clean();
    }
    let directory = TestDirectory::create();
    let output = directory.join("preserved.o");
    std::fs::write(&output, b"previous object bytes").unwrap();
    for (typed, owned) in [(&first, &second), (&second, &second)] {
        let error = first
            .emit_with_products_from(
                typed,
                owned,
                first.declaration("main", SymbolKind::Function),
                &output,
            )
            .expect_err("foreign Unit facts must fail before object publication");
        assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
        assert_eq!(std::fs::read(&output).unwrap(), b"previous object bytes");
    }
}
}
mod unit_storage_tests {
//! SPEC-0248: native Unit containers retain source order and logical cardinality.

use super::super::UnitSource;
use super::emit_link_and_run;
use crate::native_tests::boxed_enum_tests::unit_source::SourceRejectionKind;
use crate::test_support::diagnostics::{assert_eq_diagnostic, diagnostic};

fn check(
    filename: &str,
    provider: &str,
    constructor: &str,
    arguments: &str,
    expected_count: usize,
    expected_stdout: &[u8],
) {
    let source = format!(
        r#"
        fun first(): Unit {{ println("first") }}
        fun second(): Unit {{ println("second") }}
        fun third(): Unit {{ println("third") }}
        fun source(): {provider}<Unit> {{
            println("source")
            return {constructor}<Unit>({arguments})
        }}
        fun main(): Unit {{
            var count = 0
            for (item in source()) {{
                count += 1
                val copy: Unit = item
                println("body")
            }}
            if (count == {expected_count}) {{ println("count-ok") }}
            println("done")
        }}
        "#,
    );
    diagnostic!("{filename}:\n{source}");
    let run = emit_link_and_run(filename, &source, "main");
    assert!(run.status.success(), "{filename}: {run:?}");
    assert_eq!(run.stdout, expected_stdout, "{filename}: {run:?}");
    assert!(run.stderr.is_empty(), "{filename}: {run:?}");
}

#[test]
fn unit_array_empty_runs_natively() {
    check(
        "unit_array_empty.ko",
        "Array",
        "arrayOf",
        "",
        0,
        b"source\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_array_single_runs_natively() {
    check(
        "unit_array_single.ko",
        "Array",
        "arrayOf",
        "first()",
        1,
        b"source\nfirst\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_array_multi_runs_natively() {
    check(
        "unit_array_multi.ko",
        "Array",
        "arrayOf",
        "first(), second(), third()",
        3,
        b"source\nfirst\nsecond\nthird\nbody\nbody\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_list_empty_runs_natively() {
    check(
        "unit_list_empty.ko",
        "List",
        "listOf",
        "",
        0,
        b"source\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_list_single_runs_natively() {
    check(
        "unit_list_single.ko",
        "List",
        "listOf",
        "first()",
        1,
        b"source\nfirst\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_list_multi_runs_natively() {
    check(
        "unit_list_multi.ko",
        "List",
        "listOf",
        "first(), second(), third()",
        3,
        b"source\nfirst\nsecond\nthird\nbody\nbody\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_mutable_list_empty_runs_natively() {
    check(
        "unit_mutable_list_empty.ko",
        "MutableList",
        "mutableListOf",
        "",
        0,
        b"source\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_mutable_list_single_runs_natively() {
    check(
        "unit_mutable_list_single.ko",
        "MutableList",
        "mutableListOf",
        "first()",
        1,
        b"source\nfirst\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_mutable_list_multi_runs_natively() {
    check(
        "unit_mutable_list_multi.ko",
        "MutableList",
        "mutableListOf",
        "first(), second(), third()",
        3,
        b"source\nfirst\nsecond\nthird\nbody\nbody\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_discard_binding_advances_logical_cursor_natively() {
    let source = r#"
        fun first(): Unit { println("first") }
        fun second(): Unit { println("second") }
        fun third(): Unit { println("third") }
        fun source(): List<Unit> {
            println("source")
            return listOf<Unit>(first(), second(), third())
        }
        fun main(): Unit {
            var count = 0
            for (_ in source()) {
                count += 1
                println("body")
            }
            if (count == 3) { println("count-ok") }
            println("done")
        }
    "#;
    let run = emit_link_and_run("unit_list_discard.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"source\nfirst\nsecond\nthird\nbody\nbody\nbody\ncount-ok\ndone\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_named_binding_read_after_if_runs_natively() {
    let source = r#"
        fun first(): Unit { println("first") }
        fun main(): Unit {
            var count = 0
            for (item in arrayOf<Unit>(first())) {
                if (count == 0) { println("branch") }
                val copy: Unit = item
                count += 1
                println("body")
            }
            if (count == 1) { println("count-ok") }
        }
    "#;
    let run = emit_link_and_run("unit_array_read_after_if.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"first\nbranch\nbody\ncount-ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_empty_mutable_list_constructor_runs_natively() {
    let source = r#"
        fun source(): MutableList<Unit> {
            println("source")
            return MutableList<Unit>()
        }
        fun main(): Unit {
            var count = 0
            for (item in source()) {
                count += 1
                val copy: Unit = item
                println("body")
            }
            if (count == 0) { println("count-ok") }
            println("done")
        }
    "#;
    let run = emit_link_and_run("unit_empty_mutable_list_constructor.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"source\ncount-ok\ndone\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_value_return_retains_l0087_at_the_returned_expression() {
    let text = "fun unitCall(): Unit {}\nfun invalid(): Unit { return unitCall() }";
    let Err(analysis) = UnitSource::analyze("unit_invalid_return.ko", text) else {
        panic!("the original Unit-valued return must be rejected by types");
    };
    assert!(matches!(analysis.kind, SourceRejectionKind::Diagnostics));
    assert_eq!(
        analysis.report.stages,
        [("parse", 0), ("names", 0), ("types", 1)]
    );
    let diagnostics = &analysis.report.diagnostics;
    assert_eq_diagnostic!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "types");
    let diagnostic = &diagnostics[0].1;
    assert_eq!(diagnostic.code().to_string(), "L0087");
    let source = diagnostic.primary_span().source_id();
    assert_eq!(
        analysis.sources.source_name(source).unwrap(),
        "unit_invalid_return.ko"
    );
    assert_eq!(analysis.sources.source_text(source).unwrap(), text);
    let start = text.rfind("unitCall()").unwrap();
    let expected = analysis
        .sources
        .span(source, start, start + "unitCall()".len())
        .unwrap();
    assert_eq!(diagnostic.primary_span(), expected);
    assert_eq!(analysis.sources.slice(expected).unwrap(), "unitCall()");
}

#[test]
fn unit_source_element_replacement_retains_l0135_at_the_target() {
    let text = r#"
        fun unitCall(): Unit {}
        fun invalid(): Unit {
            var xs = arrayOf<Unit>(unitCall())
            for (item in xs) {
                val copy: Unit = item
                xs[0] = unitCall()
            }
        }
    "#;
    let Err(analysis) = UnitSource::analyze("unit_invalid_source_replacement.ko", text) else {
        panic!("the original iteration source mutation must be rejected by ownership");
    };
    assert!(matches!(analysis.kind, SourceRejectionKind::Diagnostics));
    assert_eq!(
        analysis.report.stages,
        [("parse", 0), ("names", 0), ("types", 0), ("ownership", 1)]
    );
    let diagnostics = &analysis.report.diagnostics;
    assert_eq_diagnostic!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "ownership");
    let diagnostic = &diagnostics[0].1;
    assert_eq!(diagnostic.code().to_string(), "L0135");
    let source = diagnostic.primary_span().source_id();
    assert_eq!(
        analysis.sources.source_name(source).unwrap(),
        "unit_invalid_source_replacement.ko"
    );
    assert_eq!(analysis.sources.source_text(source).unwrap(), text);
    let start = text.find("xs[0]").unwrap();
    let expected = analysis
        .sources
        .span(source, start, start + "xs[0]".len())
        .unwrap();
    assert_eq!(diagnostic.primary_span(), expected);
    assert_eq!(analysis.sources.slice(expected).unwrap(), "xs[0]");
}
}

use super::emit_link_and_run;

#[test]
fn array_iteration_sum_runs_natively() {
    let source = r#"
        fun sum_array(xs: Array<Int>): Int {
            var sum = 0
            for (x in xs) {
                sum = sum + x
            }
            return sum
        }

        fun main(): Unit {
            val sum = sum_array(arrayOf(10, 20, 30))
            if (sum == 60) {
                println("array_sum_ok")
            }
        }
    "#;
    let run = emit_link_and_run("array_sum.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"array_sum_ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn list_iteration_sum_runs_natively() {
    let source = r#"
        fun sum_list(xs: List<Int>): Int {
            var sum = 0
            for (x in xs) {
                sum = sum + x
            }
            return sum
        }

        fun main(): Unit {
            val sum = sum_list(listOf(1, 2, 3, 4, 5))
            if (sum == 15) {
                println("list_sum_ok")
            }
        }
    "#;
    let run = emit_link_and_run("list_sum.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"list_sum_ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn mutable_list_with_break_and_continue_runs_natively() {
    let source = r#"
        fun eval_mutable(xs: MutableList<Int>): Int {
            var sum = 0
            for (x in xs) {
                if (x < 0) {
                    continue
                }
                if (x > 100) {
                    break
                }
                sum = sum + x
            }
            return sum
        }

        fun main(): Unit {
            val sum = eval_mutable(mutableListOf(10, -5, 20, 150, 30))
            if (sum == 30) {
                println("break_continue_ok")
            }
        }
    "#;
    let run = emit_link_and_run("break_continue.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"break_continue_ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn temporary_container_iteration_runs_natively() {
    let source = r#"
        fun main(): Unit {
            var sum = 0
            for (x in listOf(100, 200, 300)) {
                sum = sum + x
            }
            if (sum == 600) {
                println("temporary_ok")
            }
        }
    "#;
    let run = emit_link_and_run("temporary.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"temporary_ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn destructuring_iteration_runs_natively() {
    let source = r#"
        value class Pair(val first: Int, val second: Int)

        fun sum_pairs(pairs: Array<Pair>): Int {
            var sum = 0
            for ((a, b) in pairs) {
                sum = sum + a + b
            }
            return sum
        }

        fun main(): Unit {
            val sum = sum_pairs(arrayOf(Pair(1, 10), Pair(2, 20), Pair(3, 30)))
            if (sum == 66) {
                println("destructure_ok")
            }
        }
    "#;
    let run = emit_link_and_run("destructure.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"destructure_ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn nested_iteration_runs_natively() {
    let source = r#"
        fun sum_matrix(matrix: Array<Array<Int>>): Int {
            var sum = 0
            for (row in matrix) {
                for (col in row) {
                    sum = sum + col
                }
            }
            return sum
        }

        fun main(): Unit {
            val matrix = arrayOf(
                arrayOf(1, 2, 3),
                arrayOf(4, 5, 6)
            )
            val sum = sum_matrix(matrix)
            if (sum == 21) {
                println("nested_ok")
            }
        }
    "#;
    let run = emit_link_and_run("nested.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"nested_ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn early_return_from_for_runs_natively() {
    let source = r#"
        fun find_target(xs: Array<Int>, target: Int): Boolean {
            for (x in xs) {
                if (x == target) {
                    return true
                }
            }
            return false
        }

        fun main(): Unit {
            val xs = arrayOf(5, 10, 15, 20)
            val found = find_target(xs, 15)
            val not_found = find_target(xs, 42)
            if (found && !not_found) {
                println("early_return_ok")
            }
        }
    "#;
    let run = emit_link_and_run("early_return.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"early_return_ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unicode_string_borrow_iteration_runs_natively() {
    let source = r#"
        fun print_all(strings: Array<String>): Unit {
            for (s in strings) {
                println(s)
            }
        }

        fun main(): Unit {
            print_all(arrayOf("hello", "世界", "Koven"))
        }
    "#;
    let run = emit_link_and_run("unicode_strings.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, "hello\n世界\nKoven\n".as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");
}

fn run_temporary_source_boundaries(
    provider: &str,
    constructor: &str,
    cases: &[(&str, &str, &[u8])],
) {
    for &(cardinality, arguments, expected) in cases {
        let source_name = format!("temporary_{provider}_{cardinality}.ko");
        let source = format!(
            r#"
                fun source(): {provider}<Int> {{
                    println("source")
                    return {constructor}<Int>({arguments})
                }}

                fun main(): Unit {{
                    for (x in source()) {{
                        println("body")
                        if (x == 7) {{
                            println("seven")
                        }}
                        if (x == 2) {{
                            println("two")
                        }}
                        if (x == 9) {{
                            println("nine")
                        }}
                    }}
                    println("done")
                }}
            "#
        );
        diagnostic!("{source_name}:\n{source}");
        let run = emit_link_and_run(&source_name, &source, "main");
        assert!(run.status.success(), "{source_name}: {run:?}");
        assert_eq!(run.stdout, expected, "{source_name}: {run:?}");
        assert!(run.stderr.is_empty(), "{source_name}: {run:?}");
    }
}

#[test]
fn temporary_array_source_boundaries_run_natively() {
    run_temporary_source_boundaries(
        "Array",
        "arrayOf",
        &[
            ("empty", "", b"source\ndone\n"),
            ("single", "7", b"source\nbody\nseven\ndone\n"),
            (
                "multi",
                "7, 2, 9",
                b"source\nbody\nseven\nbody\ntwo\nbody\nnine\ndone\n",
            ),
        ],
    );
}

#[test]
fn temporary_list_source_boundaries_run_natively() {
    run_temporary_source_boundaries(
        "List",
        "listOf",
        &[
            ("empty", "", b"source\ndone\n"),
            ("single", "7", b"source\nbody\nseven\ndone\n"),
            (
                "multi",
                "7, 2, 9",
                b"source\nbody\nseven\nbody\ntwo\nbody\nnine\ndone\n",
            ),
        ],
    );
}

#[test]
fn temporary_mutable_list_source_boundaries_run_natively() {
    run_temporary_source_boundaries(
        "MutableList",
        "mutableListOf",
        &[
            ("empty", "", b"source\ndone\n"),
            ("single", "7", b"source\nbody\nseven\ndone\n"),
            (
                "multi",
                "7, 2, 9",
                b"source\nbody\nseven\nbody\ntwo\nbody\nnine\ndone\n",
            ),
        ],
    );
}

#[allow(unused_imports)] pub(crate) use cleanup_tests::conditional_owned_source_tests;
