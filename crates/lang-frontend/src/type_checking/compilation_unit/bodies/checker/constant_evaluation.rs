//! Source-qualified adapter for the shared constant evaluator; no unit-specific arithmetic.
use super::BodyChecker;
use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    parser::{Expression, IntegerLiteralKind},
    source::Span,
    type_checking::{
        CompilationUnitTypeError, ConstValue, TypeCheckingError, UnitExpressionId, UnitTypeKind,
        constant_evaluation::ConstantEvaluationContext,
    },
};

impl ConstantEvaluationContext for BodyChecker<'_> {
    type Id = UnitExpressionId;
    type Error = CompilationUnitTypeError;

    fn node(&self, expression: Self::Id) -> Result<(Expression, Span), Self::Error> {
        let node = self
            .file(expression.source_unit())
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(TypeCheckingError::from)?;
        Ok((node.payload().clone(), node.span()))
    }

    fn child(&self, parent: Self::Id, local: ExpressionId) -> Self::Id {
        UnitExpressionId::new(parent.source_unit(), local)
    }

    fn text(&self, span: Span) -> Result<&str, Self::Error> {
        Ok(self.sources.slice(span).map_err(TypeCheckingError::from)?)
    }

    fn reference_value(&self, expression: Self::Id) -> Option<ConstValue> {
        self.parts
            .constant_selections
            .get(&expression)
            .and_then(|symbol| self.constant_values.get(symbol))
            .cloned()
    }

    fn integer_value(
        &self,
        expression: Self::Id,
        span: Span,
        kind: IntegerLiteralKind,
        negative: bool,
    ) -> Result<Option<ConstValue>, Self::Error> {
        let Some(UnitTypeKind::Builtin(ty)) = self
            .parts
            .expression_types
            .get(&expression)
            .and_then(|ty| self.signatures.types().get(*ty))
        else {
            return Ok(None);
        };
        let text = self.text(span)?;
        Ok(crate::type_checking::integer_literal_magnitude(text, kind)
            .and_then(|value| i128::try_from(value).ok())
            .and_then(|value| ConstValue::integer(*ty, if negative { -value } else { value })))
    }

    fn evaluation_failure(&mut self, span: Span) -> Result<Option<ConstValue>, Self::Error> {
        self.emit(
            codes::CONSTANT_EVALUATION_FAILURE,
            "constant arithmetic is outside its defined range",
            span,
        )?;
        Ok(None)
    }
}
