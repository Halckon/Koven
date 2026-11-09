//! Ordinary Rc if proofs come from real null-comparison, Rc-effect and non-null-use facts.
//! This does not change the separate nullable While/When or generic call gates.
use super::*;

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
            if !super::super::super::span_contains(body.span(), operation.span())
                || super::super::super::enclosing_lambda(self.parsed, operation.span()).is_some()
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
        }))
    }
}
