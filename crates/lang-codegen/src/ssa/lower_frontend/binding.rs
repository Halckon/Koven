//! 显式 local binding 的 owned/borrowed 交付。
use super::*;
impl ExpressionLowerer<'_> {
    /// Consume the original owner before changing its SSA identity for approved T -> T?.
    pub(super) fn adapt_owned_value_to_expected(
        &mut self,
        expression: ExpressionId,
        owner: ValueId,
        expected: TypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let actual = self
            .typed
            .expression_type(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let actual = self.resolve_type(actual, span)?;
        let expected = self.resolve_type(expected, span)?;
        if actual == expected {
            return Ok(owner);
        }
        if !matches!(self.typed.types().get(expected), Some(TypeKind::Nullable(inner)) if *inner == actual)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let nullable = self
            .type_ids
            .get(&expected)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if self.map_results.contains_key(&nullable)
            && self.typed.copyability(actual) == Some(Copyability::Copyable)
        {
            return self.wrap_map_result(owner, nullable, span);
        }
        self.forget_delivered_owners(&[owner]);
        let (_, results) = self.append(
            Operation::NullableWrap { nullable, owner },
            vec![EntityType::Value(nullable)],
            span,
        )?;
        Ok(value(results[0]))
    }

    /// Adapt only the already-mapped owned source-call boundary, preserving its
    /// existing argument slot so later operands carry the wrapped owner.
    pub(super) fn adapt_pending_source_value_argument(
        &mut self,
        expression: ExpressionId,
        owner: ValueId,
        expected: TypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        use crate::ssa::model::SsaTypeKind;
        let qualified = (|| {
            let actual = self
                .resolve_type(self.typed.expression_type(expression)?, span)
                .ok()?;
            let expected = self.resolve_type(expected, span).ok()?;
            if self.typed.copyability(actual) != Some(Copyability::MoveOnly)
                || !matches!(self.typed.types().get(expected), Some(TypeKind::Nullable(inner)) if *inner == actual)
                || self.temporaries.get(&expression.index()).copied() != Some(owner)
            {
                return None;
            }
            let actual_ssa = *self.type_ids.get(&actual)?;
            let nullable = *self.type_ids.get(&expected)?;
            if !matches!(
                self.ssa_types.get(actual_ssa.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            ) || !matches!(self.ssa_types.get(nullable.index()), Some(SsaTypeKind::NullableHandle { inner }) if *inner == actual_ssa)
                || self
                    .function
                    .entity(EntityId::Value(owner))
                    .map(|entity| entity.ty)
                    != Some(EntityType::Value(actual_ssa))
            {
                return None;
            }
            Some(expected)
        })();
        let Some(expected) = qualified else {
            return Ok(owner);
        };
        let wrapped = self.adapt_owned_value_to_expected(expression, owner, expected, span)?;
        self.temporaries.insert(expression.index(), wrapped);
        Ok(wrapped)
    }

    pub(super) fn lower_local_variable(
        &mut self,
        declaration: ItemId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let (item, _) = orchestrate::unwrap_modified(self.parsed, declaration)?;
        let Item::Variable {
            name, initializer, ..
        } = item
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if let Some(fact) = self
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.initializer() == initializer)
        {
            return self.lower_result_binding(fact.binding(), initializer, span);
        }
        let mut lowered = self.lower(initializer)?;
        if matches!(lowered, LoweredValue::Diverged) {
            return Ok(lowered);
        }
        let name_span =
            present_name(name).ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.source_slice(name_span)? == "_" {
            return Ok(LoweredValue::Unit);
        }
        let symbol = self.declaration_symbol(name_span, SymbolKind::Variable)?;
        let declared = self
            .typed
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        // The new local owns a MoveOnly initializer, including grouped aliases.
        // Retire the source before nullable wrapping can replace its SSA identity.
        if let LoweredValue::Value(owner) = lowered {
            let concrete = self.resolve_type(declared, span)?;
            if self.typed.copyability(concrete) == Some(Copyability::MoveOnly) {
                self.forget_delivered_owners(&[owner]);
            } else {
                self.temporaries.retain(|_, temporary| *temporary != owner);
            }
        }
        if let LoweredValue::Value(owner) = lowered {
            lowered = LoweredValue::Value(self.adapt_owned_value_to_expected(
                initializer,
                owner,
                declared,
                span,
            )?);
        }
        self.bindings.insert(symbol, lowered);
        Ok(LoweredValue::Unit)
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn deliver_assignment_rhs(
        &mut self,
        operator: AssignmentOperator,
        expression: ExpressionId,
        assigned: LoweredValue,
        span: Span,
    ) {
        // The original assignment installs this exact value. Deliver its owner just
        // as a local initializer does; do not change adaptation or rejection rules.
        if operator == AssignmentOperator::Assign
            && let LoweredValue::Value(owner) = assigned
            && let Some(template) = self.typed.expression_type(expression)
            && let Ok(concrete) = self.resolve_type(template, span)
            && self.typed.copyability(concrete) == Some(Copyability::MoveOnly)
        {
            self.forget_delivered_owners(&[owner]);
        }
    }
}
