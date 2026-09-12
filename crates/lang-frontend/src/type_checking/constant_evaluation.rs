//! Shared iterative evaluator; adapters supply source identity, typed literals and selected values.
use super::constant_value::{ConstValue as Value, decode_text};
use crate::{
    ast::ExpressionId,
    parser::{
        BinaryOperator, Expression, IntegerLiteralKind, LiteralKind, PrefixOperator, StringPart,
    },
    source::Span,
};

/// The adapter preserves source identity when entering a local AST child. Name selection,
/// dependency ordering and cycle rejection must be complete before evaluation starts.
pub(super) trait ConstantEvaluationContext {
    type Id: Copy;
    type Error;
    fn node(&self, expression: Self::Id) -> Result<(Expression, Span), Self::Error>;
    fn child(&self, parent: Self::Id, local: ExpressionId) -> Self::Id;
    fn text(&self, span: Span) -> Result<&str, Self::Error>;
    fn reference_value(&self, expression: Self::Id) -> Option<Value>;
    fn integer_value(
        &self,
        expression: Self::Id,
        span: Span,
        kind: IntegerLiteralKind,
        negative: bool,
    ) -> Result<Option<Value>, Self::Error>;
    fn evaluation_failure(&mut self, span: Span) -> Result<Option<Value>, Self::Error>;
}

enum Step<Id> {
    Enter(Id),
    Prefix(PrefixOperator, Span),
    Left(BinaryOperator, Id, Span),
    Binary(BinaryOperator, Value, Span),
}

pub(super) fn evaluate<C: ConstantEvaluationContext>(
    context: &mut C,
    root: C::Id,
) -> Result<Option<Value>, C::Error> {
    let mut pending = vec![Step::Enter(root)];
    let mut value = None;
    while let Some(step) = pending.pop() {
        match step {
            Step::Enter(expression) => {
                let (node, span) = context.node(expression)?;
                match node {
                    Expression::Group { expression: child } => {
                        pending.push(Step::Enter(context.child(expression, child)))
                    }
                    Expression::Name | Expression::Member { .. } => {
                        value = context.reference_value(expression);
                    }
                    Expression::Literal(LiteralKind::Boolean(boolean)) => {
                        value = Some(Value::Boolean(boolean))
                    }
                    Expression::Literal(LiteralKind::Integer(kind)) => {
                        value = context.integer_value(expression, span, kind, false)?;
                    }
                    Expression::Literal(LiteralKind::Char) => {
                        let text = context.text(span)?;
                        value = text
                            .strip_prefix('\'')
                            .and_then(|text| text.strip_suffix('\''))
                            .and_then(decode_text)
                            .and_then(|text| {
                                let mut chars = text.chars();
                                let first = chars.next()?;
                                chars.next().is_none().then_some(Value::Char(first))
                            });
                    }
                    Expression::String { parts } => {
                        let mut bytes = Vec::new();
                        for part in parts {
                            let StringPart::Text(span) = part else {
                                return Ok(None);
                            };
                            let Some(text) = decode_text(context.text(span)?) else {
                                return Ok(None);
                            };
                            bytes.extend_from_slice(text.as_bytes());
                        }
                        value = Some(Value::String(bytes.into()));
                    }
                    Expression::Prefix {
                        operator,
                        operator_span,
                        operand,
                    } => {
                        let operand = context.child(expression, operand);
                        let (operand_node, operand_span) = context.node(operand)?;
                        // Negative literal magnitude can be one greater than the positive type bound.
                        if operator == PrefixOperator::Minus
                            && let Expression::Literal(LiteralKind::Integer(kind)) = &operand_node
                        {
                            value = context.integer_value(expression, operand_span, *kind, true)?;
                        } else {
                            pending.push(Step::Prefix(operator, operator_span));
                            pending.push(Step::Enter(operand));
                        }
                    }
                    Expression::Binary {
                        left,
                        operator,
                        operator_span,
                        right,
                    } => {
                        pending.push(Step::Left(
                            operator,
                            context.child(expression, right),
                            operator_span,
                        ));
                        pending.push(Step::Enter(context.child(expression, left)));
                    }
                    _ => return Ok(None),
                }
            }
            Step::Prefix(operator, span) => {
                let Some(operand) = value.take() else {
                    return Ok(None);
                };
                value = operand.prefix(operator);
                if value.is_none() {
                    return context.evaluation_failure(span);
                }
            }
            Step::Left(operator, right, span) => {
                let Some(left) = value.take() else {
                    return Ok(None);
                };
                if matches!(
                    (&left, operator),
                    (Value::Boolean(false), BinaryOperator::LogicalAnd)
                        | (Value::Boolean(true), BinaryOperator::LogicalOr)
                ) {
                    value = Some(left);
                } else {
                    pending.push(Step::Binary(operator, left, span));
                    pending.push(Step::Enter(right));
                }
            }
            Step::Binary(operator, left, span) => {
                let Some(right) = value.take() else {
                    return Ok(None);
                };
                value = left.binary(operator, right);
                if value.is_none() {
                    return context.evaluation_failure(span);
                }
            }
        }
    }
    Ok(value)
}
