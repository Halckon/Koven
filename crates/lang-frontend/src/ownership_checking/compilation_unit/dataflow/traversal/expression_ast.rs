//! AST dispatch after typed operations and the direct short-circuit chain.
use super::{
    AccessKind, Checker, Expression, ExpressionId, ExpressionUse, Flows, OwnershipCheckingError,
    State,
};
use crate::parser::StringPart;

impl Checker<'_> {
    pub(super) fn check_expression_ast(
        &mut self,
        id: ExpressionId,
        state: State,
        usage: ExpressionUse,
        escaping: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(id)?;
        let span = node.span();
        match node.payload().clone() {
            Expression::Error | Expression::Literal(_) | Expression::SuperMember { .. } => {
                Ok(Flows::next(state))
            }
            Expression::This => {
                let mut state = state;
                if self.reference_symbol(span).is_some() {
                    self.use_name(id, span, usage, &mut state)?;
                } else {
                    self.use_this(id, span, usage, &mut state)?;
                }
                Ok(Flows::next(state))
            }
            Expression::Name => {
                let mut state = state;
                self.use_name(id, span, usage, &mut state)?;
                Ok(Flows::next(state))
            }
            Expression::Group { expression } => {
                self.check_expression_with_escape(expression, state, usage, escaping)
            }
            Expression::String { parts } => {
                let mut flows = Flows::next(state);
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        flows = self.chain_expression(flows, expression, ExpressionUse::Read)?;
                    }
                }
                Ok(flows)
            }
            Expression::Lambda { body, .. } => self.check_lambda(id, body, state),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let branch_usage = self.control_result_usage(id)?;
                self.check_if(
                    id,
                    condition,
                    (then_branch, else_branch),
                    state,
                    branch_usage,
                    false,
                )
            }
            Expression::When {
                subject, entries, ..
            } => {
                let branch_usage = self.control_result_usage(id)?;
                self.check_when(id, subject, &entries, state, branch_usage, false)
            }
            Expression::Return { value, .. } => {
                let mut flows = if let Some(value) = value {
                    self.check_return_expression(
                        value,
                        state,
                        ExpressionUse::Consume {
                            parameter_span: None,
                        },
                    )?
                } else {
                    Flows::next(state)
                };
                flows.next = None;
                Ok(flows)
            }
            Expression::Break { .. } => {
                self.no_carry_break(id, &state);
                Ok(Flows {
                    breaks: Some(state),
                    ..Flows::default()
                })
            }
            Expression::Continue { .. } => Ok(Flows {
                continues: Some(state),
                ..Flows::default()
            }),
            Expression::NonNullAssert { operand, .. } => {
                self.check_non_null_assertion(id, operand, state, escaping)
            }
            Expression::Prefix { operand, .. }
            | Expression::Cast {
                expression: operand,
                ..
            }
            | Expression::TypeTest {
                expression: operand,
                ..
            }
            | Expression::Propagate { value: operand, .. } => {
                self.check_expression(operand, state, ExpressionUse::Read)
            }
            Expression::Binary {
                left,
                operator: crate::parser::BinaryOperator::Elvis,
                right,
                ..
            } if escaping => {
                let mut flows = self.check_expression(left, state, ExpressionUse::Read)?;
                if let Some(next) = flows.next.take() {
                    flows.merge(self.check_escaping_expression(
                        right,
                        next,
                        ExpressionUse::Read,
                    )?);
                }
                Ok(flows)
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                if self.uses_string_binary_operands(left, operator, right) {
                    self.check_binary_read(id, left, right, state)
                } else {
                    let flows = self.check_expression(left, state, ExpressionUse::Read)?;
                    self.chain_expression(flows, right, ExpressionUse::Read)
                }
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => {
                if let Some(put) = self.typed.map_put(self.unit_expression(target)) {
                    return self.check_map_assignment(put, state);
                }
                let diagnostic_count = self.diagnostics.len();
                if self.place(target)?.is_some_and(|place| {
                    !place.fields().is_empty()
                        || self.symbol_kind(place.root())
                            == Some(crate::name_resolution::SymbolKind::Field)
                }) {
                    self.reject_borrowed_closure_escape(value, &state)?;
                }
                if self.diagnostics.len() != diagnostic_count {
                    return self.check_expression(
                        value,
                        state,
                        ExpressionUse::Consume {
                            parameter_span: None,
                        },
                    );
                }
                self.check_assignment(id, target, operator, value, state)
            }
            Expression::Member {
                receiver,
                name_span,
                ..
            } => {
                if let ExpressionUse::Consume { parameter_span } = usage
                    && self
                        .typed
                        .expression_type(self.unit_expression(id))
                        .is_some_and(|ty| {
                            self.typed.copyability(ty)
                                == crate::type_checking::Copyability::MoveOnly
                        })
                    && let Some(owner) = self.rc_value_receiver(id)?
                {
                    let diagnostic_count = self.diagnostics.len();
                    let flows = self.check_expression(receiver, state, ExpressionUse::Read)?;
                    if flows.next.is_some() && self.diagnostics.len() == diagnostic_count {
                        self.report_rc_payload_move(owner, name_span, parameter_span)?;
                    }
                    return Ok(flows);
                }
                let mut state = state;
                if let Some(place) = self.place(id)? {
                    match usage {
                        ExpressionUse::Read => {
                            self.access_place(
                                &place,
                                AccessKind::Read,
                                name_span,
                                None,
                                &mut state,
                            )?;
                        }
                        ExpressionUse::Consume { parameter_span } => {
                            self.consume_place(&place, id, name_span, parameter_span, &mut state)?;
                        }
                        ExpressionUse::Place { parameter_span } => {
                            self.ensure_available(&place, name_span, parameter_span, &state)?;
                        }
                    }
                    Ok(Flows::next(state))
                } else if self
                    .typed
                    .aggregate_projection(self.unit_expression(id))
                    .is_some()
                    && self.element_place_descriptor(receiver)?.is_some()
                {
                    // 与单文件所有权检查保持一致：元素后的聚合字段投影仍是 Place，
                    // 其精确 drop 计划由 liveness 标记为 IndexPlace 暂缓。
                    self.check_expression(
                        receiver,
                        state,
                        ExpressionUse::Place {
                            parameter_span: None,
                        },
                    )
                } else {
                    let diagnostic_count = self.diagnostics.len();
                    let flows = self.check_expression(receiver, state, ExpressionUse::Read)?;
                    if flows.next.is_some()
                        && self.diagnostics.len() == diagnostic_count
                        && let ExpressionUse::Consume { parameter_span } = usage
                    {
                        self.reject_unrooted_field_move(id, name_span, parameter_span)?;
                    }
                    Ok(flows)
                }
            }
            Expression::Call {
                callee, arguments, ..
            } => self.check_call(id, callee, &arguments, state),
            Expression::Index { receiver, index } => {
                if self.element_place_descriptor(id)?.is_some() {
                    return self.check_element_expression(id, state, usage);
                }
                let flows = self.check_expression(receiver, state, ExpressionUse::Read)?;
                self.chain_expression(flows, index, ExpressionUse::Read)
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.check_expression(receiver, state, ExpressionUse::Read)
                } else {
                    Ok(Flows::next(state))
                }
            }
        }
    }
}
