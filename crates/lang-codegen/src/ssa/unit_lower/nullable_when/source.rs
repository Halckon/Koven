//! Same-analysis descriptor admission, one subject evaluation and exact use identity.
use super::*;
use lang_frontend::type_checking::{IntrinsicTypeConstructor, NominalKind};

impl UnitExpressionLowerer<'_> {
    /// Type information is already published. Syntax only distinguishes the bounded
    /// null/else descriptor contract from the still unsupported general pattern path.
    pub(in super::super) fn missing_nullable_when_descriptor(
        &self,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let bounded = if let [first, second] = entries {
            first.else_span.is_none()
                && second.else_span.is_some()
                && second.conditions.is_empty()
                && matches!(first.conditions.as_slice(), [WhenCondition::Expression(condition)]
                    if self.parsed.ast().expressions().get(*condition).is_ok_and(|node|
                        matches!(node.payload(), Expression::Literal(LiteralKind::Null))))
        } else {
            false
        };
        Err(lowering_error(
            if bounded {
                LoweringErrorKind::MissingFact
            } else {
                LoweringErrorKind::UnsupportedNode
            },
            span,
        ))
    }

    pub(super) fn prepare_nullable_when_source(
        &self,
        descriptor: &UnitNullableWhenDescriptor,
        span: Span,
    ) -> Result<NullableWhenSource, LoweringError> {
        use UnitNullableWhenDomain::{Empty, NonNull, Null, Nullable};
        if !matches!(
            descriptor.source_category(),
            NullableWhenSubjectCategory::OwnedRoot | NullableWhenSubjectCategory::Temporary
        )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if self.function.borrow_return.is_some() || self.function.carrier_return.is_some() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let concrete = |ty| {
            resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)
        };
        let declared = concrete(descriptor.nullable_type())?;
        let narrowed = concrete(descriptor.inner_type())?;
        let supported = match self.typed.types().get(narrowed) {
            Some(UnitTypeKind::Nominal { declaration, .. }) => self
                .typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .is_some_and(|nominal| nominal.kind() == NominalKind::Class),
            Some(UnitTypeKind::Intrinsic { constructor, .. }) => matches!(
                constructor,
                IntrinsicTypeConstructor::Box | IntrinsicTypeConstructor::Rc
            ),
            _ => false,
        };
        if !supported
            || self.typed.copyability(declared) != Copyability::MoveOnly
            || self.typed.copyability(narrowed) != Copyability::MoveOnly
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let nullable = self
            .type_ids
            .get(&declared)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let inner = self
            .type_ids
            .get(&narrowed)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(self.typed.types().get(declared), Some(UnitTypeKind::Nullable(actual)) if *actual == narrowed)
            || self.ssa_types.get(nullable.index()) != Some(&SsaTypeKind::NullableHandle { inner })
            || !matches!(self.ssa_types.get(inner.index()),
                Some(SsaTypeKind::HeapOwner { .. } | SsaTypeKind::SharedOwner { .. }))
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let control = descriptor.expression();
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(control.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::When {
            subject: Some(subject),
            entries,
            ..
        } = node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let plan = self
            .owned
            .nullable_when(control)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if control.source_unit() != self.source_unit
            || descriptor.subject() != UnitExpressionId::new(self.source_unit, *subject)
            || self.typed.expression_type(descriptor.subject()) != Some(descriptor.nullable_type())
            || plan.descriptor() != descriptor
            || entries.len() != 2
            || descriptor.entries().len() != 2
            || plan.entries().len() != 2
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        for (index, ((actual, entry), owned)) in entries
            .iter()
            .zip(descriptor.entries())
            .zip(plan.entries())
            .enumerate()
        {
            if entry.index() != index
                || owned.index() != index
                || entry.body() != UnitStatementId::new(self.source_unit, actual.body)
                || owned.body() != entry.body()
                || entry.is_else() != actual.else_span.is_some()
                || entry.is_else() != (index == 1)
                || entry.span() != actual.span
                || entry.falls_through() != owned.falls_through()
                || entry.input_domain() != if index == 0 { Nullable } else { NonNull }
                || entry.body_domain() != if index == 0 { Null } else { NonNull }
                || entry.remaining_domain() != if index == 0 { NonNull } else { Empty }
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, entry.span()));
            }
        }
        if descriptor.falls_through()
            != descriptor.entries().iter().any(|entry| entry.falls_through())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let [WhenCondition::Expression(condition)] = entries[0].conditions.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let [alternative] = descriptor.entries()[0].alternatives() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let condition_node = self
            .parsed
            .ast()
            .expressions()
            .get(*condition)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(condition_node.payload(), Expression::Literal(LiteralKind::Null))
            || alternative.expression() != UnitExpressionId::new(self.source_unit, *condition)
            || alternative.span() != condition_node.span()
            || alternative.input_domain() != Nullable
            || alternative.matched_domain() != Null
            || alternative.remaining_domain() != NonNull
            || !descriptor.entries()[1].alternatives().is_empty()
            || !entries[1].conditions.is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let operands = self.nullable_when_group_chain(descriptor.subject(), span)?;
        let leaf = *operands.last()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        for operand in &operands {
            if self.typed.expression_type(*operand) != Some(descriptor.nullable_type()) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        let root = match descriptor.source_category() {
            NullableWhenSubjectCategory::OwnedRoot => {
                if !self
                    .parsed
                    .ast()
                    .expressions()
                    .get(leaf.expression())
                    .is_ok_and(|node| matches!(node.payload(), Expression::Name)) {
                    // An owned This has no stable Name reference; it is not a
                    // temporary and does not gain this whole-root capability.
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                let Some(root) = descriptor.stable_symbol() else {
                    // Unstable/captured roots have no subject FlowKey. The absence
                    // is a known capability boundary, not permission to invent one.
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                };
                self.validate_nullable_when_owned_root(root, span)?;
                if descriptor.subject_reference() != Some(leaf)
                    || descriptor
                        .source_generation()
                        .is_none_or(|generation| generation == u64::MAX)
                    || plan.source_place().is_none_or(|path| !path.is_root() || path.root() != root)
                    || self.direct_name_symbol(leaf.expression(), span)? != Some(root)
                    || self.typed.symbol_type(root) != Some(descriptor.nullable_type())
                    || operands.iter().any(|operand| self.typed.expression_category(*operand)
                        != Some(ExpressionCategory::Place))
                    || self.borrow_bindings.contains_key(&root)
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                if self.nullable_proofs.contains_key(&root) {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                Some(root)
            }
            NullableWhenSubjectCategory::Temporary => {
                if descriptor.stable_symbol().is_some()
                    || descriptor.subject_reference().is_some()
                    || descriptor.source_generation().is_some()
                    || plan.source_place().is_some()
                    || !plan.uses().is_empty()
                    || self.typed.nullable_when_uses().iter().any(|fact| fact.control() == control)
                    || operands.iter().any(|operand| self.typed.expression_category(*operand)
                        != Some(ExpressionCategory::Temporary))
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                None
            }
            _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
        };
        // A temporary has exactly the complete-expression obligation, not an early
        // subject-chain obligation that would make the body observe a dead owner.
        if root.is_none() && self.owned.drops().iter().any(|fact| {
            matches!(fact.point(), UnitDropPoint::AfterExpression(point) if operands.contains(&point))
                && matches!(fact.target(), UnitDropTarget::Temporary(target) if operands.contains(&target))
        }) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if root.is_none() {
            let normal_drops = self.owned.drops().iter().filter(|fact| {
                fact.point() == UnitDropPoint::AfterExpression(control)
                    && fact.target() == UnitDropTarget::Temporary(descriptor.subject())
            }).count();
            if normal_drops != usize::from(descriptor.falls_through()) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(NullableWhenSource {
            operands,
            root,
            inner,
            nullable,
        })
    }

    fn validate_nullable_when_owned_root(
        &self,
        root: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        use lang_frontend::ownership_checking::OwnershipBindingKind;
        let symbol = self
            .names
            .names()
            .source_units()
            .get(root.source_unit().index())
            .and_then(|source| source.resolution().symbols().get(root.symbol().index()))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(symbol.kind(), SymbolKind::Variable | SymbolKind::ValueParameter)
            || symbol.is_synthetic()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owned = match self.owned.binding_kind(root) {
            Some(OwnershipBindingKind::Owned) => true,
            Some(OwnershipBindingKind::Shared | OwnershipBindingKind::Exclusive) => false,
            None => {
                symbol.kind() == SymbolKind::Variable
                    && self.owned.formed_owned_locals().contains(&root)
            }
        };
        if !owned {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn evaluate_nullable_when_subject(
        &mut self,
        descriptor: &UnitNullableWhenDescriptor,
        source: &NullableWhenSource,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let leaf = source
            .operands
            .last()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let value = self.lower_expression(leaf.expression())?;
        let LoweredValue::Value(owner) = value else {
            return Ok(value);
        };
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(source.nullable))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if let Some(root) = source.root {
            if self.bindings.get(&root) != Some(&value) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        } else {
            for operand in &source.operands {
                if self.temporaries.get(operand).is_some_and(|value| *value != owner) {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
            for operand in &source.operands {
                self.temporaries.insert(*operand, owner);
            }
            if self.temporaries.get(&descriptor.subject()) != Some(&owner) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(value)
    }

    pub(super) fn nullable_when_group_chain(
        &self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<Vec<UnitExpressionId>, LoweringError> {
        let mut operands = Vec::new();
        let mut current = expression;
        loop {
            if current.source_unit() != self.source_unit {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            operands.push(current);
            let node = self.parsed.ast().expressions().get(current.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => {
                    current = UnitExpressionId::new(self.source_unit, *expression);
                }
                _ => return Ok(operands),
            }
        }
    }

    pub(super) fn validate_nullable_when_use(
        &self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<
        (
            UnitNullableWhenUseWitness,
            nullable_proof::NullableProof,
            Vec<UnitExpressionId>,
        ),
        LoweringError,
    > {
        let witness = self
            .typed
            .nullable_when_use(expression)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let proof = self
            .nullable_proofs
            .get(&witness.symbol())
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let scope = proof
            .when_scope()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let descriptor = self
            .typed
            .nullable_when(scope.control)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        let operands = self.nullable_when_group_chain(expression, span)?;
        if witness.expression() != expression
            || witness.control() != scope.control
            || witness.entry_index() != scope.entry
            || witness.source_generation() != scope.generation
            || witness.subject() != descriptor.subject()
            || descriptor.entries().get(scope.entry).map(|entry| entry.body()) != Some(witness.body())
            || witness.nullable_type() != descriptor.nullable_type()
            || witness.inner_type() != descriptor.inner_type()
            || operands.last() != Some(&witness.reference())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        for operand in &operands {
            let fact = self.typed.nullable_when_use(*operand)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let ordinary = self.typed.non_null_use(*operand)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if fact.expression() != *operand
                || fact.reference() != witness.reference()
                || fact.control() != witness.control()
                || fact.subject() != witness.subject()
                || fact.entry_index() != witness.entry_index()
                || fact.body() != witness.body()
                || fact.symbol() != witness.symbol()
                || fact.source_generation() != witness.source_generation()
                || fact.nullable_type() != witness.nullable_type()
                || fact.inner_type() != witness.inner_type()
                || ordinary.expression() != *operand
                || ordinary.symbol() != witness.symbol()
                || ordinary.declared_type() != witness.nullable_type()
                || ordinary.narrowed_type() != witness.inner_type()
                || self.typed.expression_type(*operand) != Some(witness.inner_type())
                || self.typed.expression_category(*operand) != Some(ExpressionCategory::Place)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        let node = self.parsed.ast().expressions().get(witness.reference().expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(node.payload(), Expression::Name)
            || self.references.get(&span_key(node.span())) != Some(&witness.symbol())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok((witness, proof, operands))
    }

    /// Only the two dedicated argument consumers may use a When view. All older
    /// projection/receiver fast paths are stopped before they can bypass a Name.
    pub(in super::super) fn guard_nullable_when_expression(
        &self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.reject_nullable_when_value(expression, span)?;
        if !self.nullable_proofs.values().any(|proof| proof.when_scope().is_some()) {
            return Ok(());
        }
        let node = self.parsed.ast().expressions().get(expression.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let receiver = match node.payload() {
            Expression::Member { receiver, .. } | Expression::Index { receiver, .. } => {
                Some(*receiver)
            }
            Expression::Assignment { target, .. } => Some(*target),
            Expression::NonNullAssert { operand, .. } => Some(*operand),
            Expression::Call { callee, .. } => Some(*callee),
            Expression::Lambda { .. } => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            _ => None,
        };
        if receiver.is_some_and(|receiver| self.references_nullable_when_root(receiver))
            || self.typed.null_comparison(expression).is_some_and(|comparison| {
                self.nullable_proofs
                    .get(&comparison.symbol())
                    .is_some_and(|proof| proof.when_scope().is_some())
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    pub(in super::super) fn reject_nullable_when_value(
        &self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.typed.nullable_when_use(expression).is_some() {
            self.validate_nullable_when_use(expression, span)?;
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if self.typed.non_null_use(expression).is_some_and(|fact| {
            self.nullable_proofs
                .get(&fact.symbol())
                .is_some_and(|proof| proof.when_scope().is_some())
        }) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    fn references_nullable_when_root(&self, expression: ExpressionId) -> bool {
        let Ok(node) = self.parsed.ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::Name => self.references.get(&span_key(node.span())).is_some_and(|root| {
                self.nullable_proofs.get(root).is_some_and(|proof| proof.when_scope().is_some())
            }),
            Expression::Group { expression } => self.references_nullable_when_root(*expression),
            Expression::Member { receiver, .. } | Expression::Index { receiver, .. } => {
                self.references_nullable_when_root(*receiver)
            }
            _ => false,
        }
    }
}
