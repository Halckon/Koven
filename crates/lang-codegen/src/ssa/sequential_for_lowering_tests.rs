use lang_frontend::{
    lexer::lex,
    name_resolution::resolve_names,
    ownership_checking::check_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

use super::{
    lower_frontend::orchestrate::lower_scalar_file, render::render_program, verify::verify_program,
};

fn analyze(source_text: &str) -> super::model::Program {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("sequential-for.ko", source_text)
        .expect("source");
    let lexed = lex(&sources, source).expect("lexer");
    let parsed = parse_file(&sources, &lexed).expect("parser");
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let program = lower_scalar_file(&sources, &parsed, &names, &typed, &owned)
        .expect("sequential for loop must lower");
    verify_program(&program).expect("lowered for loop SSA must verify");
    program
}

#[test]
fn borrowed_named_array_iteration_lowers_to_verified_ssa_cfg() {
    let text = r#"
        fun sum(xs: Array<Int>): Int {
            var total = 0
            for (x in xs) {
                total = total + x
            }
            return total
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("container.element_place"), "{ssa}");
    assert!(ssa.contains("borrow.shared"), "{ssa}");
    assert!(ssa.contains("end_borrow"), "{ssa}");
}

#[test]
fn discard_binding_lowers_to_verified_ssa() {
    let text = r#"
        fun count(xs: List<Int>): Int {
            var c = 0
            for (_ in xs) {
                c = c + 1
            }
            return c
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("container.element_place"), "{ssa}");
    assert!(ssa.contains("end_borrow"), "{ssa}");
}

#[test]
fn temporary_list_source_iteration_lowers_to_verified_ssa() {
    let text = r#"
        fun test(): Int {
            var total = 0
            for (x in listOf(10, 20, 30)) {
                total = total + x
            }
            return total
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("container.element_place"), "{ssa}");
    assert!(ssa.contains("end_borrow"), "{ssa}");
}

#[test]
fn mutable_list_with_break_and_continue_lowers_to_verified_ssa() {
    let text = r#"
        fun test(xs: MutableList<Int>): Int {
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
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("container.element_place"), "{ssa}");
}

#[test]
fn early_return_from_for_loop_lowers_to_verified_ssa() {
    let text = r#"
        fun find_first_even(xs: Array<Int>): Int {
            for (x in xs) {
                if (x == 2) {
                    return x
                }
            }
            return -1
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("return %"), "{ssa}");
}

#[test]
fn borrowed_destructuring_iteration_lowers_to_verified_ssa() {
    let text = r#"
        value class Pair(val first: Int, val second: Int)
        fun sum_pairs(pairs: Array<Pair>): Int {
            var total = 0
            for ((a, b) in pairs) {
                total = total + a + b
            }
            return total
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
    assert!(ssa.contains("container.element_place"), "{ssa}");
}

#[test]
fn nested_for_loops_lower_to_verified_ssa() {
    let text = r#"
        fun matrix_sum(rows: Array<Array<Int>>): Int {
            var total = 0
            for (row in rows) {
                for (x in row) {
                    total = total + x
                }
            }
            return total
        }
    "#;
    let program = analyze(text);
    let ssa = render_program(&program);
    assert!(ssa.contains("container.length"), "{ssa}");
}
