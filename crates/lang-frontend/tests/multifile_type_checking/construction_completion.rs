//! Construction completion follows operands without erasing static result identity.
use super::*;

#[test]
fn operands_preserve_construction_type_and_completion() {
    for (template, value) in [
        ("Holder(ARG)", "7"),
        ("Holder(((ARG)))", "7"),
        ("Box<Point>(Point(ARG))", "7"),
        ("Point(ARG)", "7"),
        ("Choice.Item(ARG)", "7"),
        ("Box<Point>(ARG)", "Point(7)"),
        ("Rc<Int>(ARG)", "7"),
        ("Pair(tail = 8, value = ARG)", "7"),
    ] {
        for (operand, completes) in [
            (value.to_owned(), true),
            ("return 9".to_owned(), false),
            ("stop()".to_owned(), false),
            ("if (flag) { return 9 } else { return 8 }".to_owned(), false),
            (format!("if (flag) {{ return 9 }} else {{ {value} }}"), true),
        ] {
            let expression = template.replace("ARG", &operand);
            let text = format!(
                "class Holder(val value: Int)\nvalue class Point(val value: Int)\nenum class Choice {{ Item(value: Int) }}\nclass Pair(val value: Int, val tail: Int)\nfun stop(): Nothing = stop()\nfun use(flag: Boolean): Int {{ val item = {expression}; return 0 }}"
            );
            let mut sources = SourceMap::new();
            let (source, file) = parsed(&mut sources, "completion.ko", &text);
            for input in [
                SourceUnitInput::memory_document(source, &file),
                SourceUnitInput::new("root", "completion.ko", source, &file),
            ] {
                let inputs = [input];
                let (environment, types) = standard_environments();
                let names = validated_names(&sources, &inputs, &environment);
                let typed =
                    check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
                assert!(
                    typed.diagnostics().is_empty(),
                    "{expression}: {:?}",
                    typed.diagnostics()
                );
                let local = file
                    .ast()
                    .expressions()
                    .iter()
                    .find(|(_, node)| sources.slice(node.span()) == Ok(expression.as_str()))
                    .unwrap()
                    .0;
                let id = UnitExpressionId::new(source_unit(&names, source), local);
                let descriptor = typed
                    .constructions()
                    .iter()
                    .find(|item| item.expression() == id)
                    .expect("typed construction");
                assert_eq!(typed.expression_type(id), Some(descriptor.result_type()));
                assert!(matches!(
                    typed.types().get(descriptor.result_type()),
                    Some(UnitTypeKind::Nominal { .. } | UnitTypeKind::Intrinsic { .. })
                ));
                assert_eq!(
                    typed.expression_falls_through(id),
                    Some(completes),
                    "{expression}"
                );
                assert_eq!(typed.clone().expression_falls_through(id), Some(completes));
            }
        }
    }
}

#[test]
fn terminated_construction_still_diagnoses_invalid_suffix() {
    for first in ["7", "return 9", "stop()"] {
        let text = format!(
            "class Pair(val value: Int, val tail: Int)\nfun stop(): Nothing = stop()\nfun use(): Int {{ val item = Pair({first}, \"bad\"); return 0 }}"
        );
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "suffix.ko", &text);
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "suffix.ko", source, &file),
        ] {
            let inputs = [input];
            let (environment, types) = standard_environments();
            let names = validated_names(&sources, &inputs, &environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert_eq!(typed.diagnostics().len(), 1, "{:?}", typed.diagnostics());
            let diagnostic = &typed.diagnostics()[0];
            assert_eq!(diagnostic.code().to_string(), "L0084");
            assert_eq!(sources.slice(diagnostic.primary_span()), Ok("\"bad\""));
        }
    }
}

#[test]
fn zero_argument_construction_and_ordinary_calls_keep_their_flow() {
    for (expression, completes) in [
        ("Empty()", true),
        ("Flag.One", true),
        ("make(7)", true),
        ("make(return 9)", false),
    ] {
        let text = format!(
            "class Empty()\nenum class Flag {{ One }}\nfun make(value: Int): Int = value\nfun use(): Int {{ val item = {expression}; return 0 }}"
        );
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "controls.ko", &text);
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "controls.ko", source, &file),
        ] {
            let inputs = [input];
            let (environment, types) = standard_environments();
            let names = validated_names(&sources, &inputs, &environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let local = file
                .ast()
                .expressions()
                .iter()
                .find(|(_, node)| sources.slice(node.span()) == Ok(expression))
                .unwrap()
                .0;
            let id = UnitExpressionId::new(source_unit(&names, source), local);
            assert_eq!(
                typed.expression_falls_through(id),
                Some(completes),
                "{expression}"
            );
            assert!(typed.expression_type(id).is_some());
        }
    }
}
