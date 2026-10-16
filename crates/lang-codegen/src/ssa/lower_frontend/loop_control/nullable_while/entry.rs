//! Qualification is the only way to retain an outer proof across a new direct guard.
use super::*;

pub(in super::super) enum NullableWhileEntry {
    Readonly(SymbolId, NonNullProof),
    Direct(DirectNullableWhileEntry),
}

/// The fields stay private: a caller cannot supply an arbitrary inherited pair.
pub(in super::super) struct DirectNullableWhileEntry {
    statement: StatementId,
    descriptor: NullComparisonDescriptor,
    inherited: Option<(SymbolId, NonNullProof)>,
    provider: Option<super::super::super::outer_borrowed_while::Relation>,
}

impl DirectNullableWhileEntry {
    pub(super) fn provider(&self) -> Option<&super::super::super::outer_borrowed_while::Relation> {
        self.provider.as_ref()
    }
    pub(super) fn into_parts(
        self,
        statement: StatementId,
        span: Span,
    ) -> Result<(NullComparisonDescriptor, Option<(SymbolId, NonNullProof)>), LoweringError> {
        if self.statement != statement {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok((self.descriptor, self.inherited))
    }
}

impl ExpressionLowerer<'_> {
    /// The real typed operand distinguishes an inherited read from a head kill.
    /// Qualify every transparent layer before retiring the current same-root view.
    fn prepare_nested_nullable_while(
        &mut self,
        descriptor: NullComparisonDescriptor,
        mut operand: ExpressionId,
        span: Span,
    ) -> Result<Option<(SymbolId, NonNullProof)>, LoweringError> {
        let root = descriptor.symbol();
        let Some(proof) = self.non_null_bindings.get(&root).copied() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if self.non_null_bindings.len() != 1 || !proof.is_while_origin() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_non_null_proof(root, proof, &self.bindings, span)?;
        let declared = self.resolve_type(descriptor.nullable_type(), span)?;
        let nullable = self
            .type_ids
            .get(&declared)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.copyability(declared) != Some(Copyability::MoveOnly)
            || self.borrow_bindings.contains_key(&root)
            || !matches!(
                self.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if self.ssa_types.get(nullable.index())
            != Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(nullable))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let storage = descriptor.nullable_type();
        let Some(TypeKind::Nullable(narrowed)) = self.typed.types().get(storage) else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let narrowed = *narrowed;
        let concrete_inner = self.resolve_type(narrowed, span)?;
        if self.type_ids.get(&concrete_inner) != Some(&proof.inner) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if self.typed.copyability(concrete_inner) != Some(Copyability::MoveOnly) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let operand_type = self
            .typed
            .expression_type(operand)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if operand_type != narrowed && operand_type != storage {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let inherited = operand_type == narrowed;
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.expression_type(operand) != Some(operand_type)
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            match (inherited, self.typed.non_null_use(operand)) {
                (true, Some(fact))
                    if fact.expression() == operand
                        && fact.symbol() == root
                        && fact.declared_type() == storage
                        && fact.narrowed_type() == narrowed => {}
                (false, None) => {}
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            }
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&root) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        if inherited {
            self.ensure_readonly_while_field_carriers(root, span)?;
            return Ok(Some((root, proof)));
        }
        // Retire only the old view, before the inner baseline/header can capture it.
        // The nullable owner remains intact for the direct guard's independent birth.
        self.direct_nullable_while_entry(descriptor, None, span)?;
        self.end_non_null_proof(root, span)?;
        Ok(None)
    }

    pub(in super::super) fn qualify_nullable_while_entry(
        &mut self,
        statement: StatementId,
        descriptor: NullComparisonDescriptor,
        operand: ExpressionId,
        span: Span,
    ) -> Result<NullableWhileEntry, LoweringError> {
        if let Some(provider) =
            self.qualify_outer_borrowed_entry(statement, descriptor, operand, span)?
        {
            return Ok(NullableWhileEntry::Direct(DirectNullableWhileEntry {
                statement,
                descriptor,
                inherited: None,
                provider: Some(provider),
            }));
        }
        let inherited = if self.non_null_bindings.is_empty() {
            None
        } else if self.non_null_bindings.contains_key(&descriptor.symbol()) {
            if let Some((root, proof)) =
                self.prepare_nested_nullable_while(descriptor, operand, span)?
            {
                return Ok(NullableWhileEntry::Readonly(root, proof));
            }
            None
        } else {
            Some(self.qualify_distinct_while_outer(statement, descriptor, operand, span)?)
        };
        Ok(NullableWhileEntry::Direct(DirectNullableWhileEntry {
            statement,
            descriptor,
            inherited,
            provider: None,
        }))
    }

    fn qualify_distinct_while_outer(
        &self,
        statement: StatementId,
        descriptor: NullComparisonDescriptor,
        mut operand: ExpressionId,
        span: Span,
    ) -> Result<(SymbolId, NonNullProof), LoweringError> {
        if self.non_null_bindings.len() != 1 {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let (&outer, &proof) = self
            .non_null_bindings
            .first_key_value()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some((scope, birth)) = proof.while_origin() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let root = descriptor.symbol();
        if root == outer
            || statement == scope
            || self.borrow_bindings.contains_key(&outer)
            || self.borrow_bindings.contains_key(&root)
            || !self.loops.iter().any(|context| {
                matches!(
                    self.function.block(context.header)
                        .and_then(|block| block.terminator.as_ref())
                        .map(|terminator| &terminator.kind),
                    Some(TerminatorKind::NullableBranch { view, .. }) if *view == birth
                )
            })
            || !matches!(
                self.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_non_null_proof(outer, proof, &self.bindings, span)?;
        let outer_storage = self
            .typed
            .symbol_type(outer)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let outer_type = self.resolve_type(outer_storage, span)?;
        let outer_nullable = self
            .type_ids
            .get(&outer_type)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.copyability(outer_type) != Some(Copyability::MoveOnly)
            || self.ssa_types.get(outer_nullable.index())
                != Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(outer_nullable))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // The new root qualifies independently; its payload need not match the outer's.
        let (_, inner) = self.direct_nullable_while_entry(descriptor, None, span)?;
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root) else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if *owner == proof.owner {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let storage = descriptor.nullable_type();
        let declared = self.resolve_type(storage, span)?;
        let Some(TypeKind::Nullable(narrowed)) = self.typed.types().get(storage) else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let narrowed = self.resolve_type(*narrowed, span)?;
        if self.typed.copyability(declared) != Some(Copyability::MoveOnly)
            || self.typed.copyability(narrowed) != Some(Copyability::MoveOnly)
            || self.type_ids.get(&narrowed) != Some(&inner)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.expression_type(operand) != Some(storage)
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
                || self.typed.non_null_use(operand).is_some()
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&root) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    return Ok((outer, proof));
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
    }
}
