//! 双 Unit 入口的名义类型身份与 arity 诊断锚点。
use super::{codes, diagnostic_spans, unit_entries::checked_entries};
use lang_frontend::type_checking::{BuiltinType, UnitTypeKind, UnitTypeRefId};

#[test]
fn nominal_signature_arity_keeps_original_type_name_spans() {
    let text = "class Box<T>(val item: T) {}\n\
                class Left {}\n\
                class Right {}\n\
                fun inspect(left: Box<Int>, again: Box<Int>, missing: Box, many: Left<Int>): Unit";
    let (sources, file, entries) = checked_entries(text);
    for (source, typed) in entries {
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
        let nominals = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|declaration| declaration.nominal())
            .collect::<Vec<_>>();
        let [boxed, left, right] = nominals.as_slice() else {
            panic!("Box, Left and Right signatures");
        };
        assert_ne!(left.declaration(), right.declaration());

        let box_types = file
            .ast()
            .type_refs()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()) == Ok("Box<Int>"))
            .map(|(id, _)| {
                typed
                    .type_ref_type(UnitTypeRefId::new(source, id))
                    .expect("typed Box<Int>")
            })
            .collect::<Vec<_>>();
        let [first, second] = box_types.as_slice() else {
            panic!("two Box<Int> uses");
        };
        assert_eq!(first, second);
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = typed.types().get(*first)
        else {
            panic!("Box<Int> instance");
        };
        assert_eq!(*declaration, boxed.declaration());
        let [argument] = arguments.as_slice() else {
            panic!("one Box type argument");
        };
        assert_eq!(
            typed.types().get(*argument),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );

        let [parameter] = boxed.type_parameters() else {
            panic!("one Box type parameter");
        };
        assert_eq!(parameter.source_unit(), source);
        let [field] = boxed.fields() else {
            panic!("one Box field");
        };
        assert_eq!(typed.symbol_type(*parameter), Some(field.ty()));
        assert_eq!(
            typed.types().get(field.ty()),
            Some(&UnitTypeKind::TypeParameter(*parameter))
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
