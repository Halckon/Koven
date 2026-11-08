//! 保留 Single 原见证，在双 Unit 入口核同一源码的 arity 诊断锚点。
use super::{codes, diagnostic_spans, unit_entries::checked_entries};

#[test]
fn nominal_signature_arity_keeps_original_type_name_spans() {
    let text = "class Box<T>(val item: T) {}\n\
                class Left {}\n\
                class Right {}\n\
                fun inspect(left: Box<Int>, again: Box<Int>, missing: Box, many: Left<Int>): Unit";
    let (sources, file, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0091", "L0091"]);
        assert_eq!(
            diagnostic_spans(&sources, &typed.diagnostics()[0]),
            ("Box", "Box")
        );
        assert_eq!(
            diagnostic_spans(&sources, &typed.diagnostics()[1]),
            ("Left", "Left")
        );
        let span = typed.diagnostics()[1].primary_span();
        let start = text.find("Left<Int>").expect("use-site nominal");
        assert_eq!(
            (span.source_id(), span.start(), span.end()),
            (file.source_id(), start, start + 4)
        );
        assert!(typed.validate().is_err());
    }
}

#[test]
fn signature_parameter_and_builtin_arity_keep_argument_anchors() {
    let text = "fun <T> inspect(parameter: T<Int>, builtin: Int<String>): Unit";
    let (sources, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0091", "L0082"]);
        assert_eq!(
            diagnostic_spans(&sources, &typed.diagnostics()[0]),
            ("Int", "T")
        );
        assert_eq!(
            diagnostic_spans(&sources, &typed.diagnostics()[1]),
            ("String", "Int")
        );
        assert!(typed.validate().is_err());
    }
}
