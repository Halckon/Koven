//! Ordinary-source boundaries kept separate from the core acceptance matrix.
use super::{analyze, lower_scalar_file};
use crate::test_support::diagnostics::diagnostic;

fn observe(text: &str, supported: bool, keys_start: usize, expected_loops: usize) {
    let a = analyze(text);
    assert!(
        a.parsed.diagnostics().is_empty(),
        "{:?}",
        a.parsed.diagnostics()
    );
    assert!(
        a.names.diagnostics().is_empty(),
        "{:?}",
        a.names.diagnostics()
    );
    assert!(
        a.typed.diagnostics().is_empty(),
        "{:?}",
        a.typed.diagnostics()
    );
    assert!(
        a.owned.diagnostics().is_empty(),
        "{:?}",
        a.owned.diagnostics()
    );
    let result = lower_scalar_file(&a.sources, &a.parsed, &a.names, &a.typed, &a.owned);
    if supported {
        crate::llvm::render_verified_program(&result.unwrap()).unwrap();
    } else {
        let error = result
            .err()
            .expect("bounded source proof remains unsupported");
        let source = a.parsed.source_id();
        assert_eq!(a.sources.source_name(source), Ok("lowering.ko"));
        assert_eq!(a.sources.source_text(source), Ok(text));
        let expected_span = a.sources.span(source, keys_start, keys_start + 4).unwrap();
        assert_eq!(a.sources.slice(expected_span), Ok("keys"));
        assert_eq!(
            error,
            crate::ssa::LoweringError {
                kind: crate::ssa::LoweringErrorKind::UnsupportedNode,
                span: Some(expected_span),
            },
            "the complete original Legacy boundary stays at its declaration"
        );
        diagnostic!("BOUNDARY {text}: {error:?}");
    }

    // Unit already accepts these complete original sources. Preserve the distinct
    // Legacy rejection above rather than treating this as consumer retirement.
    use crate::{
        native_tests::boxed_enum_tests::unit_source::UnitSource,
        ssa::model::{Operation, Origin},
    };
    use lang_frontend::{
        name_resolution::{SourceUnitKey, SymbolKind},
        parser::Statement,
    };

    let unit =
        UnitSource::analyze("lowering.ko", text).expect("strict unchanged return-boundary source");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    let source = unit.parsed().source_id();
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), source);
    assert_eq!(unit.sources().source_name(source), Ok("lowering.ko"));
    assert_eq!(unit.sources().source_text(source), Ok(text));
    assert_eq!(
        unit.basic_types().types().sequential_iterations().len(),
        expected_loops
    );
    assert_eq!(unit.ownership().iterations().len(), expected_loops);
    let declaration = unit.declaration("main", SymbolKind::Function);
    let main_span = unit.declaration_span(declaration);
    assert_eq!(main_span.source_id(), source);
    let mut statements = Vec::new();
    for plan in unit.ownership().iterations() {
        let descriptor = plan.descriptor();
        let statement = descriptor.statement();
        assert_eq!(statement.source_unit(), document.id());
        assert_eq!(descriptor.source().source_unit(), document.id());
        assert!(!statements.contains(&statement));
        statements.push(statement);
        assert_eq!(
            unit.types().sequential_iteration(statement),
            Some(descriptor)
        );
        let node = unit
            .parsed()
            .ast()
            .statements()
            .get(statement.statement())
            .unwrap();
        assert_eq!(node.span().source_id(), source);
        assert!(main_span.start() <= node.span().start() && node.span().end() <= main_span.end());
        let Statement::For { source, .. } = node.payload() else {
            panic!("a real original For, not a substitute provider");
        };
        assert_eq!(descriptor.source().expression(), *source);
    }
    let (program, entry) = unit
        .lower_entry(declaration)
        .expect("the complete original return boundary already lowers through Unit");
    let [module] = program.modules.as_slice() else {
        panic!("one original source module");
    };
    let roots = module
        .functions
        .iter()
        .filter(|function| function.origin == Origin::Source(main_span))
        .collect::<Vec<_>>();
    let [function] = roots.as_slice() else {
        panic!("exactly the original main root");
    };
    assert_eq!(function.id(), entry);
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::ContainerElementPlace { .. }
            ))
            .count(),
        expected_loops
    );
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("the actual main and all original loop element accesses verify through LLVM");
}

#[test]
fn owned_source_return_group_and_same_type_decoy() {
    for live in [false, true] {
        let text = super::source("arrayOf", "Array", false, live, true)
            .replace(
                "val keys = arrayOf(1, 2, 3)",
                "var keys = ((arrayOf(1, 2, 3)))",
            )
            .replace("if (key == 2)", "val decoy = arrayOf(7, 8); if (key == 2)")
            .replace("println(\"tail\")", "decoy.size; println(\"tail\")");
        super::original_text(&text, "work", "Array", false, live, true);
    }
}

#[test]
fn owned_source_return_two_returns_remain_bounded() {
    observe(
        "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 1) { return }; if (key == 2) { return } }; keys.size }",
        false,
        23,
        1,
    );
}
#[test]
fn owned_source_return_mixed_continue_remains_bounded() {
    observe(
        "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 1) { return }; if (key == 2) { continue } }; keys.size }",
        false,
        23,
        1,
    );
}
#[test]
fn owned_source_return_replacement_seed_remains_bounded() {
    observe(
        "fun main(): Unit { var keys = arrayOf(1, 2); keys = arrayOf(2, 3); for (key in keys) { if (key == 2) { return } }; keys.size }",
        false,
        23,
        1,
    );
}
#[test]
fn owned_source_return_alias_seed_remains_bounded() {
    observe(
        "fun main(): Unit { val original = arrayOf(1, 2); val keys = original; for (key in keys) { if (key == 2) { return } }; keys.size }",
        false,
        53,
        1,
    );
}
#[test]
fn owned_source_return_resource_source_remains_bounded() {
    observe(
        "class Item(val id: Int) { deinit() {} }\nfun main(): Unit { val keys = arrayOf(Item(1), Item(2)); for (key in keys) { if (key.id == 2) { return } }; keys.size }",
        false,
        63,
        1,
    );
}
#[test]
fn owned_source_return_fresh_inner_instance() {
    super::single_text(
        "fun main(): Unit { for (outer in arrayOf(1, 2)) { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { return } }; keys.size } }",
        true,
        false,
    );
}
#[test]
fn owned_source_return_outer_owner_with_inner_loop() {
    observe(
        "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { for (other in arrayOf(3)) { other }; if (key == 2) { return } }; keys.size }",
        false,
        23,
        2,
    );
}
#[test]
fn owned_source_return_same_source_nested() {
    observe(
        "fun main(): Unit { val keys = arrayOf(1, 2); for (outer in keys) { for (key in keys) { if (key == 2) { return } } }; keys.size }",
        false,
        23,
        2,
    );
}
#[test]
fn owned_source_return_same_source_sequential() {
    observe(
        "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { return } }; for (key in keys) { if (key == 2) { return } }; keys.size }",
        false,
        23,
        2,
    );
}

#[test]
fn owned_source_return_source_loan_rejects_body_moves_and_mutation() {
    for (factory, operation) in [
        ("arrayOf", "consume(keys)"),
        ("arrayOf", "keys[0] = 9"),
        ("mutableListOf", "keys.add(4)"),
        ("mutableListOf", "keys.removeLast()"),
    ] {
        let text = format!(
            "fun consume(own source: Array<Int>): Unit {{}}\nfun main(): Unit {{ var keys = {factory}(1, 2, 3); for (key in keys) {{ {operation}; if (key == 2) {{ return }} }} }}"
        );
        let a = analyze(&text);
        assert!(a.parsed.diagnostics().is_empty());
        assert!(a.names.diagnostics().is_empty());
        assert!(a.typed.diagnostics().is_empty());
        assert!(
            a.owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135")
        );
    }
}

#[test]
fn owned_source_return_fresh_inner_exact_source_structure() {
    for live in [false, true] {
        let after = if live { "keys.size" } else { "" };
        let text = format!(
            "fun main(): Unit {{ for (outer in arrayOf(1, 2)) {{ val keys = arrayOf(1, 2); for (key in keys) {{ if (key == 2) {{ return }} }}; {after} }} }}"
        );
        super::single_text(&text, live, false);
    }
}
