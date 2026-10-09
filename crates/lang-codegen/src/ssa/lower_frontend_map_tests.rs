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
