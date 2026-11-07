//! Guide 的顶层声明拒绝与 initializer 溢出恢复同时保留。
use super::{checked_entries, codes};

#[test]
fn top_level_runtime_rejection_preserves_numeric_overflow_recovery_in_both_entries() {
    let magnitude = "340282366920938463463374607431768211456";
    // 第一份源码与旧 numeric_overflow Single witness 逐字一致，旧 witness 不删除。
    for (text, expected) in [
        (
            "val huge = 340282366920938463463374607431768211456",
            vec![("L0167", "val"), ("L0090", magnitude)],
        ),
        (
            "var huge = 340282366920938463463374607431768211456",
            vec![("L0167", "var"), ("L0090", magnitude)],
        ),
        (
            "const val huge = 340282366920938463463374607431768211456",
            vec![("L0090", magnitude)],
        ),
        (
            "fun huge(): Unit { val value = 340282366920938463463374607431768211456 }",
            vec![("L0090", magnitude)],
        ),
        ("val value = 1", vec![("L0167", "val")]),
    ] {
        let (sources, parsed, entries) = checked_entries(text);
        for (_, typed) in entries {
            assert_eq!(
                codes(typed.diagnostics()),
                expected.iter().map(|(code, _)| *code).collect::<Vec<_>>(),
                "{text}"
            );
            for (diagnostic, (_, marker)) in typed.diagnostics().iter().zip(&expected) {
                let span = diagnostic.primary_span();
                assert_eq!(span.source_id(), parsed.source_id());
                assert_eq!(sources.slice(span).unwrap(), *marker);
                let start = text.find(marker).unwrap();
                assert_eq!(span.start(), start);
                assert_eq!(span.end(), start + marker.len());
            }
            assert!(typed.validate().is_err(), "invalid source cannot validate");
        }
    }
}
