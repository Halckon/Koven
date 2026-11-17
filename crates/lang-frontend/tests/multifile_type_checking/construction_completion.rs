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

#[test]
fn non_completing_construction_preserves_identity_without_delivering_a_result() {
    for (expression, completes) in [
        ("Holder(return 9)", false),
        ("Holder(7)", true),
        ("Point(return 9)", false),
        ("Point(7)", true),
        ("Choice.Item(return 9)", false),
        ("Choice.Item(7)", true),
        ("Box<Point>(return 9)", false),
        ("Box<Point>(Point(7))", true),
        ("Rc<Int>(return 9)", false),
        ("Rc<Int>(7)", true),
        ("make(return 9)", false),
        ("make(7)", true),
        ("Generic<Int>(return 9)", false),
        ("Holder(if (flag) { return 9 } else { return 8 })", false),
        ("Holder(if (flag) { return 9 } else { 7 })", true),
    ] {
        let text = format!(
            "class Holder(val value: Int)\nclass Generic<T>(val value: T)\nvalue class Point(val value: Int)\nenum class Choice {{ Item(value: Int) }}\nfun make(value: Int): Holder = Holder(value)\nfun use(flag: Boolean): Int {{ val item: Int = {expression}; return 0 }}"
        );
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "expectation.ko", &text);
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "expectation.ko", source, &file),
        ] {
            let inputs = [input];
            let (environment, types) = standard_environments();
            let names = validated_names(&sources, &inputs, &environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            let local = file
                .ast()
                .expressions()
                .iter()
                .find(|(_, node)| sources.slice(node.span()) == Ok(expression))
                .unwrap()
                .0;
            let id = UnitExpressionId::new(source_unit(&names, source), local);
            if completes {
                assert_eq!(
                    typed.diagnostics().len(),
                    1,
                    "{expression}: {:?}",
                    typed.diagnostics()
                );
                let diagnostic = &typed.diagnostics()[0];
                assert_eq!(diagnostic.code().to_string(), "L0084");
                assert_eq!(sources.slice(diagnostic.primary_span()), Ok(expression));
            } else {
                assert!(
                    typed.diagnostics().is_empty(),
                    "{expression}: {:?}",
                    typed.diagnostics()
                );
                assert_eq!(typed.expression_falls_through(id), Some(false));
                assert!(matches!(
                    typed
                        .expression_type(id)
                        .and_then(|ty| typed.types().get(ty)),
                    Some(UnitTypeKind::Nominal { .. } | UnitTypeKind::Intrinsic { .. })
                ));
                if !expression.starts_with("make(") {
                    assert_eq!(
                        typed
                            .constructions()
                            .iter()
                            .find(|d| d.expression() == id)
                            .unwrap()
                            .result_type(),
                        typed.expression_type(id).unwrap()
                    );
                }
            }
        }
    }
}

#[test]
fn non_completing_construction_keeps_return_value_and_suffix_diagnostics() {
    for (expression, marker) in [
        ("Holder(return \"bad\")", "\"bad\""),
        ("Pair(return 9, \"bad\")", "\"bad\""),
    ] {
        let text = format!(
            "class Holder(val value: Int)\nclass Pair(val value: Int, val tail: Int)\nfun use(): Int {{ val item: Int = {expression}; return 0 }}"
        );
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "diagnostics.ko", &text);
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "diagnostics.ko", source, &file),
        ] {
            let inputs = [input];
            let (environment, types) = standard_environments();
            let names = validated_names(&sources, &inputs, &environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert_eq!(typed.diagnostics().len(), 1, "{:?}", typed.diagnostics());
            assert_eq!(typed.diagnostics()[0].code().to_string(), "L0084");
            assert_eq!(
                sources.slice(typed.diagnostics()[0].primary_span()),
                Ok(marker)
            );
        }
    }
}
