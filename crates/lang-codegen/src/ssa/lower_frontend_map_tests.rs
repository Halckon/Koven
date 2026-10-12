//! Map 查询 runtime 合同尚未启用的源码边界。
use super::{LoweringErrorKind, analyze, lower_scalar_file};

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
    let analysis = analyze(source);
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{source}: {:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{source}: {:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{source}: {:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{source}: {:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    crate::llvm::render_verified_program(&program)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    program
}

#[test]
fn map_remove_temporary_null_comparison_inspects_real_drop_facts() {
    use lang_frontend::ownership_checking::DropTarget;
    let source = "fun entry() { var m = mutableMapOf<Int, String>(); m.put(1, \"text\".clone()); val present = m.remove(1) != null; if (present) { println(\"present\") } }";
    let analysis = analyze(source);
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let removed = analysis.typed.map_removes()[0].expression();
    let drops = analysis
        .owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == DropTarget::Temporary(removed))
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        1,
        "frontend publishes the actual result cleanup"
    );
    assert!(matches!(
        drops[0].point(),
        lang_frontend::ownership_checking::DropPoint::AfterBinaryOperands(_)
    ));
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("temporary result consumes its explicit frontend cleanup fact");
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
fn map_remove_single_for_guarded_owner_boundary_is_explicit() {
    use lang_frontend::ownership_checking::{
        CleanupOwnerValue, DropTarget, IterationCleanupAction, LoanTarget,
    };
    let source = include_str!(
        "../../tests/fixtures/owned-null-cleanup-0296/single-loop-boundary.ko"
    );
    let analysis = analyze(source);
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    assert!(
        !analysis
            .owned
            .cleanup_steps()
            .iter()
            .any(|(_, action)| matches!(
                action,
                IterationCleanupAction::ReleaseClosureInstances { .. }
                    | IterationCleanupAction::ReleaseRetainedClosureSources { .. }
                    | IterationCleanupAction::TestLastCaptureLoan { .. }
                    | IterationCleanupAction::SaveClosureCapture { .. }
                    | IterationCleanupAction::SaveOwnerSnapshot { .. }
            )),
        "{:?}",
        analysis.owned.cleanup_steps()
    );
    let guarded = analysis
        .owned
        .drops()
        .iter()
        .find(|fact| fact.condition().is_some())
        .expect("ordinary for publishes guarded owner cleanup");
    let DropTarget::Named(symbol) = guarded.target() else {
        panic!("outer named owner")
    };
    let Some(CleanupOwnerValue::IterationPhi {
        statement,
        symbol: owner_symbol,
        ..
    }) = analysis
        .owned
        .cleanup_conditions()
        .owner_value(guarded.owner().unwrap())
    else {
        panic!("loop owner phi")
    };
    assert_eq!(*owner_symbol, symbol);
    let plan = analysis.owned.iteration(*statement).unwrap();
    assert!(!matches!(plan.source(), LoanTarget::Place(place) if place.root() == symbol));
    assert_eq!(
        analysis
            .typed
            .is_resource_type(analysis.typed.symbol_type(symbol).unwrap()),
        Some(false)
    );
    let error = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .err()
    .expect("guarded owner has no single-file carrier");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(guarded.value_origin()));
}
