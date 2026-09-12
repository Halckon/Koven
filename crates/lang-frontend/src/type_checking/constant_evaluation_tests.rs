//! Exercise the shared evaluator with source-qualified AST IDs, independently of Checker.
use super::constant_evaluation::{ConstantEvaluationContext, evaluate};
use super::{BuiltinType, ConstValue, TypeCheckingError};
use crate::{
    ast::ExpressionId,
    lexer::lex,
    parser::{Expression, IntegerLiteralKind, Item, ParsedFile, parse_file},
    source::{SourceMap, Span},
};

type Id = (usize, ExpressionId);
struct Context {
    sources: SourceMap,
    files: Vec<ParsedFile>,
    failures: Vec<Span>,
}

impl Context {
    fn new(texts: &[&str]) -> (Self, Vec<Id>) {
        let mut sources = SourceMap::new();
        let mut files = Vec::new();
        let mut roots = Vec::new();
        for (unit, text) in texts.iter().enumerate() {
            let source = sources
                .add_source(format!("unit-{unit}.ko"), *text)
                .unwrap();
            let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
            assert!(
                parsed.diagnostics().is_empty(),
                "{:?}",
                parsed.diagnostics()
            );
            let Item::Constant { initializer, .. } = parsed
                .ast()
                .items()
                .get(parsed.roots()[0])
                .unwrap()
                .payload()
            else {
                panic!("fixture root must be a constant");
            };
            roots.push((unit, *initializer));
            files.push(parsed);
        }
        (
            Self {
                sources,
                files,
                failures: Vec::new(),
            },
            roots,
        )
    }
}

impl ConstantEvaluationContext for Context {
    type Id = Id;
    type Error = TypeCheckingError;
    fn node(&self, (unit, expression): Id) -> Result<(Expression, Span), Self::Error> {
        let node = self.files[unit].ast().expressions().get(expression)?;
        Ok((node.payload().clone(), node.span()))
    }
    fn child(&self, (unit, _): Id, local: ExpressionId) -> Id {
        (unit, local)
    }
    fn text(&self, span: Span) -> Result<&str, Self::Error> {
        Ok(self.sources.slice(span)?)
    }
    fn reference_value(&self, (unit, _): Id) -> Option<ConstValue> {
        Some(ConstValue::String(
            if unit == 0 { "甲" } else { "乙" }.as_bytes().into(),
        ))
    }
    fn integer_value(
        &self,
        _: Id,
        span: Span,
        _: IntegerLiteralKind,
        negative: bool,
    ) -> Result<Option<ConstValue>, Self::Error> {
        let magnitude: i128 = self.sources.slice(span)?.parse().unwrap();
        Ok(ConstValue::integer(
            BuiltinType::Int,
            if negative { -magnitude } else { magnitude },
        ))
    }
    fn evaluation_failure(&mut self, span: Span) -> Result<Option<ConstValue>, Self::Error> {
        self.failures.push(span);
        Ok(None)
    }
}

#[test]
fn source_qualified_children_and_selected_values_do_not_alias_local_expression_ids() {
    let (mut context, roots) = Context::new(&[
        r#"const val VALUE = ("中" + "\n") + SELECTED"#,
        r#"const val VALUE = ("文" + "\0") + SELECTED"#,
    ]);
    assert_eq!(roots[0].1, roots[1].1, "the fixture must reuse local IDs");
    // Visit in reverse order too: the evaluator must retain each root's source identity.
    for index in [1, 0, 0, 1] {
        let expected = if index == 0 { "中\n甲" } else { "文\0乙" };
        assert_eq!(
            evaluate(&mut context, roots[index]).unwrap(),
            Some(ConstValue::String(expected.as_bytes().into()))
        );
    }
    assert!(context.failures.is_empty());
}

#[test]
fn short_circuit_and_failure_spans_follow_the_selected_source() {
    let (mut context, roots) = Context::new(&[
        "const val VALUE = false && (1 / 0 == 0)",
        "const val VALUE = true && (1 / 0 == 0)",
    ]);
    assert_eq!(
        evaluate(&mut context, roots[0]).unwrap(),
        Some(ConstValue::Boolean(false))
    );
    assert!(
        context.failures.is_empty(),
        "short-circuited division must not evaluate"
    );
    assert_eq!(evaluate(&mut context, roots[1]).unwrap(), None);
    assert_eq!(context.failures.len(), 1);
    let failure = context.failures[0];
    assert_eq!(failure.source_id(), context.files[1].source_id());
    assert_eq!(context.sources.slice(failure).unwrap(), "/");
}
