use lang_frontend::{
    lexer::lex,
    name_resolution::resolve_names,
    ownership_checking::check_ownership,
    parser::parse_file,
    source::{SourceId, SourceMap},
    type_checking::{check_types, standard_environments},
};

use super::super::{
    lower_frontend::orchestrate::lower_scalar_file,
    model::{Operation, Origin},
    render::render_program,
    verify::verify_program,
};
use crate::llvm::render_verified_program;

struct Artifacts {
    source: SourceId,
    ssa: String,
    llvm: String,
}

fn analyze_and_render(case: &str, text: &str, expected_lengths: usize) -> Artifacts {
    // Every call owns a fresh complete frontend chain. The stable input key is the same
    // logical filename, source bytes and one-source registration order, not map identity.
    let mut sources = SourceMap::new();
    let source = sources.add_source(case, text).expect("unique case source");
    let lexed = lex(&sources, source).expect("lexing");
    let parsed = parse_file(&sources, &lexed).expect("parsing");
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("name resolution");
    let typed = check_types(&sources, &parsed, &names, &types).expect("type checking");
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership checking");
    for actual in [
        parsed.source_id(),
        names.source_id(),
        typed.source_id(),
        owned.source_id(),
    ] {
        assert_eq!(actual, source, "{case}: frontend chain source identity");
    }
    assert!(
        lexed.diagnostics().is_empty(),
        "{case}: {:?}",
        lexed.diagnostics()
    );
    assert!(
        parsed.diagnostics().is_empty(),
        "{case}: {:?}",
        parsed.diagnostics()
    );
    assert!(
        names.diagnostics().is_empty(),
        "{case}: {:?}",
        names.diagnostics()
    );
    assert!(
        typed.diagnostics().is_empty(),
        "{case}: {:?}",
        typed.diagnostics()
    );
    assert!(
        owned.diagnostics().is_empty(),
        "{case}: {:?}",
        owned.diagnostics()
    );
    let program = lower_scalar_file(&sources, &parsed, &names, &typed, &owned)
        .unwrap_or_else(|error| panic!("{case}: lowering: {error:?}\n{text}"));
    verify_program(&program).unwrap_or_else(|error| panic!("{case}: SSA: {error:?}"));

    let mut length_count = 0;
    let check_span = |span: lang_frontend::source::Span| {
        assert_eq!(span.source_id(), source, "{case}: foreign SSA origin");
        assert_eq!(sources.source_name(span.source_id()).unwrap(), case);
        sources
            .slice(span)
            .expect("origin retains valid byte bounds");
    };
    let check_origin = |origin: &Origin| check_span(origin.span());
    for module in &program.modules {
        for origin in module.type_origins.values() {
            check_span(origin.primary);
            check_span(origin.declaration);
        }
        for function in &module.functions {
            check_origin(&function.origin);
            for entity in function
                .values
                .iter()
                .chain(&function.places)
                .chain(&function.loans)
            {
                check_origin(&entity.origin);
            }
            for block in &function.blocks {
                check_origin(&block.origin);
                check_origin(&block.terminator.as_ref().expect("terminator").origin);
            }
            for instruction in &function.instructions {
                check_origin(&instruction.origin);
                length_count += usize::from(matches!(
                    instruction.operation,
                    Operation::ContainerLength { .. }
                ));
            }
        }
    }
    assert_eq!(length_count, expected_lengths, "{case}: actual for loops");

    // Keep complete renderer output. SourceId's existing Debug contract hides only the
    // SourceMap owner token; source index, byte offsets and synthetic reasons remain.
    // SSA-local type/function/block/entity IDs, operations, operands and order remain too.
    let ssa = render_program(&program);
    let llvm = render_verified_program(&program)
        .unwrap_or_else(|error| panic!("{case}: LLVM: {error:?}\n{text}"));
    Artifacts { source, ssa, llvm }
}

fn assert_fresh_analyses_are_deterministic(case: &str, text: &str, expected_lengths: usize) {
    let first = analyze_and_render(case, text, expected_lengths);
    let second = analyze_and_render(case, text, expected_lengths);
    assert_ne!(
        first.source, second.source,
        "{case}: must use independent maps"
    );
    assert_eq!(
        first.ssa, second.ssa,
        "{case}: complete SSA differs\n{text}"
    );
    // Both calls use the same native target/options and module name. Do not strip
    // ModuleID/source_filename, target layout, symbol names, metadata or instructions.
    // This is same-host reproducibility, not cross-target or object/DWARF equivalence.
    assert_eq!(
        first.llvm, second.llvm,
        "{case}: complete LLVM differs\n{text}"
    );
}

#[test]
fn temporary_list_continue_is_deterministic_across_fresh_analyses() {
    assert_fresh_analyses_are_deterministic(
        "determinism-temporary-list-continue.ko",
        r#"
            fun source(): List<Int> = listOf(7, 2, 9)
            fun scan(): Int {
                var total = 0
                for (value in source()) {
                    if (value == 2) { continue }
                    total = total + value
                }
                return total
            }
        "#,
        1,
    );
}

#[test]
fn mixed_array_destructuring_is_deterministic_across_fresh_analyses() {
    assert_fresh_analyses_are_deterministic(
        "determinism-mixed-array.ko",
        r#"
            value class Parts(val count: Int, val label: String)
            fun scan(parts: Array<Parts>): Int {
                for ((count, label) in parts) {
                    println(label)
                    if (count > 0) { return count }
                }
                return 0
            }
        "#,
        1,
    );
}

#[test]
fn nested_array_return_is_deterministic_across_fresh_analyses() {
    assert_fresh_analyses_are_deterministic(
        "determinism-nested-array.ko",
        r#"
            fun scan(rows: Array<Array<Int>>): Int {
                var total = 0
                for (row in rows) {
                    for (value in row) {
                        if (value == 2) { continue }
                        if (value == 9) { return total }
                        total = total + value
                    }
                }
                return total
            }
        "#,
        2,
    );
}
