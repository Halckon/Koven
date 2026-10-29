//! Ordinary Rc if proofs come from real null-comparison, Rc-effect and non-null-use facts.
//! This does not change the separate nullable While/When or generic call gates.
use super::*;
use lang_frontend::type_checking::RcOperationKind;

impl UnitExpressionLowerer<'_> {
    pub(super) fn prepare_rc_nullable_if_entry(
        &self,
        control: UnitExpressionId,
        condition: ExpressionId,
        span: Span,
    ) -> Result<Option<NullableIfEntry>, LoweringError> {
        let Some(descriptor) = self.map_borrow_condition(condition, span)? else {
            return Ok(None);
        };
        let root = descriptor.symbol();
        let concrete = resolve_concrete_type(
            self.typed,
            descriptor.nullable_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        let Some(UnitTypeKind::Nullable(narrowed)) = self.typed.types().get(concrete) else {
            return Ok(None);
        };
        if !matches!(
            self.typed.types().get(*narrowed),
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Rc,
                ..
            })
        ) {
            return Ok(None);
        }
        if !descriptor.non_null_when_true()
            || self.thunk_expression.is_some()
            || !self.is_direct_function_body_if(control, span)?
        {
            return Ok(None);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(control.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::If {
            condition: actual,
            then_branch,
            else_branch: None,
            ..
        } = *node.payload()
        else {
            return Ok(None);
        };
        let comparison = self.ungroup_map_if_expression(condition, span)?;
        if actual != condition
            || control.source_unit() != self.source_unit
            || root.source_unit() != self.source_unit
            || descriptor.expression() != UnitExpressionId::new(self.source_unit, comparison)
            || self.typed.symbol_type(root) != Some(descriptor.nullable_type())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let comparison_node = self
            .parsed
            .ast()
            .expressions()
            .get(comparison)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary {
            left,
            operator: BinaryOperator::NotEqual,
            right,
            ..
        } = *comparison_node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let left = self.ungroup_map_if_expression(left, span)?;
        let right = self.ungroup_map_if_expression(right, span)?;
        let nodes = self.parsed.ast().expressions();
        let is_null = |id| {
            nodes
                .get(id)
                .is_ok_and(|node| matches!(node.payload(), Expression::Literal(LiteralKind::Null)))
        };
        let subject = match (is_null(left), is_null(right)) {
            (true, false) => right,
            (false, true) => left,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let subject = nodes
            .get(subject)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(subject.payload(), Expression::Name)
            || self.references.get(&span_key(subject.span())) != Some(&root)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let body = self
            .parsed
            .ast()
            .statements()
            .get(then_branch)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let mut required = false;
        for effect in self.owned.rc_effects() {
            if effect.expression().source_unit() != self.source_unit {
                continue;
            }
            let Some(use_fact) = self.typed.non_null_use(effect.receiver()) else {
                continue;
            };
            if use_fact.symbol() != root {
                continue;
            }
            let operation = nodes
                .get(effect.expression().expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if !super::super::super::map_iteration::contains(body.span(), operation.span())
                || super::super::super::map_iteration::enclosing_lambda(
                    self.parsed,
                    operation.span(),
                )
                .is_some()
            {
                continue;
            }
            self.validated_rc_operation(effect.expression(), operation.span())?;
            if use_fact.declared_type() != descriptor.nullable_type()
                || use_fact.expression() != effect.receiver()
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    operation.span(),
                ));
            }
            required = true;
        }
        if !required {
            return Ok(None);
        }
        // No adapter-local operand, hidden receiver or other carrier is admitted here.
        self.ensure_no_while_proof_carriers(span)?;
        if !self.pending_call_frames.is_empty()
            || !self.temporaries.is_empty()
            || !self.loops.is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let nullable = *self
            .type_ids
            .get(&concrete)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let inner = *self
            .type_ids
            .get(narrowed)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self.typed.copyability(concrete) != Copyability::MoveOnly
            || self.ssa_types.get(nullable.index()) != Some(&SsaTypeKind::NullableHandle { inner })
            || !matches!(
                self.ssa_types.get(inner.index()),
                Some(SsaTypeKind::SharedOwner { .. })
            )
            || self
                .function
                .entity(EntityId::Value(owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(nullable))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(Some(NullableIfEntry {
            root,
            owner,
            inner,
            scope: control,
            non_null_when_true: true,
            consume: None,
        }))
    }

    /// Only this intrinsic Share can cross the existing live-If call gate.
    pub(in crate::ssa::unit_lower) fn can_share_with_nullable_rc_proof(
        &self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        if self.nullable_proofs.is_empty() {
            return Ok(false);
        }
        let Some(descriptor) = self.typed.rc_operation(expression) else {
            return Ok(false);
        };
        if descriptor.kind() != RcOperationKind::Share {
            return Ok(false);
        }
        let descriptor = self.validated_rc_operation(expression, span)?;
        Ok(self
            .nullable_rc_operand(descriptor.receiver(), span)?
            .is_some())
    }

    /// A non-null view never substitutes for an owned Value. Both Rc operations borrow it.
    pub(in crate::ssa::unit_lower) fn nullable_rc_operand(
        &self,
        receiver: UnitExpressionId,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        let Some(use_fact) = self.typed.non_null_use(receiver) else {
            return Ok(None);
        };
        let root = use_fact.symbol();
        let proof = self
            .nullable_proofs
            .get(&root)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        let declared = resolve_concrete_type(
            self.typed,
            use_fact.declared_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        let narrowed = resolve_concrete_type(
            self.typed,
            use_fact.narrowed_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        if proof.if_scope().is_none()
            || root.source_unit() != self.source_unit
            || self.typed.symbol_type(root) != Some(use_fact.declared_type())
            || !matches!(self.typed.types().get(declared), Some(UnitTypeKind::Nullable(inner)) if *inner == narrowed)
            || self.type_ids.get(&narrowed) != Some(&proof.inner)
            || !matches!(
                self.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::SharedOwner { .. })
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let mut operand = receiver;
        loop {
            let fact = self
                .typed
                .non_null_use(operand)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if operand.source_unit() != self.source_unit
                || fact.expression() != operand
                || fact.symbol() != root
                || fact.declared_type() != use_fact.declared_type()
                || fact.narrowed_type() != use_fact.narrowed_type()
                || self.typed.expression_type(operand) != Some(fact.narrowed_type())
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => {
                    operand = UnitExpressionId::new(self.source_unit, *expression)
                }
                Expression::Name if self.references.get(&span_key(node.span())) == Some(&root) => {
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
        }
        Ok(Some(proof.loan))
    }
}
