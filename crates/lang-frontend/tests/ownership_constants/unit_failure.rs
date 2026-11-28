//! Original failed constant materialization capability through both Unit layouts.
use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
    name_resolution::{
        SourceUnitInput, SourceUnitKey, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::check_compilation_unit_constant_ownership,
    parser::{Expression, Item, NameMarker},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

pub(super) fn check(text: &str) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("constants.ko", text).unwrap();
    assert_eq!(sources.source_text(source), Ok(text));
    let sources = &sources;
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
    // Select arguments from the two unchanged consume calls, not from either
    // ownership diagnostic or its labels.
    let operands = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| sources.slice(node.span()) == Ok("take(input)"))
        .map(|(_, call)| {
            let Expression::Call { arguments, .. } = call.payload() else {
                panic!("original ownership delivery call");
            };
            let [argument] = arguments.as_slice() else {
                panic!("one original argument");
            };
            let span = parsed
                .ast()
                .expressions()
                .get(argument.value)
                .unwrap()
                .span();
            assert_eq!(sources.slice(span), Ok("input"));
            span
        })
        .collect::<Vec<_>>();
    let [first, repeated] = operands.as_slice() else {
        panic!("two separate moves from the same original name");
    };
    assert!(first.end() < repeated.start());
    let catalog = DiagnosticCodeCatalog::try_new(&["L0131"]).unwrap();
    let mut diagnostic = Diagnostic::new(
        sources,
        Severity::Error,
        catalog.resolve("L0131").unwrap(),
        "use of moved value",
        *repeated,
    )
    .unwrap();
    diagnostic
        .add_label(sources, *first, "value was moved here")
        .unwrap();
    // The Unit diagnostic retains that contract and adds the selected declaration.
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
        assert_eq!(
            owned.ownership().diagnostics(),
            std::slice::from_ref(&diagnostic)
        );
        assert!(!owned.ownership().diagnostics().is_empty());
        assert!(owned.materializations().is_none());
        assert!(owned.ownership().drops().is_empty());
        assert!(owned.ownership().loans().is_empty());
        assert!(owned.validate().is_err());
    }
}
