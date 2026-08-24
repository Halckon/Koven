//! SPEC-0003 的诊断模型、目录不变量与完整排序契约测试。

mod support;

use std::ptr;

use lang_frontend::{
    diagnostic::{
        Diagnostic, DiagnosticCodeCatalog, DiagnosticCodeError, DiagnosticDetail, DiagnosticError,
        Severity, SpanRole, TextField, codes, ordered_diagnostics,
    },
    source::{SourceError, SourceId, SourceMap, Span},
};
use support::{catalog, code};

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources
        .add_source(name, text)
        .expect("test source names must be unique")
}

fn span(sources: &SourceMap, source_id: SourceId, start: usize, end: usize) -> Span {
    sources
        .span(source_id, start, end)
        .expect("test spans must be valid")
}

fn diagnostic(
    sources: &SourceMap,
    severity: Severity,
    code: lang_frontend::diagnostic::DiagnosticCode,
    message: &str,
    primary_span: Span,
) -> Diagnostic {
    Diagnostic::new(sources, severity, code, message, primary_span)
        .expect("test diagnostics must be valid")
}

fn assert_left_orders_first(sources: &SourceMap, left: Diagnostic, right: Diagnostic) {
    let diagnostics = [right, left];
    let ordered = ordered_diagnostics(sources, &diagnostics)
        .expect("both test diagnostics must resolve in the source map");

    assert!(
        ptr::eq(ordered[0], &diagnostics[1]),
        "the complete deterministic key must order the left diagnostic first"
    );
}

#[test]
fn catalog_accepts_only_exact_ascii_ldddd_codes() {
    let valid = DiagnosticCodeCatalog::try_new(&["L9999", "L0000", "L0042"])
        .expect("all codes use the required format");

    assert_eq!(valid.len(), 3);
    assert!(!valid.is_empty());
    assert_eq!(
        valid.resolve("L0000").map(|code| code.to_string()),
        Ok("L0000".to_owned())
    );
    assert_eq!(
        valid.resolve("L0042").map(|code| code.to_string()),
        Ok("L0042".to_owned())
    );

    for invalid in [
        "",
        "L000",
        "L00000",
        "l0001",
        "E0001",
        "L00A1",
        "L٠٠٠١",
        " L001",
        "L001 ",
    ] {
        assert_eq!(
            DiagnosticCodeCatalog::try_new(&[invalid]),
            Err(DiagnosticCodeError::InvalidFormat {
                code: invalid.to_owned(),
            }),
            "unexpected result for {invalid:?}"
        );
    }
}

#[test]
fn catalog_rejects_duplicates_and_unknown_lookups() {
    assert_eq!(
        DiagnosticCodeCatalog::try_new(&["L9000", "L9000"]),
        Err(DiagnosticCodeError::DuplicateCode {
            code: catalog(&["L9000"])
                .resolve("L9000")
                .expect("the helper code is registered"),
        })
    );

    let catalog = catalog(&["L9000"]);
    assert_eq!(
        catalog.resolve("L9001"),
        Err(DiagnosticCodeError::UnknownCode {
            code: "L9001".to_owned(),
        })
    );
}

#[test]
fn production_catalog_contains_exactly_the_published_frontend_codes() {
    let expected = [
        "L0001", "L0002", "L0003", "L0004", "L0005", "L0006", "L0007", "L0008", "L0009", "L0010",
        "L0011", "L0012", "L0013", "L0014", "L0015", "L0016", "L0017", "L0018", "L0019", "L0020",
        "L0021", "L0022", "L0023", "L0024", "L0025", "L0026", "L0027", "L0028", "L0029", "L0030",
        "L0031", "L0032", "L0033", "L0034", "L0035", "L0036", "L0037", "L0038", "L0039", "L0040",
        "L0041", "L0042", "L0043", "L0044", "L0045", "L0046", "L0047", "L0048", "L0049", "L0050",
        "L0051", "L0052", "L0053", "L0054", "L0055", "L0056", "L0057", "L0058", "L0059", "L0060",
        "L0061", "L0062", "L0063", "L0064", "L0065", "L0066", "L0067", "L0068", "L0069", "L0070",
        "L0071", "L0072", "L0073", "L0074", "L0075", "L0076", "L0077", "L0078", "L0079", "L0080",
        "L0081", "L0082", "L0083", "L0084", "L0085", "L0086", "L0087", "L0088", "L0089", "L0090",
        "L0091", "L0092", "L0093", "L0094", "L0095", "L0096", "L0097", "L0098", "L0099", "L0100",
        "L0101", "L0102", "L0103", "L0104", "L0105", "L0106", "L0107", "L0108", "L0109", "L0110",
        "L0111", "L0112", "L0113", "L0114", "L0115", "L0116", "L0117", "L0118", "L0119", "L0120",
        "L0121", "L0122", "L0123", "L0124", "L0125", "L0126", "L0127", "L0128", "L0129", "L0130",
        "L0131", "L0132", "L0133", "L0134", "L0135", "L0136", "L0137", "L0138", "L0139", "L0140",
        "L0141",
    ];
    let catalog = codes::catalog().expect("the checked-in production catalog must be valid");

    assert_eq!(codes::ALL, expected);
    assert_eq!(catalog.len(), expected.len());
    for raw_code in expected {
        assert_eq!(
            catalog.resolve(raw_code).map(|code| code.to_string()),
            Ok(raw_code.to_owned()),
        );
    }
}

#[test]
fn complete_diagnostic_preserves_detail_order_and_getters() {
    let catalog = catalog(&["L9000"]);
    let code = code(&catalog, "L9000").expect("the code is registered");
    let mut sources = SourceMap::new();
    let primary_id = add_source(&mut sources, "primary.ko", "primary");
    let related_id = add_source(&mut sources, "related.ko", "related");
    let primary = span(&sources, primary_id, 0, 7);
    let related = span(&sources, related_id, 1, 4);
    let mut diagnostic = diagnostic(
        &sources,
        Severity::Warning,
        code,
        "primary message",
        primary,
    );

    diagnostic
        .add_note("first note")
        .expect("the note is non-empty and single-line");
    diagnostic
        .add_label(&sources, related, "related message")
        .expect("the related span and message are valid");
    diagnostic
        .add_help("last help")
        .expect("the help is non-empty and single-line");

    assert_eq!(diagnostic.severity(), Severity::Warning);
    assert_eq!(diagnostic.code(), code);
    assert_eq!(diagnostic.message(), "primary message");
    assert_eq!(diagnostic.primary_span(), primary);
    assert!(matches!(diagnostic.details()[0], DiagnosticDetail::Note(_)));
    assert!(matches!(
        diagnostic.details()[1],
        DiagnosticDetail::Label(_)
    ));
    assert!(matches!(diagnostic.details()[2], DiagnosticDetail::Help(_)));

    match &diagnostic.details()[1] {
        DiagnosticDetail::Label(label) => {
            assert_eq!(label.span(), related);
            assert_eq!(label.message(), "related message");
        }
        DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => {
            panic!("the second detail must remain the inserted label")
        }
    }
}

#[test]
fn empty_required_text_is_rejected_for_every_field() {
    let catalog = catalog(&["L9000"]);
    let code = code(&catalog, "L9000").expect("the code is registered");
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "source.ko", "text");
    let primary = span(&sources, source_id, 0, 1);

    assert_eq!(
        Diagnostic::new(&sources, Severity::Error, code, "", primary),
        Err(DiagnosticError::EmptyText {
            field: TextField::PrimaryMessage,
        })
    );

    let mut diagnostic = diagnostic(&sources, Severity::Error, code, "message", primary);
    assert_eq!(
        diagnostic.add_label(&sources, primary, ""),
        Err(DiagnosticError::EmptyText {
            field: TextField::LabelMessage,
        })
    );
    assert_eq!(
        diagnostic.add_note(""),
        Err(DiagnosticError::EmptyText {
            field: TextField::Note,
        })
    );
    assert_eq!(
        diagnostic.add_help(""),
        Err(DiagnosticError::EmptyText {
            field: TextField::Help,
        })
    );
    assert!(diagnostic.details().is_empty());
}

#[test]
fn multiline_required_text_is_rejected_for_every_field() {
    let catalog = catalog(&["L9000"]);
    let code = code(&catalog, "L9000").expect("the code is registered");
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "source.ko", "text");
    let primary = span(&sources, source_id, 0, 1);

    assert_eq!(
        Diagnostic::new(&sources, Severity::Error, code, "line\nbreak", primary,),
        Err(DiagnosticError::MultilineText {
            field: TextField::PrimaryMessage,
        })
    );

    let mut diagnostic = diagnostic(&sources, Severity::Error, code, "message", primary);
    assert_eq!(
        diagnostic.add_label(&sources, primary, "line\rbreak"),
        Err(DiagnosticError::MultilineText {
            field: TextField::LabelMessage,
        })
    );
    assert_eq!(
        diagnostic.add_note("line\nbreak"),
        Err(DiagnosticError::MultilineText {
            field: TextField::Note,
        })
    );
    assert_eq!(
        diagnostic.add_help("line\rbreak"),
        Err(DiagnosticError::MultilineText {
            field: TextField::Help,
        })
    );
    assert!(diagnostic.details().is_empty());
}

#[test]
fn cross_map_primary_and_label_spans_are_rejected() {
    let catalog = catalog(&["L9000"]);
    let code = code(&catalog, "L9000").expect("the code is registered");
    let mut first = SourceMap::new();
    let first_id = add_source(&mut first, "first.ko", "first");
    let first_span = span(&first, first_id, 0, 1);
    let mut second = SourceMap::new();
    let second_id = add_source(&mut second, "second.ko", "second");
    let second_span = span(&second, second_id, 0, 1);

    assert_eq!(
        Diagnostic::new(
            &second,
            Severity::Error,
            code,
            "invalid primary",
            first_span,
        ),
        Err(DiagnosticError::InvalidSpan {
            role: SpanRole::Primary,
            source: SourceError::InvalidSourceId {
                source_id: first_id,
            },
        })
    );

    let mut diagnostic = diagnostic(&first, Severity::Error, code, "valid primary", first_span);
    assert_eq!(
        diagnostic.add_label(&first, second_span, "invalid label"),
        Err(DiagnosticError::InvalidSpan {
            role: SpanRole::Label { detail_index: 0 },
            source: SourceError::InvalidSourceId {
                source_id: second_id,
            },
        })
    );
    assert!(diagnostic.details().is_empty());
}

#[test]
fn total_order_uses_every_primary_and_detail_field() {
    let catalog = catalog(&["L9000", "L9001"]);
    let code_0 = code(&catalog, "L9000").expect("the code is registered");
    let code_1 = code(&catalog, "L9001").expect("the code is registered");
    let mut sources = SourceMap::new();
    let a_id = add_source(&mut sources, "a.ko", "abcdef");
    let b_id = add_source(&mut sources, "b.ko", "abcdef");
    let a_0_1 = span(&sources, a_id, 0, 1);
    let a_0_2 = span(&sources, a_id, 0, 2);
    let a_1_2 = span(&sources, a_id, 1, 2);
    let b_0_1 = span(&sources, b_id, 0, 1);

    let base = || diagnostic(&sources, Severity::Error, code_0, "message", a_0_1);

    assert_left_orders_first(
        &sources,
        base(),
        diagnostic(&sources, Severity::Error, code_0, "message", b_0_1),
    );
    assert_left_orders_first(
        &sources,
        base(),
        diagnostic(&sources, Severity::Error, code_0, "message", a_1_2),
    );
    assert_left_orders_first(
        &sources,
        base(),
        diagnostic(&sources, Severity::Error, code_0, "message", a_0_2),
    );
    assert_left_orders_first(
        &sources,
        base(),
        diagnostic(&sources, Severity::Warning, code_0, "message", a_0_1),
    );
    assert_left_orders_first(
        &sources,
        base(),
        diagnostic(&sources, Severity::Error, code_1, "message", a_0_1),
    );
    assert_left_orders_first(
        &sources,
        diagnostic(&sources, Severity::Error, code_0, "a message", a_0_1),
        diagnostic(&sources, Severity::Error, code_0, "b message", a_0_1),
    );

    let mut label_source_left = base();
    label_source_left
        .add_label(&sources, a_0_1, "label")
        .expect("the label is valid");
    let mut label_source_right = base();
    label_source_right
        .add_label(&sources, b_0_1, "label")
        .expect("the label is valid");
    assert_left_orders_first(&sources, label_source_left, label_source_right);

    let mut label_start_left = base();
    label_start_left
        .add_label(&sources, a_0_2, "label")
        .expect("the label is valid");
    let mut label_start_right = base();
    label_start_right
        .add_label(&sources, a_1_2, "label")
        .expect("the label is valid");
    assert_left_orders_first(&sources, label_start_left, label_start_right);

    let mut label_end_left = base();
    label_end_left
        .add_label(&sources, a_0_1, "label")
        .expect("the label is valid");
    let mut label_end_right = base();
    label_end_right
        .add_label(&sources, a_0_2, "label")
        .expect("the label is valid");
    assert_left_orders_first(&sources, label_end_left, label_end_right);

    let mut label_text_left = base();
    label_text_left
        .add_label(&sources, a_0_1, "a label")
        .expect("the label is valid");
    let mut label_text_right = base();
    label_text_right
        .add_label(&sources, a_0_1, "b label")
        .expect("the label is valid");
    assert_left_orders_first(&sources, label_text_left, label_text_right);

    let mut detail_variant_left = base();
    detail_variant_left
        .add_label(&sources, a_0_1, "label")
        .expect("the label is valid");
    let mut detail_variant_right = base();
    detail_variant_right
        .add_note("note")
        .expect("the note is valid");
    assert_left_orders_first(&sources, detail_variant_left, detail_variant_right);

    let mut note_text_left = base();
    note_text_left
        .add_note("a note")
        .expect("the note is valid");
    let mut note_text_right = base();
    note_text_right
        .add_note("b note")
        .expect("the note is valid");
    assert_left_orders_first(&sources, note_text_left, note_text_right);

    let mut help_text_left = base();
    help_text_left
        .add_help("a help")
        .expect("the help is valid");
    let mut help_text_right = base();
    help_text_right
        .add_help("b help")
        .expect("the help is valid");
    assert_left_orders_first(&sources, help_text_left, help_text_right);

    let mut note_prefix = base();
    note_prefix.add_note("same").expect("the note is valid");
    let mut note_longer = note_prefix.clone();
    note_longer
        .add_note("later")
        .expect("the later note is valid");
    assert_left_orders_first(&sources, note_prefix, note_longer);

    let mut help_prefix = base();
    help_prefix.add_help("same").expect("the help is valid");
    let mut help_longer = help_prefix.clone();
    help_longer
        .add_help("later")
        .expect("the later help is valid");
    assert_left_orders_first(&sources, help_prefix, help_longer);

    let mut label_prefix = base();
    label_prefix
        .add_label(&sources, a_0_1, "same")
        .expect("the label is valid");
    let mut label_longer = label_prefix.clone();
    label_longer
        .add_label(&sources, a_0_1, "later")
        .expect("the label is valid");
    assert_left_orders_first(&sources, label_prefix, label_longer);
}

#[test]
fn ordering_is_independent_of_diagnostic_and_source_load_order() {
    let catalog = catalog(&["L9000"]);
    let code = code(&catalog, "L9000").expect("the code is registered");

    let mut first = SourceMap::new();
    let first_b = add_source(&mut first, "b.ko", "b");
    let first_a = add_source(&mut first, "a.ko", "a");
    let first_diagnostics = [
        diagnostic(
            &first,
            Severity::Error,
            code,
            "from b",
            span(&first, first_b, 0, 1),
        ),
        diagnostic(
            &first,
            Severity::Error,
            code,
            "from a",
            span(&first, first_a, 0, 1),
        ),
    ];

    let mut second = SourceMap::new();
    let second_a = add_source(&mut second, "a.ko", "a");
    let second_b = add_source(&mut second, "b.ko", "b");
    let second_diagnostics = [
        diagnostic(
            &second,
            Severity::Error,
            code,
            "from a",
            span(&second, second_a, 0, 1),
        ),
        diagnostic(
            &second,
            Severity::Error,
            code,
            "from b",
            span(&second, second_b, 0, 1),
        ),
    ];

    let first_messages: Vec<_> = ordered_diagnostics(&first, &first_diagnostics)
        .expect("all first-map diagnostics are valid")
        .into_iter()
        .map(Diagnostic::message)
        .collect();
    let second_messages: Vec<_> = ordered_diagnostics(&second, &second_diagnostics)
        .expect("all second-map diagnostics are valid")
        .into_iter()
        .map(Diagnostic::message)
        .collect();

    assert_eq!(first_messages, ["from a", "from b"]);
    assert_eq!(first_messages, second_messages);
}
