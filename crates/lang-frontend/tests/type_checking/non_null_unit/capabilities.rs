//! 原来源类别矩阵与条件 Copyable 类型事实，不授予 ownership 能力。
use super::checked_entries;

#[test]
fn non_null_assertion_records_extraction_candidates_without_deciding_ownership() {
    use lang_frontend::type_checking::{
        Copyability, ExpressionCategory, NullableWhenSubjectCategory as Source,
    };
    for (inner, copyability) in [
        ("Int", Copyability::Copyable),
        ("Node", Copyability::MoveOnly),
    ] {
        for (declaration, operand, category) in [
            (
                format!("fun read(own x: {inner}?): {inner} = x!!"),
                "x",
                Source::OwnedRoot,
            ),
            (
                format!("fun read(x: {inner}?): {inner} = x!!"),
                "x",
                Source::BorrowRoot,
            ),
            (
                format!("fun read(inout x: {inner}?): {inner} = (x)!!"),
                "(x)",
                Source::InoutRoot,
            ),
            (
                format!(
                    "class Holder(val item: {inner}?)\nfun read(h: Holder): {inner} = h.item!!"
                ),
                "h.item",
                Source::OrdinaryField,
            ),
            (
                format!("fun read(xs: Array<{inner}?>): {inner} = xs[0]!!"),
                "xs[0]",
                Source::ContainerElement,
            ),
            (
                format!("fun make(): {inner}? = null\nfun read(): {inner} = make()!!"),
                "make()",
                Source::Temporary,
            ),
        ] {
            let text = format!("class Node {{}}\n{declaration}");
            let (sources, parsed, entries) = checked_entries(&text);
            for (_, _, typed) in entries {
                assert!(
                    typed.diagnostics().is_empty(),
                    "{text}: {:?}",
                    typed.diagnostics()
                );
                let [plan] = typed.non_null_assertions() else {
                    panic!("{text}")
                };
                assert_eq!(plan.source_category(), category, "{text}");
                assert_eq!(plan.copyability(), copyability, "{text}");
                assert_eq!(
                    plan.category(),
                    if category == Source::Temporary {
                        ExpressionCategory::Temporary
                    } else {
                        ExpressionCategory::Place
                    }
                );
                assert_eq!(
                    sources
                        .slice(
                            parsed
                                .ast()
                                .expressions()
                                .get(plan.operand().expression())
                                .unwrap()
                                .span()
                        )
                        .unwrap(),
                    operand
                );
                assert_eq!(sources.slice(plan.operator_span()).unwrap(), "!!");
            }
        }
    }
}

#[test]
fn non_null_assertion_preserves_conditional_copyability() {
    use lang_frontend::type_checking::Copyability;
    for (parameter, expected) in [
        ("T: Copyable", Copyability::Copyable),
        ("T", Copyability::MoveOnly),
    ] {
        let (_, _, entries) =
            checked_entries(&format!("fun <{parameter}> read(own x: T?): T = x!!"));
        for (_, _, typed) in entries {
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let [plan] = typed.non_null_assertions() else {
                panic!("one generic assertion")
            };
            assert_eq!(plan.copyability(), expected);
            assert!(typed.types().get(plan.inner_type()).is_some());
            assert_eq!(typed.copyability(plan.inner_type()), expected);
        }
    }
}
