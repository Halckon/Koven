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

fn assert_map_source_lowers(source: &str) {
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
}
