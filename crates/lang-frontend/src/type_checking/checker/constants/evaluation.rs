//! Evaluate only qualified, acyclic constants; runtime short-circuit does not alter syntax edges.

use super::super::*;
use crate::parser::{BinaryOperator, Expression, LiteralKind, PrefixOperator, StringPart};
use crate::type_checking::constant_value::{ConstValue as Value, decode_text};

enum Step {
    Enter(ExpressionId),
    Prefix(PrefixOperator, Span),
    Left(BinaryOperator, ExpressionId, Span),
    Binary(BinaryOperator, Value, Span),
}

impl Checker<'_> {
    pub(in crate::type_checking::checker) fn evaluate_constants(
        &mut self,
    ) -> Result<(), TypeCheckingError> {
        let mut remaining = vec![0; self.symbol_kinds.len()];
        let mut reverse = vec![Vec::new(); self.symbol_kinds.len()];
        let mut ready = std::collections::VecDeque::new();
        for (&symbol, dependencies) in &self.constant_dependencies {
            remaining[symbol.index()] = dependencies.len();
            if dependencies.is_empty() {
                ready.push_back(symbol);
            }
            for target in dependencies {
                reverse[target.index()].push(symbol);
            }
        }
        while let Some(symbol) = ready.pop_front() {
            if self.constant_dependencies[&symbol]
                .iter()
                .all(|target| self.constant_values.contains_key(target))
                && let Some(&item) = self.constant_items.get(&symbol)
                && let Item::Constant { initializer, .. } = self.ast().items().get(item)?.payload()
                && let Some(value) = self.evaluate_constant_expression(*initializer)?
            {
                self.constant_values.insert(symbol, value);
            }
            for &dependent in &reverse[symbol.index()] {
                remaining[dependent.index()] -= 1;
                if remaining[dependent.index()] == 0 {
                    ready.push_back(dependent);
                }
            }
        }
        Ok(())
    }

    fn evaluate_constant_expression(
        &mut self,
        root: ExpressionId,
    ) -> Result<Option<Value>, TypeCheckingError> {
        let mut pending = vec![Step::Enter(root)];
        let mut value = None;
        while let Some(step) = pending.pop() {
            match step {
                Step::Enter(expression) => {
                    let node = self.ast().expressions().get(expression)?;
                    let span = node.span();
                    match node.payload().clone() {
                        Expression::Group { expression } => pending.push(Step::Enter(expression)),
                        Expression::Name => {
                            value = match self.reference(span, Namespace::Value) {
                                Some(ReferenceTarget::Symbol(symbol)) => {
                                    self.constant_values.get(symbol).cloned()
                                }
                                _ => None,
                            };
                        }
                        Expression::Member { .. } => {
                            value = self
                                .associated_constant_uses
                                .get(&expression.index())
                                .and_then(|symbol| self.constant_values.get(symbol))
                                .cloned();
                        }
                        Expression::Literal(LiteralKind::Boolean(boolean)) => {
                            value = Some(Value::Boolean(boolean))
                        }
                        Expression::Literal(LiteralKind::Integer(kind)) => {
                            value = self.constant_integer_literal(expression, span, kind, false)?;
                        }
                        Expression::Literal(LiteralKind::Char) => {
                            let text = self.sources.slice(span)?;
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
                                let Some(text) = decode_text(self.sources.slice(span)?) else {
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
                            let operand_node = self.ast().expressions().get(operand)?;
                            // Negative literal magnitude can be one greater than the positive type bound.
                            if operator == PrefixOperator::Minus
                                && let Expression::Literal(LiteralKind::Integer(kind)) =
                                    operand_node.payload()
                            {
                                value = self.constant_integer_literal(
                                    expression,
                                    operand_node.span(),
                                    *kind,
                                    true,
                                )?;
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
                            pending.push(Step::Left(operator, right, operator_span));
                            pending.push(Step::Enter(left));
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
                        return self.constant_evaluation_failure(span);
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
                        return self.constant_evaluation_failure(span);
                    }
                }
            }
        }
        Ok(value)
    }

    fn constant_integer_literal(
        &self,
        expression: ExpressionId,
        span: Span,
        kind: crate::parser::IntegerLiteralKind,
        negative: bool,
    ) -> Result<Option<Value>, TypeCheckingError> {
        let Some(TypeKind::Builtin(ty)) =
            self.expression_types[expression.index()].map(|ty| self.kind(ty))
        else {
            return Ok(None);
        };
        Ok(self
            .integer_magnitude(span, kind)?
            .and_then(|magnitude| i128::try_from(magnitude).ok())
            .and_then(|value| Value::integer(*ty, if negative { -value } else { value })))
    }

    fn constant_evaluation_failure(
        &mut self,
        span: Span,
    ) -> Result<Option<Value>, TypeCheckingError> {
        self.emit(
            self.constant_evaluation_code,
            "constant arithmetic is outside its defined range",
            span,
        )?;
        Ok(None)
    }
}
