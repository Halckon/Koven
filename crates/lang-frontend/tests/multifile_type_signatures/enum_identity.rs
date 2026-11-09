//! Explicit visibility must not change enum declaration or case identity.
use super::*;
use lang_frontend::type_checking::{BuiltinType, check_compilation_unit_types};

#[test]
fn explicit_public_enum_preserves_case_signatures_across_files() {
    let mut sources = SourceMap::new();
    let (model_id, model) = parsed(
        &mut sources,
        "model.ko",
        "package model\npublic enum class Shape { Circle(radius: Int), Point }",
    );
    let (use_id, usage) = parsed(
        &mut sources,
        "use.ko",
        "package app\nimport model.Shape\nfun inspect(value: Shape): Boolean = value is Shape.Circle",
    );
    let inputs = [
        SourceUnitInput::new("root", "model/model.ko", model_id, &model),
        SourceUnitInput::new("root", "app/use.ko", use_id, &usage),
    ];
    let (name_environment, type_environment) = standard_environments();
    let mut permutations = Vec::new();
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let validated = names(&sources, &inputs, &name_environment)
            .validate()
            .expect("valid names");
        let signatures =
            collect_compilation_unit_signatures(&sources, &inputs, &validated, &type_environment)
                .expect("explicit public enum signatures");
        assert!(
            signatures.diagnostics().is_empty(),
            "{:?}",
            signatures.diagnostics()
        );
        let nominal = signatures
            .declarations()
            .iter()
            .find_map(|declaration| declaration.nominal())
            .expect("enum signature");
        assert_eq!(
            nominal
                .enum_cases()
                .iter()
                .map(|case| case.name())
                .collect::<Vec<_>>(),
            ["Circle", "Point"]
        );
        for case in nominal.enum_cases() {
            assert_eq!(case.name_span().source_id(), model_id);
            assert_eq!(
                signatures.symbol_type(case.type_symbol()),
                Some(case.case_type())
            );
            assert_eq!(
                signatures.symbol_type(case.value_symbol()),
                Some(case.value_type())
            );
        }
        let [payload] = nominal.enum_cases()[0].payloads() else {
            panic!("one Circle payload")
        };
        assert_eq!(payload.name(), "radius");
        assert_eq!(
            signatures.types().get(payload.ty()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert!(nominal.enum_cases()[1].payloads().is_empty());
        let typed = check_compilation_unit_types(&sources, &inputs, &validated, &type_environment)
            .expect("public enum body checks internally");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(typed.validate().is_ok());
        permutations.push(signatures);
    }
    assert_eq!(permutations[0], permutations[1]);
    for signatures in permutations {
        assert!(signatures.validate().is_ok());
    }
}

#[test]
fn explicit_visibility_preserves_finite_inline_components_in_both_entries() {
    for visibility in ["public", "internal", "private"] {
        let mut sources = SourceMap::new();
        let text = format!(
            "{visibility} enum class Shape {{ Circle(radius: Int), Point }}\n{visibility} value class Holder(val shape: Shape)"
        );
        let (source, file) = parsed(&mut sources, "model.ko", &text);
        let (name_environment, type_environment) = standard_environments();
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "model.ko", source, &file),
        ] {
            let inputs = [input];
            let validated = names(&sources, &inputs, &name_environment)
                .validate()
                .expect("valid names");
            let signatures = collect_compilation_unit_signatures(
                &sources,
                &inputs,
                &validated,
                &type_environment,
            )
            .expect("modified finite classifier");
            assert!(
                signatures.diagnostics().is_empty(),
                "{text}: {:?}",
                signatures.diagnostics()
            );
            assert!(signatures.validate().is_ok());
        }
    }
}

#[test]
fn explicit_visibility_does_not_skip_recursive_inline_layout_diagnostics() {
    for visibility in ["public", "internal", "private"] {
        for (declaration, primary) in [
            (
                "enum class Recursive { Next(payload: Recursive) }",
                "Recursive",
            ),
            ("value class Direct(val next: Direct)", "Direct"),
        ] {
            let mut sources = SourceMap::new();
            let text = format!("{visibility} {declaration}");
            let (source, file) = parsed(&mut sources, "model.ko", &text);
            let (name_environment, type_environment) = standard_environments();
            for input in [
                SourceUnitInput::memory_document(source, &file),
                SourceUnitInput::new("root", "model.ko", source, &file),
            ] {
                let inputs = [input];
                let validated = names(&sources, &inputs, &name_environment)
                    .validate()
                    .expect("valid names");
                let signatures = collect_compilation_unit_signatures(
                    &sources,
                    &inputs,
                    &validated,
                    &type_environment,
                )
                .expect("modified recursive classifier remains a diagnostic product");
                assert_eq!(codes(signatures.diagnostics()), ["L0116"], "{text}");
                let span = signatures.diagnostics()[0].primary_span();
                assert_eq!(sources.slice(span), Ok(primary));
                assert_eq!(
                    span.start(),
                    text.rfind(primary).expect("recursive type use")
                );
                assert_eq!(span.source_id(), source);
                assert!(signatures.validate().is_err());
            }
        }
    }
}
