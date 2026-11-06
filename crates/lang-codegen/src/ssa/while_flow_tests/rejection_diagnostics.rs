//! Exact negative diagnostic oracles with full failure evidence on disk.
use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};

pub(super) fn assert_rejection(group: &str, source: &str, unit: bool, error: &str) {
    let offset = if unit { "package p\n".len() } else { 0 };
    let primary = |start: usize, length: usize| {
        format!(
            "primary_span: Span {{ source_id: SourceId(0), start: {}, end: {} }}",
            start + offset,
            start + offset + length
        )
    };
    if group == "pending-child" {
        assert_diagnostic!(error.starts_with("ownership"), "{error}");
        assert_eq_diagnostic!(
            error.matches("Diagnostic { severity:").count(),
            1,
            "{error}"
        );
        assert_eq_diagnostic!(error.matches("code: L0135,").count(), 1, "{error}");
        let mutation = source.find("current = null").unwrap();
        let borrowed = source.find("current.text").unwrap();
        assert_diagnostic!(
            error.contains(&primary(mutation, "current".len())),
            "{error}"
        );
        let label = format!(
            "Label(DiagnosticLabel {{ span: Span {{ source_id: SourceId(0), start: {}, end: {} }}, message: DiagnosticText(\"conflicting loan starts here\") }})",
            borrowed + offset,
            borrowed + offset + "current.text".len()
        );
        assert_diagnostic!(error.contains(&label), "{error}");
    } else {
        assert_diagnostic!(error.starts_with("complete:"), "{error}");
        let count = if group == "same-root" { 2 } else { 1 };
        assert_eq_diagnostic!(
            error.matches("Diagnostic { severity:").count(),
            count,
            "{error}"
        );
        assert_eq_diagnostic!(error.matches("code: L0165,").count(), count, "{error}");
        for (start, _) in source.rmatch_indices("current.value").take(count) {
            assert_diagnostic!(
                error.contains(&primary(start, "current.value".len())),
                "{error}"
            );
        }
    }
}
