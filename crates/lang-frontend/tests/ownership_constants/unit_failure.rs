//! Original failed constant materialization capability through both Unit layouts.
use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::{
        SourceUnitInput, SourceUnitKey, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::check_compilation_unit_constant_ownership,
    parser::{Item, NameMarker},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

pub(super) fn check(sources: &SourceMap, expected: &[Diagnostic]) {
    let [original] = expected else {
        panic!("one original failure")
    };
    let source = original.primary_span().source_id();
    let parsed =
        super::parser_test_assertions::parse_file_twice(sources, source, "constant failure Unit");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let parameters: Vec<_> = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Function {
                name: NameMarker::Present(name),
                parameters,
                ..
            } if sources.slice(*name).unwrap() == "take" => Some(parameters),
            _ => None,
        })
        .collect();
    let [parameters] = parameters.as_slice() else {
        panic!("one actual take declaration")
    };
    let [parameter] = parameters.as_slice() else {
        panic!("one actual take parameter")
    };
    assert_eq!(sources.slice(parameter.span).unwrap(), "own text: String");
    let mut unit_expected = expected.to_vec();
    let [diagnostic] = unit_expected.as_mut_slice() else {
        panic!("one original failure")
    };
    // Unit retains the complete Single diagnostic and adds this exact declaration label.
    diagnostic
        .add_label(sources, parameter.span, "selected parameter declared here")
        .unwrap();
    let (environment, types) = standard_environments();
    for rooted in [false, true] {
        let input = if rooted {
            SourceUnitInput::new("root", "constants.ko", source, &parsed)
        } else {
            SourceUnitInput::memory_document(source, &parsed)
        };
        let inputs = [input];
        let index = index_compilation_unit(sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let [entry] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(entry.source_id(), source);
        match (rooted, entry.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "constants.ko");
            }
            _ => panic!("requested entry identity"),
        }
        let names = resolve_compilation_unit_names(sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let typed = typed.validate_constants().unwrap();
        let owned =
            check_compilation_unit_constant_ownership(sources, &inputs, &names, &types, &typed)
                .unwrap();
        assert!(owned.is_compatible_with(&typed));
        assert_eq!(owned.ownership().diagnostics(), unit_expected);
        assert!(!owned.ownership().diagnostics().is_empty());
        assert!(owned.materializations().is_none());
        assert!(owned.ownership().drops().is_empty());
        assert!(owned.ownership().loans().is_empty());
        assert!(owned.validate().is_err());
    }
}
