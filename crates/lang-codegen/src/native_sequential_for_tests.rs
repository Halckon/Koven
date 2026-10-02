#[path = "native_sequential_for_tests/cleanup_tests.rs"]
mod cleanup_tests;

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
        eprintln!("{source_name}:\n{source}");
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
