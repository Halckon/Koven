//! Original return-loop body conflicts through strict Unit frontend admission.
//! No lowering/native execution and no synthetic facts or fault injection.
use crate::native_tests::boxed_enum_tests::unit_source::{SourceRejectionKind, UnitSource};
use lang_frontend::diagnostic::{DiagnosticDetail, Severity};

pub(super) fn assert_original_rejections() {
    // Original four fixture texts are unchanged. Primary and loan spans agree with
    // Legacy; Unit has more specific messages and a selected-parameter label for move.
    for (factory, operation, message, primary, loan, parameter) in [
        (
            "arrayOf",
            "consume(keys)",
            "move conflicts with an active loan",
            (121, 125),
            (105, 109),
            true,
        ),
        (
            "arrayOf",
            "keys[0] = 9",
            "mutation conflicts with an active loan",
            (113, 120),
            (105, 109),
            false,
        ),
        (
            "mutableListOf",
            "keys.add(4)",
            "new loan conflicts with an active loan",
            (119, 123),
            (111, 115),
            false,
        ),
        (
            "mutableListOf",
            "keys.removeLast()",
            "new loan conflicts with an active loan",
            (119, 123),
            (111, 115),
            false,
        ),
    ] {
        let text = format!(
            "fun consume(own source: Array<Int>): Unit {{}}\nfun main(): Unit {{ var keys = {factory}(1, 2, 3); for (key in keys) {{ {operation}; if (key == 2) {{ return }} }} }}"
        );
        let rejected = UnitSource::analyze("lowering.ko", &text)
            .err()
            .expect("body conflict cannot acquire an executable Unit capability");
        assert!(matches!(rejected.kind, SourceRejectionKind::Diagnostics));
        assert_eq!(
            rejected.report.stages,
            [("parse", 0), ("names", 0), ("types", 0), ("ownership", 1)]
        );
        let [(stage, diagnostic)] = rejected.report.diagnostics.as_slice() else {
            panic!("exactly one original ownership conflict");
        };
        assert_eq!(*stage, "ownership");
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.code().to_string(), "L0135");
        assert_eq!(diagnostic.message(), message);
        let span = diagnostic.primary_span();
        assert_eq!((span.start(), span.end()), primary);
        assert_eq!(
            rejected.sources.source_name(span.source_id()),
            Ok("lowering.ko")
        );
        assert_eq!(
            rejected.sources.source_text(span.source_id()),
            Ok(text.as_str())
        );
        assert_eq!(
            rejected.sources.slice(span),
            Ok(&text[primary.0..primary.1])
        );
        let mut labels = vec![(loan, "conflicting loan starts here")];
        if parameter {
            labels.push(((12, 34), "selected parameter declared here"));
        }
        assert_eq!(diagnostic.details().len(), labels.len());
        for (detail, (range, message)) in diagnostic.details().iter().zip(labels) {
            let DiagnosticDetail::Label(label) = detail else {
                panic!("the complete ordered details contain only source labels");
            };
            assert_eq!(label.message(), message);
            assert_eq!(label.span().source_id(), span.source_id());
            assert_eq!((label.span().start(), label.span().end()), range);
            assert_eq!(
                rejected.sources.slice(label.span()),
                Ok(&text[range.0..range.1])
            );
        }
    }
}
