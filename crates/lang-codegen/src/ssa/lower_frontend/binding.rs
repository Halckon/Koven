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
