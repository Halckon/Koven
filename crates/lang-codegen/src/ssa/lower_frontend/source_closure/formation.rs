//! Capture formation and thunk views consume the existing ownership/capture descriptors.

use super::*;
use crate::ssa::model::{ClosureCaptureOperand, PlaceAccess};

impl ExpressionLowerer<'_> {
    pub(in crate::ssa::lower_frontend) fn lower_source_closure(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let owner = self
            .source_token
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let plan = self
            .source_closures
            .lambdas
            .get(&(owner, expression.index()))
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mut captures = Vec::new();
        for (field, capture) in plan.captures.iter().enumerate() {
            if capture.mode == SsaCaptureMode::Shared {
                let loan = if let Some(&source) = self.borrow_bindings.get(&capture.symbol) {
                    let (_, results) = self.append(
                        Operation::SharedReborrow { source },
                        vec![EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: capture.ty,
                        }],
                        capture.span,
                    )?;
                    require_loan(results[0], capture.span)?
                } else {
                    let Some(LoweredValue::Value(owner)) =
                        self.bindings.get(&capture.symbol).copied()
                    else {
                        return Err(error(LoweringErrorKind::MissingFact, capture.span));
                    };
                    let (_, results) = self.append(
                        Operation::RootPlace { owner },
                        vec![EntityType::Place(capture.ty)],
                        capture.span,
                    )?;
                    let EntityId::Place(place) = results[0] else {
                        return Err(error(LoweringErrorKind::InvalidModel, capture.span));
                    };
                    let (_, results) = self.append(
                        Operation::BorrowBegin {
                            place,
                            kind: LoanKind::Shared,
                        },
                        vec![EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: capture.ty,
                        }],
                        capture.span,
                    )?;
                    require_loan(results[0], capture.span)?
                };
                self.capture_loans.insert((expression.index(), field), loan);
                captures.push(ClosureCaptureOperand::Shared(loan));
            } else {
                let operand = match capture.effect {
                    ClosureCaptureEffect::Copy => {
                        if let Some(LoweredValue::Value(source)) =
                            self.bindings.get(&capture.symbol).copied()
                        {
                            let (_, results) = self.append(
                                Operation::Copy { source },
                                vec![EntityType::Value(capture.ty)],
                                capture.span,
                            )?;
                            value(results[0])
                        } else if let Some(&source) = self.borrow_bindings.get(&capture.symbol) {
                            let (_, results) = self.append(
                                Operation::Read {
                                    source: PlaceAccess::Loan(source),
                                },
                                vec![EntityType::Value(capture.ty)],
                                capture.span,
                            )?;
                            value(results[0])
                        } else {
                            return Err(error(LoweringErrorKind::MissingFact, capture.span));
                        }
                    }
                    ClosureCaptureEffect::Move => {
                        let Some(LoweredValue::Value(owner)) =
                            self.bindings.remove(&capture.symbol)
                        else {
                            return Err(error(LoweringErrorKind::MissingFact, capture.span));
                        };
                        self.forget_delivered_owners(&[owner]);
                        owner
                    }
                    ClosureCaptureEffect::Borrow | ClosureCaptureEffect::Unknown => {
                        return Err(error(LoweringErrorKind::UnsupportedNode, capture.span));
                    }
                };
                captures.push(ClosureCaptureOperand::Owned(operand));
            }
        }
        let operation = if captures.is_empty() {
            Operation::FunctionAddress { target: plan.thunk }
        } else {
            Operation::ClosureConstruct {
                closure: plan.callable,
                thunk: plan.thunk,
                captures,
            }
        };
        let (_, results) = self.append(operation, vec![EntityType::Value(plan.callable)], span)?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    /// Lambda tail evaluation preserves ordinary statement/drop facts and the void Unit ABI.
    pub(in crate::ssa::lower_frontend) fn lower_lambda_body(
        &mut self,
        plan: &ClosurePlan,
    ) -> Result<LoweredValue, LoweringError> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(plan.body)
            .map_err(|_| error(LoweringErrorKind::MissingFact, plan.span))?;
        let lang_frontend::parser::Statement::LambdaBody { elements } = node.payload() else {
            return Err(error(LoweringErrorKind::MissingFact, plan.span));
        };
        let elements = elements.clone();
        let mut result = LoweredValue::Unit;
        for statement in elements {
            result = self.lower_statement(statement)?;
            if matches!(result, LoweredValue::Diverged) {
                return Ok(result);
            }
        }
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::AfterStatement(plan.body))?;
        if builtin_type(self.typed, plan.return_type) == Some(BuiltinType::Unit) {
            Ok(LoweredValue::Unit)
        } else if plan.result_expression.is_some() {
            Ok(result)
        } else {
            Err(error(LoweringErrorKind::MissingFact, plan.span))
        }
    }

    pub(in crate::ssa::lower_frontend) fn bind_capture_views(
        &mut self,
        plan: &ClosurePlan,
    ) -> Result<(), LoweringError> {
        let parameters = self
            .function
            .block(self.block)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, plan.span))?
            .parameters
            .clone();
        let offset = usize::from(!plan.captures.is_empty());
        for (&symbol, &parameter) in plan.parameter_symbols.iter().zip(&parameters[offset..]) {
            match parameter {
                EntityId::Value(owner) => {
                    self.bindings.insert(symbol, LoweredValue::Value(owner));
                }
                EntityId::Loan(loan) => {
                    self.borrow_bindings.insert(symbol, loan);
                }
                EntityId::Place(_) => {
                    return Err(error(LoweringErrorKind::InvalidModel, plan.span));
                }
            }
        }
        if plan.captures.is_empty() {
            return Ok(());
        }
        let environment = require_loan(parameters[0], plan.span)?;
        for (field, capture) in plan.captures.iter().enumerate() {
            let (_, results) = self.append(
                Operation::SharedFieldLoan {
                    base: environment,
                    field,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: capture.field_type,
                }],
                capture.span,
            )?;
            let slot = require_loan(results[0], capture.span)?;
            let view = if capture.mode == SsaCaptureMode::Shared {
                self.capture_loans
                    .insert((plan.expression.index(), field), slot);
                let (_, results) = self.append(
                    Operation::SharedReferenceFollow { source: slot },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: capture.ty,
                    }],
                    capture.span,
                )?;
                require_loan(results[0], capture.span)?
            } else {
                slot
            };
            self.borrow_bindings.insert(capture.symbol, view);
        }
        Ok(())
    }

    /// Close thunk-local views in child-before-parent order, using current CFG rebound IDs.
    pub(in crate::ssa::lower_frontend) fn end_thunk_capture_views(
        &mut self,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(expression) = self.thunk_expression else {
            return Ok(());
        };
        let owner = self
            .source_token
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let plan = self
            .source_closures
            .lambdas
            .get(&(owner, expression.index()))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let captures = plan
            .captures
            .iter()
            .enumerate()
            .map(|(field, capture)| (field, capture.symbol))
            .collect::<Vec<_>>();
        for (field, symbol) in captures.into_iter().rev() {
            let loan = self
                .borrow_bindings
                .remove(&symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
            if let Some(loan) = self.capture_loans.remove(&(expression.index(), field)) {
                self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
            }
        }
        Ok(())
    }

    /// SSA Drop already releases a closure's shared dependencies. Retire only the carrier slots.
    /// Emitting another BorrowEnd would double-end the current capture loan.
    pub(in crate::ssa::lower_frontend) fn release_owner_capture_loans(
        &mut self,
        owner: crate::ssa::model::ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(EntityType::Value(ty)) = self
            .function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.ty)
        else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let Some(plan) = self
            .source_closures
            .lambdas
            .values()
            .find(|plan| plan.callable == ty && !plan.captures.is_empty())
        else {
            return Ok(());
        };
        let keys = plan
            .captures
            .iter()
            .enumerate()
            .filter_map(|(field, capture)| {
                (capture.mode == SsaCaptureMode::Shared).then_some((plan.expression.index(), field))
            })
            .collect::<Vec<_>>();
        for key in keys.into_iter().rev() {
            self.capture_loans
                .remove(&key)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        }
        Ok(())
    }
}

fn require_loan(entity: EntityId, span: Span) -> Result<crate::ssa::model::LoanId, LoweringError> {
    match entity {
        EntityId::Loan(loan) => Ok(loan),
        _ => Err(error(LoweringErrorKind::InvalidModel, span)),
    }
}
