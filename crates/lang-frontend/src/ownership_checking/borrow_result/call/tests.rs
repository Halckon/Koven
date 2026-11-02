use super::*;
use crate::{lexer::lex, parser::parse_file, source::SourceMap};

fn operand(text: &str) -> (ParsedFile, ExpressionId) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("borrow-call.ko", format!("fun test(): Int = {text}"))
        .unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let expressions = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()).unwrap() == text).then_some(id))
        .collect::<Vec<_>>();
    let [expression] = expressions.as_slice() else {
        panic!("unique operand")
    };
    let expression = *expression;
    (parsed, expression)
}

#[test]
fn only_groups_forward_the_original_call_to_the_typed_predicate() {
    for text in ["view(source)", "((view(source)))"] {
        let (parsed, expression) = operand(text);
        let calls = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), Expression::Call { .. }).then_some(id)
            })
            .collect::<Vec<_>>();
        let [call] = calls.as_slice() else {
            panic!("one call")
        };
        for accepted in [false, true] {
            let mut observed = Vec::new();
            let result = borrow_result_call(&parsed, expression, |id| {
                observed.push(id);
                accepted
            });
            assert_eq!(observed, vec![*call]);
            assert_eq!(result, accepted.then_some(*call));
        }
    }
    for text in [
        "source",
        "(source)",
        "1",
        "-view(source)",
        "view(source) + 1",
    ] {
        let (parsed, expression) = operand(text);
        assert_eq!(
            borrow_result_call(&parsed, expression, |_| panic!("not a call operand")),
            None
        );
    }
}

#[test]
fn malformed_ast_lookup_remains_absent_without_invoking_typed_resolution() {
    let (parsed, _) = operand("0");
    let (larger, _) = operand("((view(source)))");
    let invalid = larger
        .ast()
        .expressions()
        .iter()
        .map(|(id, _)| id)
        .find(|id| parsed.ast().expressions().get(*id).is_err())
        .unwrap();
    assert_eq!(
        borrow_result_call(&parsed, invalid, |_| panic!("invalid expression")),
        None
    );
}
