//! Ordinary Unit diagnostics extracted from the mixed legacy identity witness.
use super::{SourceKey, source_key};
use crate::ssa::unit_lower_test_support::parsed;
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn original_recovery_diagnostics_stop_at_unit_capability_gates() {
    for (body, code) in [("missing", "L0080"), ("17", "L0084")] {
        let mut sources = SourceMap::new();
        let text = format!("package pair\nfun entry(): String = {body}");
        let (source, file) = parsed(&mut sources, "pair/entry.ko", &text);
        let (name_environment, environment) = standard_environments();
        let expected_span = SourceKey(
            "pair/entry.ko".to_owned(),
            text.len() - body.len(),
            text.len(),
        );
        // Retain the original unselected same-text source without mixing products.
        let (foreign_source, _foreign_file) = parsed(&mut sources, "foreign.ko", &text);
        assert_ne!(source, foreign_source);
        let inputs = [SourceUnitInput::new("root", "pair/entry.ko", source, &file)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment).unwrap();
        if body == "missing" {
            assert_eq!(names.diagnostics()[0].code().to_string(), code);
            assert_eq!(
                source_key(&sources, names.diagnostics()[0].primary_span()),
                expected_span
            );
            assert!(
                names.validate().is_err(),
                "unit cannot create names capability"
            );
        } else {
            let names = names.validate().unwrap();
            let typed =
                check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
            assert_eq!(typed.diagnostics()[0].code().to_string(), code);
            assert_eq!(
                source_key(&sources, typed.diagnostics()[0].primary_span()),
                expected_span
            );
            assert!(
                typed.clone().validate().is_err(),
                "basic cannot create typed capability"
            );
            assert!(
                typed.validate_constants().is_err(),
                "const cannot create typed capability"
            );
        }
    }
}
