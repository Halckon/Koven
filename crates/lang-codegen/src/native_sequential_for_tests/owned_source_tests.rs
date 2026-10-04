//! SPEC-0182 owned/Borrow sources: independent native stdout across 72 cells.
use super::super::{SymbolKind, analyze, boxed_enum_tests, emit_link_and_run, symbol};

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
                let analysis = analyze("resource-elements.ko", &source);
                let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
                    &analysis.sources,
                    &analysis.parsed,
                    &analysis.names,
                    &analysis.typed,
                    &analysis.owned,
                    symbol(&analysis, "entry", SymbolKind::Function),
                )
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
