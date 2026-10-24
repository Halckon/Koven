//! Map 查询 runtime 合同尚未启用的源码边界。
#[path = "lower_frontend_map_key_return_tests.rs"]
mod map_key_return_tests;
use super::{LoweringErrorKind, analyze, lower_scalar_file};

#[path = "lower_frontend_map_loop_tests.rs"]
mod loop_owner_tests;

#[test]
fn map_nullable_value_owned_remove_remains_unsupported() {
    let analysis = analyze(
        "class Token(val n: Int)\nfun inspect(inout m: MutableMap<Int, Token?>): Unit {\n val result = m.remove(1)\n}",
    );
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let Err(error) = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) else {
        panic!("nullable owned remove requires a separate Missing contract")
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}

#[test]
fn map_operands_follow_argument_control_flow() {
    for operation in [
        "m.put(if (true) { 1 } else { 2 }, 3)",
        "m.put(1, if (true) { 3 } else { 4 })",
        "m.get(if (true) { 1 } else { 2 })",
        "m.contains(if (true) { 1 } else { 2 })",
        "m.remove(if (true) { 1 } else { 2 })",
    ] {
        let source = format!(
            "fun entry(): Unit {{\n var m = mutableMapOf<Int, Int>()\n {operation}\n m.size\n}}"
        );
        assert_map_source_lowers(&source);
    }
    for key in ["key", "\"key\".clone()"] {
        let source = format!(
            "fun entry(): Unit {{\n var m = mutableMapOf<String, Int>()\n val key = \"key\".clone()\n m.put({key}, if (true) {{ 3 }} else {{ 4 }})\n m.size\n}}"
        );
        assert_map_source_lowers(&source);
    }
}

#[test]
fn map_mutation_collects_drop_glue_before_abort() {
    for (declarations, body) in [
        (
            "class Resource(val n: Int) { deinit() { println(\"drop\") } }",
            "var m = mutableMapOf<Int, Resource>()\n m.put(1, Resource(1))\n error(\"stop\")",
        ),
        (
            "",
            "var m = mutableMapOf<String, Int>()\n val key = \"key\".clone()\n m.remove(key)\n error(key)",
        ),
    ] {
        let source = format!("{declarations}\nfun entry(): Unit {{\n {body}\n}}");
        assert_map_source_lowers(&source);
    }
}

#[test]
fn map_put_control_exit_mixed_return_key() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { return } else { 8 }, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn map_put_control_exit_mixed_return_value() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, if (flag) { return } else { 8 })\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn map_put_control_exit_mixed_reversed_key() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { 8 } else { return }, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn map_put_control_exit_mixed_reversed_value() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, if (flag) { 8 } else { return })\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn map_put_control_exit_both_return_key() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { return } else { return }, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn map_put_control_exit_both_return_value() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, if (flag) { return } else { return })\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn map_put_control_exit_direct_return_key() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(return, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn map_put_control_exit_direct_return_value() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, return)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn map_put_control_exit_key_then_value() {
    let source = "fun inspect(own flag: Boolean, own other: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { return } else { 8 }, if (other) { return } else { 8 })\n m.size\n}\nfun entry(): Unit { inspect(false, false); inspect(false, true); inspect(true, false); inspect(true, true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 1);
}

fn assert_map_put_count(program: &crate::ssa::model::Program, expected: usize) {
    use crate::ssa::model::Operation;
    let function = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name.contains("inspect"))
        .unwrap();
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::MapPut { .. }))
            .count(),
        expected
    );
    if expected == 0 {
        assert!(
            !function
                .instructions
                .iter()
                .any(|instruction| matches!(instruction.operation, Operation::DirectCall { .. })),
            "a fully diverged put must not lower the suffix call"
        );
    }
}

fn assert_map_source_lowers(source: &str) -> crate::ssa::model::Program {
    lower_map_unit(source, "entry").0
}

fn lower_map_unit(
    source: &str,
    entry_name: &str,
) -> (
    crate::ssa::model::Program,
    crate::native_tests::boxed_enum_tests::unit_source::UnitSource,
) {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    let analysis = UnitSource::analyze("lowering.ko", source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    analysis.assert_frontend_clean();
    let declaration = analysis.declaration(entry_name, SymbolKind::Function);
    let (program, entry) = analysis
        .lower_entry(declaration)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    assert_eq!(
        program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap()
            .origin,
        crate::ssa::model::Origin::Source(analysis.declaration_span(declaration))
    );
    crate::llvm::render_verified_program(&program)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    (program, analysis)
}

#[test]
fn map_remove_temporary_null_comparison_inspects_real_drop_facts() {
    use lang_frontend::{
        ownership_checking::{UnitDropPoint, UnitDropTarget},
        parser::{BinaryOperator, Expression, LiteralKind},
    };
    let source = "fun entry() { var m = mutableMapOf<Int, String>(); m.put(1, \"text\".clone()); val present = m.remove(1) != null; if (present) { println(\"present\") } }";
    let (_, analysis) = lower_map_unit(source, "entry");
    let [remove] = analysis.types().map_removes() else {
        panic!("one original Map remove descriptor")
    };
    let removed = remove.expression();
    let drops = analysis
        .ownership()
        .drops()
        .iter()
        .filter(|fact| fact.target() == UnitDropTarget::Temporary(removed))
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        1,
        "frontend publishes the actual result cleanup"
    );
    let UnitDropPoint::AfterBinaryOperands(comparison) = drops[0].point() else {
        panic!("the original temporary is cleaned after the null comparison operands")
    };
    assert_eq!(comparison.source_unit(), removed.source_unit());
    assert_eq!(
        removed.source_unit(),
        analysis.names().names().index().source_units()[0].id()
    );
    let expressions = analysis.parsed().ast().expressions();
    let Expression::Binary {
        left,
        operator: BinaryOperator::NotEqual,
        right,
        ..
    } = expressions.get(comparison.expression()).unwrap().payload()
    else {
        panic!("the fact belongs to the original non-null comparison")
    };
    assert_eq!(*left, removed.expression());
    assert!(matches!(
        expressions.get(*right).unwrap().payload(),
        Expression::Literal(LiteralKind::Null)
    ));
    assert_eq!(
        drops[0].value_origin(),
        expressions.get(removed.expression()).unwrap().span()
    );
    assert_eq!(
        analysis.types().expression_type(removed),
        Some(remove.result_type())
    );
}

#[test]
fn map_remove_single_for_guarded_owner_is_carried() {
    let source =
        include_str!("../../tests/fixtures/owned-null-cleanup-0296/single-loop-boundary.ko");
    loop_owner_tests::unit_identity::check_guarded_original(source);
}
