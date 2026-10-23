//! SPEC-0246：单文件 direct-field commit 与未扩展边界。
use super::*;
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;

fn analyze_field(source: &str) -> UnitSource {
    let analysis = UnitSource::analyze("lowering.ko", source)
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    analysis.assert_frontend_clean();
    analysis
}

fn lower_field(
    analysis: &UnitSource,
) -> Result<crate::ssa::model::Program, crate::ssa::LoweringError> {
    let entry = analysis.declaration("test", lang_frontend::name_resolution::SymbolKind::Function);
    analysis.lower_entry(entry).map(|(program, _)| program)
}

#[test]
fn field_replace_single_uses_dedicated_exchange_and_preserves_parent() {
    let analysis = analyze_field(
        r#"
        class Holder(var state: String, val id: Int)
        fun test(): String {
            val holder = Holder("old", 1)
            val old = replace(&holder.state, "new")
            if (holder.id == 1) println(old)
            return replace(&holder.state, "last")
        }
    "#,
    );
    let program = lower_field(&analysis).expect("direct field exchange");
    let rendered = render_program(&program);
    assert!(rendered.contains("heap.field_exchange"), "{rendered}");
    assert!(!rendered.contains("root_replace"), "{rendered}");
    render_verified_program(&program).expect("dedicated exchange verifies");
}

#[test]
fn field_replace_single_existing_field_storage_families() {
    for (declarations, ty, first, second) in [
        ("", "Int", "1", "2"),
        ("", "Boolean", "true", "false"),
        ("class Node(val id: Int)", "Node", "Node(1)", "Node(2)"),
        ("class Node(val id: Int)", "Node?", "Node(1)", "Node(2)"),
        (
            "value class Inline(val id: Int)",
            "Inline",
            "Inline(1)",
            "Inline(2)",
        ),
        (
            "value class Inline(val id: Int)",
            "Box<Inline>",
            "Box(Inline(1))",
            "Box(Inline(2))",
        ),
        ("", "Rc<Int>", "Rc(1)", "Rc(2)"),
        ("", "List<Int>", "listOf(1)", "listOf(2)"),
    ] {
        let source = format!(
            "{declarations}\nclass Holder(var state: {ty})\nfun test(): Unit {{ val initial: {ty} = {first}\nval holder = Holder(initial)\nval old = replace(&holder.state, {second}) }}"
        );
        let analysis = analyze_field(&source);
        let program =
            lower_field(&analysis).unwrap_or_else(|failure| panic!("{source}\n{failure:?}"));
        render_verified_program(&program).expect("existing field storage verifies");
    }
}

#[test]
fn field_replace_single_unclosed_projections_stay_unsupported() {
    for source in [
        "class Inner(var value: Int)\nclass Outer(val inner: Inner)\nfun test(): Unit { val holder = Outer(Inner(1))\nval old = replace(&holder.inner.value, 2) }",
        "class Holder<T>(var value: T)\nfun test(): Unit { val holder = Holder(1)\nval old = replace(&holder.value, 2) }",
        "class Holder(var a: Int, var b: Int)\nfun test(): Unit { val holder = Holder(1, 2)\nswap(&holder.a, &holder.b) }",
        "class Holder(var a: Int, var b: Int)\nfun test(): Unit { val holder = Holder(1, 2)\nval old = replace(&holder.a, replace(&holder.b, 3)) }",
        "class Holder(var a: Int, var b: Int)\nfun test(): Unit { val holder = Holder(1, 2)\nval old = replace(&holder.a, holder.b) }",
        "class Holder(var a: String, var b: String)\nfun test(): Unit { val holder = Holder(\"a\", \"b\")\nval old = replace(&holder.a, holder.b.clone()) }",
    ] {
        let analysis = analyze_field(source);
        assert_eq!(
            lower_field(&analysis).err().expect("unsupported").kind,
            LoweringErrorKind::UnsupportedNode,
            "{source}"
        );
    }
}
