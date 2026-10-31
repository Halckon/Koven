//! CI-only descriptor mutation guard; never execute this target locally.

use super::{OwnershipCheckingError, validate_construction};
use crate::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::{Expression, parse_file},
    source::SourceMap,
    type_checking::{
        UnitExpressionId, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

#[test]
fn missing_construction_argument_preserves_exact_internal_identity() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "invalid-construction-descriptor.ko",
            "class Holder(val item: Int)\nfun build(): Holder = Holder(1)",
        )
        .unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let expressions = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(expression, node)| {
            (matches!(node.payload(), Expression::Call { .. })
                && sources.slice(node.span()).unwrap() == "Holder(1)")
                .then_some(expression)
        })
        .collect::<Vec<_>>();
    let [expression] = expressions.as_slice() else {
        panic!("one source-derived Holder construction");
    };
    let expression = *expression;

    for input in [
        SourceUnitInput::new(
            "root",
            "invalid-construction-descriptor.ko",
            source,
            &parsed,
        ),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (name_environment, type_environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .unwrap()
            .validate()
            .unwrap();
        let source_unit = names
            .names()
            .index()
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == source)
            .unwrap()
            .id();
        let raw =
            check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
        assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &type_environment, &raw)
                .unwrap();
        assert!(complete.is_same_analysis(&raw));
        let typed = raw.validate().unwrap();
        let [descriptor] = typed.types().constructions() else {
            panic!("one real construction descriptor");
        };
        assert_eq!(
            descriptor.expression(),
            UnitExpressionId::new(source_unit, expression)
        );
        assert_eq!(descriptor.arguments().len(), 1);
        validate_construction(&parsed, source_unit, typed.types(), descriptor).unwrap();

        // Remove only the real argument mapping; all other descriptor fields and the
        // original validated product remain intact. This is not a malformed source.
        let mut malformed = descriptor.clone();
        malformed.arguments.clear();
        assert_eq!(malformed.expression(), descriptor.expression());
        assert_eq!(malformed.instance(), descriptor.instance());
        assert_eq!(malformed.result_type(), descriptor.result_type());
        assert!(malformed.arguments().is_empty());
        let error = validate_construction(&parsed, source_unit, typed.types(), &malformed)
            .expect_err("missing argument mapping must be an internal identity error");
        assert!(matches!(
            error,
            OwnershipCheckingError::InvalidUnitConstruction {
                source_unit: rejected_source,
                expression: rejected_expression,
            } if rejected_source == source_unit.index()
                && rejected_expression == expression.index()
        ));

        assert_eq!(descriptor.arguments().len(), 1);
        validate_construction(&parsed, source_unit, typed.types(), descriptor).unwrap();
    }
}
