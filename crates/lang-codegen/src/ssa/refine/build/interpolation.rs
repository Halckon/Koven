//! String interpolation using the typed part order and the existing String operations.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::Expression,
    source::Span,
    type_checking::{BuiltinType, UnitDisplayTarget, UnitExpressionId, UnitInterpolationPart},
};

use super::{Builder, Lowered, field::OwnerLoan};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error, string_literal},
    model::{EntityType, LoanKind, SsaTypeId},
    refine::body::{ROp, RTerm, Slot},
};

impl Builder<'_, '_> {
    pub(super) fn interpolation(
        &mut self,
        expression: ExpressionId,
        parts: &[UnitInterpolationPart],
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let depth = self.interpolation_owners.len();
        let string = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.interpolation_parts(parts, depth, string, span);
        // A terminating path emitted its own cleanup. Keep only enclosing expressions active.
        self.interpolation_owners.truncate(depth);
        result
    }

    fn interpolation_parts(
        &mut self,
        parts: &[UnitInterpolationPart],
        depth: usize,
        string: SsaTypeId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let source_text = self
            .inputs
            .sources
            .source_text(span.source_id())
            .map_err(|_| error(LoweringErrorKind::MismatchedSource, span))?;
        for part in parts {
            match *part {
                UnitInterpolationPart::Text(origin) => {
                    if origin.source_id() != span.source_id() {
                        return Err(error(LoweringErrorKind::MismatchedSource, origin));
                    }
                    let mut bytes = Vec::new();
                    string_literal::decode_text(source_text, origin, &mut bytes)?;
                    let text = self.interpolation_text(string, bytes, origin)?;
                    if self.interpolation_owners.len() == depth {
                        self.interpolation_owners.push((text, self.loops.len()));
                    } else {
                        self.append_interpolation(depth, text, string, origin)?;
                        self.push(ROp::Drop { owner: text }, Vec::new(), origin)?;
                    }
                }
                UnitInterpolationPart::Format { operand, target }
                    if operand.source_unit() == self.inputs.source_unit =>
                {
                    if self.inputs.owned.interpolation_operand_completes(operand) == Some(false) {
                        return self.interpolation_exit(operand.expression());
                    }
                    if !self.append_display_operand(operand, target, depth, string, span)? {
                        return Ok(Lowered::Diverged);
                    }
                }
                UnitInterpolationPart::Exit(operand)
                    if operand.source_unit() == self.inputs.source_unit =>
                {
                    return self.interpolation_exit(operand.expression());
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        let (result, _) = self
            .interpolation_owners
            .get(depth)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        Ok(Lowered::Value(result))
    }

    fn interpolation_exit(&mut self, operand: ExpressionId) -> Result<Lowered, LoweringError> {
        match self.expression(operand)? {
            Lowered::Diverged => Ok(Lowered::Diverged),
            _ => Err(error(LoweringErrorKind::InvalidModel, self.span(operand))),
        }
    }

    fn append_display_operand(
        &mut self,
        operand: UnitExpressionId,
        target: UnitDisplayTarget,
        depth: usize,
        string: SsaTypeId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let expression = operand.expression();
        let operand_span = self.span(expression);
        let (ty, enum_case) = self
            .inputs
            .display_receiver_type(operand)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, operand_span))?;
        let builtin = self.inputs.builtin_display(target, ty);
        let string_receiver = builtin == Some(BuiltinType::String);
        let copyable_scalar = builtin.is_some_and(|builtin| {
            matches!(builtin, BuiltinType::Boolean | BuiltinType::Char)
                || crate::ssa::refine::subset::integer(builtin)
        });
        let mut expressions = vec![expression];
        let mut root = expression;
        while let (Expression::Group { expression }, _) = self.node(root)? {
            root = expression;
            expressions.push(root);
        }
        let mut field_loans = Vec::new();
        let mut field_reads = Vec::new();
        let view = if copyable_scalar {
            // Evaluate Copyable operands through the normal value path, including loan reads.
            None
        } else if !string_receiver
            && self
                .inputs
                .typed
                .aggregate_projection(self.unit(root))
                .is_some_and(|projection| {
                    projection.kind()
                        == lang_frontend::type_checking::UnitAggregateProjectionKind::Field
                })
        {
            let (loan, created, reads) = self.field_projection_loan(root, operand_span)?;
            field_loans = created;
            field_reads = reads;
            Some(loan)
        } else {
            self.display_receiver_view(root, string_receiver)?
        };
        let value = if let Some(view) = view {
            view
        } else if enum_case {
            // Admission checked a read of an already-refined binding, with no new owner.
            self.owner_name(root, self.span(root))?
        } else {
            match self.expression_inner(root)? {
                Lowered::Value(slot) => slot,
                Lowered::Diverged => {
                    for expression in expressions.into_iter().rev() {
                        self.finish_expression(expression, Lowered::Diverged)?;
                    }
                    return Ok(false);
                }
                Lowered::Unit => return Err(error(LoweringErrorKind::MissingFact, operand_span)),
            }
        };
        let formatted = match builtin {
            Some(BuiltinType::String) => None,
            Some(BuiltinType::Boolean) => {
                Some(self.boolean_display(value, string, operand_span)?)
            }
            _ if copyable_scalar => Some(self.value(
                ROp::StringFromScalar { scalar: value },
                EntityType::Value(string),
                operand_span,
            )?),
            _ if matches!(target, UnitDisplayTarget::Requirement(_)) => {
                Some(self.call_display(operand, value, view.is_some(), string, operand_span)?)
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, operand_span)),
        };
        // Both the read and any synchronous receiver borrow end before source cleanup.
        for loan in field_loans.into_iter().rev() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), operand_span)?;
        }
        if self.interpolation_owners.len() == depth {
            let empty = self.interpolation_text(string, Vec::new(), span)?;
            self.interpolation_owners.push((empty, self.loops.len()));
        }
        self.append_interpolation(depth, formatted.unwrap_or(value), string, operand_span)?;
        if let Some(owner) = formatted {
            self.push(ROp::Drop { owner }, Vec::new(), operand_span)?;
        }
        for read in field_reads {
            self.drops(UnitDropPoint::AfterExpression(self.unit(read)))?;
        }
        for expression in expressions.into_iter().rev() {
            if view.is_some() || enum_case {
                self.drops(UnitDropPoint::AfterExpression(self.unit(expression)))?;
            } else {
                self.finish_expression(expression, Lowered::Value(value))?;
            }
        }
        Ok(true)
    }

    fn display_receiver_view(
        &self,
        root: ExpressionId,
        string_receiver: bool,
    ) -> Result<Option<Slot>, LoweringError> {
        if let Some(view) = self.narrowed_view(root)? {
            return Ok(Some(view));
        }
        if string_receiver {
            return self.borrowed_string_loan(root);
        }
        let (node, span) = self.node(root)?;
        let binding = self
            .reference(span)
            .ok()
            .and_then(|symbol| self.borrowed.get(&symbol).copied());
        let view = match node {
            Expression::Name => binding,
            Expression::This => self.receiver.or(binding),
            _ => None,
        };
        Ok(view.filter(|slot| matches!(self.body.slots[slot.0], EntityType::Loan { .. })))
    }

    fn call_display(
        &mut self,
        operand: UnitExpressionId,
        receiver: Slot,
        borrowed: bool,
        string: SsaTypeId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let site = self
            .inputs
            .source_plan
            .display_site(self.inputs.source_token, operand)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let callee = self
            .inputs
            .function_ids
            .get(site.key())
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let loan = if borrowed {
            self.shared_operand(
                OwnerLoan {
                    loan: receiver,
                    scoped: false,
                },
                span,
            )?
        } else {
            self.lend(receiver, LoanKind::Shared, span)?
        };
        let mut loans = Vec::new();
        if loan.scoped {
            loans.push(loan.loan);
        }
        let receiver =
            self.project_delegated_receiver(loan.loan, site.delegation(), &mut loans, span)?;
        let result = self.value(
            ROp::DirectCall {
                callee,
                receiver: Some(receiver),
                arguments: Vec::new(),
            },
            EntityType::Value(string),
            span,
        )?;
        for loan in loans.into_iter().rev() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        Ok(result)
    }

    fn boolean_display(
        &mut self,
        condition: Slot,
        string: SsaTypeId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let when_true = self.body.block();
        let when_false = self.body.block();
        let join = self.body.block();
        let result = self.body.slot(EntityType::Value(string));
        self.terminate(
            RTerm::Branch {
                condition,
                when_true,
                when_false,
            },
            span,
        )?;
        for (block, bytes) in [(when_true, b"true".as_slice()), (when_false, b"false")] {
            self.block = Some(block);
            let source = self.interpolation_text(string, bytes.to_vec(), span)?;
            self.push(ROp::Alias { source }, vec![result], span)?;
            self.goto(join, span)?;
        }
        self.block = Some(join);
        Ok(result)
    }

    fn interpolation_text(
        &mut self,
        string: SsaTypeId,
        bytes: Vec<u8>,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        self.value(
            ROp::StringLiteral { string, bytes },
            EntityType::Value(string),
            span,
        )
    }

    fn append_interpolation(
        &mut self,
        index: usize,
        right: Slot,
        string: SsaTypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let (left, loop_depth) = self.interpolation_owners[index];
        let result = self.value(
            ROp::StringConcat { left, right },
            EntityType::Value(string),
            span,
        )?;
        self.push(ROp::Drop { owner: left }, Vec::new(), span)?;
        self.interpolation_owners[index] = (result, loop_depth);
        Ok(())
    }

    /// Emit cleanup on the current edge without changing the enclosing expression stack.
    pub(super) fn drop_interpolation_prefixes(
        &mut self,
        min_loop_depth: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        for index in (0..self.interpolation_owners.len()).rev() {
            let (owner, loop_depth) = self.interpolation_owners[index];
            if loop_depth < min_loop_depth {
                break;
            }
            self.push(ROp::Drop { owner }, Vec::new(), span)?;
        }
        Ok(())
    }
}
