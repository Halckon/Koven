//! Named owned String? views reuse the ordinary If proof and retirement protocol.
use super::{heap::NamedNullableIfRoot, *};

impl UnitExpressionLowerer<'_> {
    pub(super) fn prepare_string_nullable_if_entry(
        &self,
        control: UnitExpressionId,
        condition: ExpressionId,
        span: Span,
    ) -> Result<Option<NullableIfEntry>, LoweringError> {
        let Some(comparison) = self.map_borrow_condition(condition, span)? else {
            return Ok(None);
        };
        let declared = comparison.nullable_type();
        let Some(UnitTypeKind::Nullable(narrowed)) = self.typed.types().get(declared) else {
            return Ok(None);
        };
        if builtin_type(self.typed, *narrowed) != Some(BuiltinType::String)
            || !self.loops.is_empty()
            || !self.pending_call_frames.is_empty()
            || !self.pending_operands.is_empty()
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
            else_branch,
            ..
        } = *node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if actual != condition || node.span() != span {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let Some(non_null) = (if comparison.non_null_when_true() {
            Some(then_branch)
        } else {
            else_branch
        }) else {
            return Ok(None);
        };
        let body = self.statement_span(non_null)?;
        let root = comparison.symbol();
        // Do not mint an unused proof for ordinary null-status checks. The witness
        // must be an actual narrowed root loan within this non-null arm.
        if !self.owned.loans().iter().any(|fact| {
            fact.kind() == FrontendLoanKind::Shared
                && fact.call().source_unit() == self.source_unit
                && fact.argument().source_unit() == self.source_unit
                && fact.end_span().source_id() == body.source_id()
                && body.start() <= fact.end_span().start() && fact.end_span().end() <= body.end()
                && matches!(fact.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == root)
                && self.typed.non_null_use(fact.argument()).is_some_and(|use_fact| {
                    use_fact.expression() == fact.argument()
                        && use_fact.symbol() == root && use_fact.declared_type() == declared
                        && use_fact.narrowed_type() == *narrowed
                })
        }) {
            return Ok(None);
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Ok(None);
        };
        if self.borrow_bindings.contains_key(&root) {
            return Ok(None);
        }
        let storage = self
            .type_ids
            .get(&declared)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let inner = self
            .type_ids
            .get(narrowed)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.ssa_types.get(storage.index())
            != Some(&SsaTypeKind::InlineNullable { inner, niche: true })
            || self.ssa_types.get(inner.index()) != Some(&SsaTypeKind::StringOwner)
            || self
                .function
                .entity(EntityId::Value(owner))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(storage))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_named_nullable_if_condition(
            condition,
            comparison,
            NamedNullableIfRoot {
                symbol: root,
                declared,
                narrowed: *narrowed,
            },
            span,
        )?;
        self.ensure_no_while_proof_carriers(span)?;
        Ok(Some(NullableIfEntry {
            root,
            owner,
            inner,
            scope: control,
            non_null_when_true: comparison.non_null_when_true(),
            consume: None,
        }))
    }
}
