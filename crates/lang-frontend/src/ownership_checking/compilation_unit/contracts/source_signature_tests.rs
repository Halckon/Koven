//! Argument and receiver contracts resolve the same source callable, not its spelling.
use super::{OwnershipCheckingError, source_callable_signature, source_parameter_span};
use crate::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    source::SourceMap,
    type_checking::{UnitCallTarget, check_compilation_unit_types, standard_environments},
};

#[test]
fn source_parameters_keep_declaration_member_and_external_boundaries() {
    const SOURCE: &str = "fun take(value: Int): Int = value\n\
        class Worker { fun take(member: Int): Int = member }\n\
        fun run(worker: Worker): Int { println(1); return take(worker.take(2)) }";
    for memory in [false, true] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("signature.ko", SOURCE).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let inputs = [if memory {
            SourceUnitInput::memory_document(source, &parsed)
        } else {
            SourceUnitInput::new("root", "signature.ko", source, &parsed)
        }];
        let (ne, te) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te)
            .unwrap()
            .validate()
            .unwrap();
        let mut observed = Vec::new();
        for call in typed.types().calls() {
            let expression = call.expression();
            let call_span = parsed
                .ast()
                .expressions()
                .get(expression.expression())
                .unwrap()
                .span();
            let text = sources.slice(call_span).unwrap();
            let expected = match text {
                "take(worker.take(2))" => {
                    assert!(matches!(call.target(), UnitCallTarget::Declaration(_)));
                    Some("value: Int")
                }
                "worker.take(2)" => {
                    assert!(matches!(call.target(), UnitCallTarget::Symbol(_)));
                    Some("member: Int")
                }
                "println(1)" => {
                    assert!(matches!(call.target(), UnitCallTarget::External(_)));
                    None
                }
                other => panic!("unexpected call: {other}"),
            };
            observed.push(text);
            let span = source_parameter_span(typed.types(), call.target(), 0, expression).unwrap();
            assert_eq!(span.map(|span| sources.slice(span).unwrap()), expected);
            if expected.is_some() {
                assert!(source_callable_signature(typed.types(), call.target()).is_some());
                assert!(matches!(
                    source_parameter_span(typed.types(), call.target(), usize::MAX, expression),
                    Err(OwnershipCheckingError::InvalidUnitCallParameter {
                        source_unit, expression: invalid_expression, parameter,
                    }) if source_unit == expression.source_unit().index()
                        && invalid_expression == expression.expression().index()
                        && parameter == usize::MAX
                ));
            } else {
                assert!(source_callable_signature(typed.types(), call.target()).is_none());
                assert_eq!(
                    source_parameter_span(typed.types(), call.target(), usize::MAX, expression)
                        .unwrap(),
                    None
                );
            }
        }
        observed.sort();
        assert_eq!(
            observed,
            ["println(1)", "take(worker.take(2))", "worker.take(2)"]
        );
    }
}
