//! Source-qualified entry identities for readonly, head-killed and distinct roots.
use super::*;

pub(in super::super) enum NullableWhileEntry {
    Readonly(UnitSymbolId, NullableProof),
    Direct(DirectNullableWhileEntry),
}

/// The inherited pair is constructed only after the complete distinct-root checks.
/// Consumers keep its origin, but must use each path's currently rebound pair.
pub(in super::super::super) struct DirectNullableWhileEntry {
    root: UnitSymbolId,
    scope: UnitStatementId,
    nullable: SsaTypeId,
    inner: SsaTypeId,
    inherited: Option<(UnitSymbolId, NullableProof)>,
}

impl DirectNullableWhileEntry {
    pub(in super::super::super) fn root(&self) -> UnitSymbolId {
        self.root
    }

    pub(in super::super::super) fn scope(&self) -> UnitStatementId {
        self.scope
    }

    pub(in super::super::super) fn inner(&self) -> SsaTypeId {
        self.inner
    }

    pub(super) fn nullable(&self) -> SsaTypeId {
        self.nullable
    }

    pub(in super::super::super) fn inherited(&self) -> Option<(UnitSymbolId, NullableProof)> {
        self.inherited
    }
}

impl UnitExpressionLowerer<'_> {
    /// No existing view is ended until every transparent operand layer qualifies.
    pub(in super::super) fn prepare_nullable_while_entry(
        &mut self,
        statement: StatementId,
        descriptor: UnitNullComparisonDescriptor,
        mut operand: UnitExpressionId,
        span: Span,
    ) -> Result<NullableWhileEntry, LoweringError> {
        let (nullable, inner) = self.direct_nullable_while_entry(descriptor, span)?;
        let root = descriptor.symbol();
        let mut entry = DirectNullableWhileEntry {
            root,
            scope: UnitStatementId::new(self.source_unit, statement),
            nullable,
            inner,
            inherited: None,
        };
        if self.nullable_proofs.is_empty() {
            self.ensure_no_while_proof_carriers(span)?;
            return Ok(NullableWhileEntry::Direct(entry));
        }
        let Some((&outer_root, &proof)) = self.nullable_proofs.first_key_value() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self.nullable_proofs.len() != 1
            || !self
                .loops
                .iter()
                .any(|context| context.proof_scope == Some(proof.scope()))
            || self.borrow_bindings.contains_key(&outer_root)
            || !matches!(
                self.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        let storage = descriptor.nullable_type();
        let declared = resolve_concrete_type(
            self.typed,
            storage,
            self.substitutions,
            self.static_self,
            span,
        )?;
        if self.typed.copyability(declared) != Copyability::MoveOnly
            || self.borrow_bindings.contains_key(&root)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(UnitTypeKind::Nullable(narrowed)) = self.typed.types().get(storage) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let narrowed = *narrowed;
        let concrete_inner = resolve_concrete_type(
            self.typed,
            narrowed,
            self.substitutions,
            self.static_self,
            span,
        )?;
        if self.type_ids.get(&concrete_inner) != Some(&inner)
            || (root == outer_root && inner != proof.inner)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if self.typed.copyability(concrete_inner) != Copyability::MoveOnly {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if operand.source_unit() != self.source_unit {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let operand_type = self
            .typed
            .expression_type(operand)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if operand_type != narrowed && operand_type != storage {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let readonly = root == outer_root && operand_type == narrowed;
        if root != outer_root && operand_type != storage {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.expression_type(operand) != Some(operand_type)
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            match (readonly, self.typed.non_null_use(operand)) {
                (true, Some(fact))
                    if fact.expression() == operand
                        && fact.symbol() == root
                        && fact.declared_type() == storage
                        && fact.narrowed_type() == narrowed => {}
                (false, None) => {}
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
            match node.payload() {
                Expression::Group { expression } => {
                    operand = UnitExpressionId::new(self.source_unit, *expression)
                }
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&root) {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        if readonly {
            self.ensure_readonly_while_carriers(root, span)?;
            return Ok(NullableWhileEntry::Readonly(root, proof));
        }
        self.ensure_no_while_proof_carriers(span)?;
        if root == outer_root {
            // The written same-root header kills the old view before rebirth.
            self.end_while_proof(root, span)?;
        } else {
            let Some(LoweredValue::Value(owner)) = self.bindings.get(&root) else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            if *owner == proof.owner || entry.scope == proof.scope() {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            entry.inherited = Some((outer_root, proof));
        }
        Ok(NullableWhileEntry::Direct(entry))
    }

    /// Check the surviving current pair, never reconstruct it from entry IDs.
    pub(in super::super::super) fn validate_readonly_while_exit(
        &self,
        root: UnitSymbolId,
        inherited: NullableProof,
        exit: &BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(proof) = exit.nullable_proofs.get(&root) else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if exit.nullable_proofs.len() != 1
            || proof.origin != inherited.origin
            || proof.inner != inherited.inner
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_nullable_proofs(&exit.bindings, &exit.nullable_proofs, span)
    }

    /// Validate an opaque qualification again at the currently rebound birth edge.
    pub(in super::super::super) fn validate_direct_while_entry(
        &self,
        entry: &DirectNullableWhileEntry,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some((root, inherited)) = entry.inherited else {
            return self.reject_nested_while_proof(span);
        };
        self.validate_readonly_while_exit(root, inherited, &self.loop_state(), span)?;
        let proof = self
            .nullable_proofs
            .get(&root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&entry.root) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if root == entry.root
            || proof.owner == *owner
            || proof.scope() == entry.scope
            || !self
                .loops
                .iter()
                .any(|context| context.proof_scope == Some(proof.scope()))
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.ensure_no_while_proof_carriers(span)
    }
}
