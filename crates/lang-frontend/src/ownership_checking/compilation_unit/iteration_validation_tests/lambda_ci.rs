//! Stage A: actual callable-boundary Return identities; preserve both Single witnesses.
use super::super::*;

#[test]
fn original_lambda_loop_returns_keep_their_actual_callable_boundary() {
    use crate::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::{Expression, parse_file},
        source::SourceMap,
        type_checking::{
            UnitExpressionId, check_compilation_unit_types, complete_compilation_unit_types,
            standard_environments,
        },
    };
    for (text, operand) in [
        (
            "fun source(): Array<Int> = arrayOf(1, 2)\nfun scan(): Int { for (item in source()) { val callback: () -> Int = { return 7 }; if (item == 1) { return callback() } }; return 0 }",
            "callback()",
        ),
        (
            "fun source(): Array<Int> = arrayOf(1, 2)\nfun scan(): Int { val callback: () -> Int = { for (item in source()) { return item }; return 0 }; return callback() }",
            "item",
        ),
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("validator.ko", text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let expected = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                let Expression::Return {
                    value: Some(value), ..
                } = node.payload()
                else {
                    return None;
                };
                (sources
                    .slice(parsed.ast().expressions().get(*value).unwrap().span())
                    .unwrap()
                    == operand)
                    .then_some(id)
            })
            .collect::<Vec<_>>();
        let [expected] = expected.as_slice() else {
            panic!("one actual loop Return");
        };
        for input in [
            SourceUnitInput::new("root", "validator.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (ne, te) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne).unwrap();
            assert!(names.diagnostics().is_empty());
            let names = names.validate().unwrap();
            let source_unit = names.names().index().source_units()[0].id();
            let raw = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
            assert!(raw.diagnostics().is_empty());
            let complete =
                complete_compilation_unit_types(&sources, &inputs, &names, &te, &raw).unwrap();
            assert!(complete.is_same_analysis(&raw));
            let typed = raw.validate().unwrap();
            let valid =
                check_compilation_unit_ownership(&sources, &inputs, &names, &te, &typed).unwrap();
            assert!(valid.diagnostics().is_empty() && valid.deferred().is_empty());
            assert!(valid.is_compatible_with(&typed));
            assert!(valid.clone().validate().is_ok());
            assert_eq!(valid.iterations().len(), 1);
            assert_eq!(
                valid.iterations()[0].descriptor.statement().source_unit(),
                source_unit
            );
            let returns = valid.iterations()[0]
                .exits()
                .iter()
                .filter_map(|exit| match exit.kind() {
                    UnitIterationExitKind::Return(expression) => Some(expression),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(returns, vec![UnitExpressionId::new(source_unit, *expected)]);
            let mut damaged = valid.clone();
            let before = damaged.iterations[0].exits.len();
            damaged.iterations[0]
                .exits
                .retain(|exit| !matches!(exit.kind(), UnitIterationExitKind::Return(_)));
            assert_eq!(damaged.iterations[0].exits.len() + 1, before);
            assert_ne!(damaged, valid);
            assert_eq!(damaged.iteration_templates, valid.iteration_templates);
            assert_eq!(
                damaged.iteration_required_exits,
                valid.iteration_required_exits
            );
            assert!(damaged.diagnostics().is_empty() && damaged.deferred().is_empty());
            assert!(damaged.is_compatible_with(&typed));
            assert!(!damaged.iterations_are_valid());
            let expected_recovery = damaged.clone();
            let recovery = damaged
                .validate()
                .expect_err("missing Return must not publish");
            assert!(recovery.is_same_analysis(&expected_recovery));
            assert!(recovery.is_compatible_with(&typed));
            assert_eq!(*recovery, expected_recovery);
            assert!(valid.clone().validate().is_ok());
        }
    }
}
