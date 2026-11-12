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
        ) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if self.function.borrow_return.is_some() || self.function.carrier_return.is_some() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let concrete =
            |ty| resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span);
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
            || !matches!(
                self.ssa_types.get(inner.index()),
                Some(SsaTypeKind::HeapOwner { .. } | SsaTypeKind::SharedOwner { .. })
            )
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
            != descriptor
                .entries()
                .iter()
                .any(|entry| entry.falls_through())
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
        if !matches!(
            condition_node.payload(),
            Expression::Literal(LiteralKind::Null)
        ) || alternative.expression() != UnitExpressionId::new(self.source_unit, *condition)
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
        let leaf = *operands
            .last()
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
                    .is_ok_and(|node| matches!(node.payload(), Expression::Name))
                {
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
                    || plan
                        .source_place()
                        .is_none_or(|path| !path.is_root() || path.root() != root)
                    || self.direct_name_symbol(leaf.expression(), span)? != Some(root)
                    || self.typed.symbol_type(root) != Some(descriptor.nullable_type())
                    || operands.iter().any(|operand| {
                        self.typed.expression_category(*operand) != Some(ExpressionCategory::Place)
                    })
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
                    || self
                        .typed
                        .nullable_when_uses()
                        .iter()
                        .any(|fact| fact.control() == control)
                    || operands.iter().any(|operand| {
                        self.typed.expression_category(*operand)
                            != Some(ExpressionCategory::Temporary)
                    })
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
            let normal_drops = self
                .owned
                .drops()
                .iter()
                .filter(|fact| {
                    fact.point() == UnitDropPoint::AfterExpression(control)
                        && fact.target() == UnitDropTarget::Temporary(descriptor.subject())
                })
                .count();
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
        if !matches!(
            symbol.kind(),
            SymbolKind::Variable | SymbolKind::ValueParameter
        ) || symbol.is_synthetic()
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
                if self
                    .temporaries
                    .get(operand)
                    .is_some_and(|value| *value != owner)
                {
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

    pub(in super::super) fn nullable_when_group_chain(
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
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(current.expression())
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
            || descriptor
                .entries()
                .get(scope.entry)
                .map(|entry| entry.body())
                != Some(witness.body())
            || witness.nullable_type() != descriptor.nullable_type()
            || witness.inner_type() != descriptor.inner_type()
            || operands.last() != Some(&witness.reference())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        for operand in &operands {
            let fact = self
                .typed
                .nullable_when_use(*operand)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let ordinary = self
                .typed
                .non_null_use(*operand)
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
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(witness.reference().expression())
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
        if !self
            .nullable_proofs
            .values()
            .any(|proof| proof.when_scope().is_some())
        {
            return Ok(());
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression.expression())
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
            || self
                .typed
                .null_comparison(expression)
                .is_some_and(|comparison| {
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
            Expression::Name => self
                .references
                .get(&span_key(node.span()))
                .is_some_and(|root| {
                    self.nullable_proofs
                        .get(root)
                        .is_some_and(|proof| proof.when_scope().is_some())
                }),
            Expression::Group { expression } => self.references_nullable_when_root(*expression),
            Expression::Member { receiver, .. } | Expression::Index { receiver, .. } => {
                self.references_nullable_when_root(*receiver)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod frontend_registration_tests {
    //! CI-only mapper contract: real FE identities, no descriptor pre-scan or SSA/native claim.

    use std::collections::BTreeSet;

    use lang_frontend::{
        lexer::lex,
        name_resolution::{SymbolKind, resolve_names},
        ownership_checking::check_ownership,
        parser::parse_file,
        type_checking::{
            IntrinsicTypeConstructor, TypeKind, check_types, complete_file_types, standard_environments,
        },
    };

    use crate::ssa::lower_frontend::orchestrate::*;
    use crate::ssa::lower_frontend::*;
    use crate::ssa::model::{MapContainerKind, Module, Ownership, SsaTypeId, SsaTypeKind};

    use crate::ssa::nullable_map_consume_lowering_tests::registration_r1_source;

    const ORDINARY: &str = r#"fun main(): Unit {
      val mapText: Map<Int, String>? = null
      var mapTextOuter = mutableMapOf<Int, Map<Int, String>>()
      val mapTextRemoved = mapTextOuter.remove(1)
      val mutableText: MutableMap<Int, String>? = null
      var mutableTextOuter = mutableMapOf<Int, MutableMap<Int, String>>()
      val mutableTextRemoved = mutableTextOuter.remove(1)
      val mutableKey: MutableMap<String, Int>? = null
      var mutableKeyOuter = mutableMapOf<Int, MutableMap<String, Int>>()
      val mutableKeyRemoved = mutableKeyOuter.remove(1)
    }
    "#;

    #[derive(Clone, Copy)]
    struct Case {
        local: &'static str,
        outer: &'static str,
        constructor: IntrinsicTypeConstructor,
        kind: MapContainerKind,
        key: BuiltinType,
        value: Option<BuiltinType>,
    }

    const CASES: &[Case] = &[
        Case {
            local: "mapText",
            outer: "mapTextOuter",
            constructor: IntrinsicTypeConstructor::Map,
            kind: MapContainerKind::Map,
            key: BuiltinType::Int,
            value: Some(BuiltinType::String),
        },
        Case {
            local: "mutableText",
            outer: "mutableTextOuter",
            constructor: IntrinsicTypeConstructor::MutableMap,
            kind: MapContainerKind::MutableMap,
            key: BuiltinType::Int,
            value: Some(BuiltinType::String),
        },
        Case {
            local: "mutableKey",
            outer: "mutableKeyOuter",
            constructor: IntrinsicTypeConstructor::MutableMap,
            kind: MapContainerKind::MutableMap,
            key: BuiltinType::String,
            value: Some(BuiltinType::Int),
        },
    ];

    const RESOURCE: Case = Case {
        local: "ordinary",
        outer: "outer",
        constructor: IntrinsicTypeConstructor::MutableMap,
        kind: MapContainerKind::MutableMap,
        key: BuiltinType::Int,
        value: None,
    };

    #[derive(Debug, PartialEq, Eq)]
    struct Snapshot {
        id: SsaTypeId,
        types: usize,
        nullable: BTreeMap<SsaTypeId, SsaTypeId>,
        results: BTreeMap<SsaTypeId, SsaTypeId>,
    }

    fn snapshot(module: &Module, id: SsaTypeId) -> Snapshot {
        Snapshot {
            id,
            types: module.types.len(),
            nullable: module.nullable_maps.clone(),
            results: module.map_results.clone(),
        }
    }

    fn builtin_identity(module: &Module, id: SsaTypeId, expected: BuiltinType) -> String {
        match expected {
            BuiltinType::Int => assert!(matches!(
                module.type_kind(id),
                Some(SsaTypeKind::Integer {
                    bits: 32,
                    signed: true
                })
            )),
            BuiltinType::String => assert_eq!(module.type_kind(id), Some(&SsaTypeKind::StringOwner)),
            _ => panic!("fixture contains only Int/String builtins"),
        }
        format!("{expected:?}")
    }

    fn storage_identity(module: &Module, wrapper: SsaTypeId, case: Case) -> String {
        let inner = module
            .nullable_map_inner(wrapper)
            .expect("N wrapper exists");
        let (kind, key, value) = module
            .map_container(inner)
            .expect("exact MapContainer payload");
        assert_eq!(kind, case.kind);
        let key_name = builtin_identity(module, key, case.key);
        let value_name = match case.value {
            Some(builtin) => builtin_identity(module, value, builtin),
            None => {
                assert!(matches!(
                    module.type_kind(value),
                    Some(SsaTypeKind::HeapOwner { .. })
                ));
                assert!(
                    module.deinits.contains_key(&value),
                    "real Resource deinit is registered"
                );
                "Resource".to_owned()
            }
        };
        assert_eq!(module.type_ownership(inner), Some(Ownership::MoveOnly));
        assert_eq!(module.type_ownership(wrapper), Some(Ownership::MoveOnly));
        let [present, payload] = module
            .aggregate_fields(wrapper)
            .expect("presence aggregate")
        else {
            panic!("nullable Map has exactly two fields");
        };
        assert_eq!(module.type_kind(*present), Some(&SsaTypeKind::Boolean));
        assert_eq!(*payload, inner);
        assert_eq!(
            module
                .nullable_maps
                .values()
                .filter(|&&ty| ty == inner)
                .count(),
            1
        );
        assert_eq!(module.map_result_value(wrapper), None);
        assert!(
            module
                .map_results
                .keys()
                .all(|ty| !module.nullable_maps.contains_key(ty))
        );
        assert!(
            module
                .map_results
                .values()
                .all(|&ty| module.map_container(ty).is_none())
        );
        format!("{kind:?}<{key_name},{value_name}>?")
    }

    fn observe(text: &str, path: &str, cases: &[Case], ordinary_first: bool) -> Vec<String> {
        let mut sources = SourceMap::new();
        let source = sources.add_source(path, text).expect("unique source");
        let lexed = lex(&sources, source).expect("lex");
        assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
        let parsed = parse_file(&sources, &lexed).expect("parse");
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &environment).expect("names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let typed = check_types(&sources, &parsed, &names, &types).expect("types");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let complete = complete_file_types(&sources, &parsed, &names, &types, &typed)
            .expect("complete original type analysis");
        assert!(complete.is_same_analysis(&typed));
        assert!(std::ptr::eq(complete.types(), &typed));
        assert!(complete.types().diagnostics().is_empty());
        let owned = check_ownership(&sources, &parsed, &names, complete.types()).expect("ownership");
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        validate_inputs(&sources, &parsed, &names, &typed, &owned).expect("ordinary input gate");

        let declarations = collect_functions(&parsed, &names, &typed).expect("real declarations");
        let templates = declarations
            .iter()
            .map(|declaration| FunctionTemplate {
                symbol: declaration.symbol,
                type_parameters: declaration.callable.type_parameters().to_vec(),
                span: declaration.span,
            })
            .collect::<Vec<_>>();
        let mut program = Program::default();
        let module_id = program.add_module("registration");
        let module = program.module_mut(module_id).expect("fresh module");
        let mut mapper = NominalTypeMapper::new(&typed).expect("fresh mapper");
        let deinits = resource_deinit::declare(module, &mut mapper, &parsed, &names, &typed)
            .expect("full deinit declaration");
        let deinit_spans = deinits.iter().map(|plan| plan.span).collect::<Vec<_>>();
        let instances = plan_instances(&parsed, &typed, &owned, &templates, &deinit_spans)
            .expect("full instance planning");
        assert!(instances.instances().iter().any(|instance| {
            names
                .symbols()
                .iter()
                .any(|symbol| symbol.id() == instance.key.symbol() && symbol.name() == "main")
        }));
        let resource = cases.iter().any(|case| case.value.is_none());
        assert_eq!(deinits.len(), usize::from(resource));
        assert_eq!(module.deinits.len(), deinits.len());
        for plan in &deinits {
            let function = module
                .function(plan.id)
                .expect("declared real deinit function");
            let Some(EntityType::Loan {
                kind: crate::ssa::model::LoanKind::Shared,
                target,
            }) = function.receiver()
            else {
                panic!("deinit has a Borrow receiver")
            };
            assert_eq!(module.deinits.get(&target), Some(&plan.id));
            assert_eq!(
                typed.is_resource_type(plan.descriptor.receiver_type()),
                Some(true)
            );
        }

        // No map-result descriptor pre-scan. Assert preparation did not register any target.
        assert!(module.nullable_maps.is_empty());
        assert!(module.map_results.is_empty());
        assert_eq!(typed.map_removes().len(), cases.len());
        let mut frontend_inputs = Vec::new();
        for &case in cases {
            let named = |name: &str| {
                let matches = names
                    .symbols()
                    .iter()
                    .filter(|symbol| {
                        symbol.name() == name
                            && symbol.kind() == SymbolKind::Variable
                            && !symbol.is_synthetic()
                            && symbol.span().source_id() == source
                    })
                    .collect::<Vec<_>>();
                assert_eq!(matches.len(), 1, "unique real local {name}");
                (
                    typed.symbol_type(matches[0].id()).expect("symbol type"),
                    matches[0].span(),
                )
            };
            let (nullable, local_span) = named(case.local);
            let Some(TypeKind::Nullable(inner)) = typed.types().get(nullable) else {
                panic!("real ordinary local is nullable");
            };
            let Some(TypeKind::Intrinsic {
                constructor,
                arguments,
            }) = typed.types().get(*inner)
            else {
                panic!("real nullable inner is intrinsic");
            };
            assert_eq!(*constructor, case.constructor);
            let [key, value] = arguments.as_slice() else {
                panic!("exact two Map arguments")
            };
            assert_eq!(Some(*key), typed.types().builtin(case.key));
            if let Some(builtin) = case.value {
                assert_eq!(Some(*value), typed.types().builtin(builtin));
            } else {
                assert_eq!(
                    Some(*value),
                    deinits.first().map(|plan| plan.descriptor.receiver_type())
                );
            }
            let call = format!("{}.remove(1)", case.outer);
            let removes = typed
                .map_removes()
                .iter()
                .filter(|remove| {
                    let span = parsed
                        .ast()
                        .expressions()
                        .get(remove.expression())
                        .expect("real remove")
                        .span();
                    sources.slice(span).expect("remove source") == call
                })
                .collect::<Vec<_>>();
            assert_eq!(removes.len(), 1, "unique real remove {call}");
            let remove = removes[0];
            let remove_span = parsed
                .ast()
                .expressions()
                .get(remove.expression())
                .expect("remove")
                .span();
            assert_eq!(remove.result_type(), nullable);
            assert_eq!(remove.value_type(), *inner);
            assert_eq!(typed.expression_type(remove.expression()), Some(nullable));
            assert_eq!(
                Some(remove.key_type()),
                typed.types().builtin(BuiltinType::Int)
            );
            let (outer, _) = named(case.outer);
            assert_eq!(typed.expression_type(remove.receiver()), Some(outer));
            assert!(matches!(typed.types().get(outer),
                Some(TypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::MutableMap, arguments })
                    if arguments.as_slice() == [remove.key_type(), *inner]));
            assert!(
                !mapper.contains_type(nullable),
                "preparation must not warm nullable cache"
            );
            assert!(
                !mapper.contains_type(*inner),
                "preparation must not warm Map cache"
            );
            frontend_inputs.push((case, nullable, *inner, local_span, remove_span));
        }
        assert_eq!(
            frontend_inputs
                .iter()
                .map(|input| input.1)
                .collect::<BTreeSet<_>>()
                .len(),
            cases.len()
        );
        let mut wrappers = BTreeSet::new();
        let mut cache_pairs = Vec::new();
        let mut summaries = Vec::new();
        for (case, nullable, inner, local_span, remove_span) in frontend_inputs {
            assert!(!mapper.contains_type(nullable));
            assert!(!mapper.contains_type(inner));
            let first = if ordinary_first {
                mapper.intern(module, &names, &typed, nullable, local_span)
            } else {
                mapper.intern_map_result(module, &names, &typed, nullable, remove_span)
            }
            .expect("first real mapper registration");
            let after_first = snapshot(module, first);
            cache_pairs.push((nullable, first));
            cache_pairs.push((
                inner,
                module.nullable_map_inner(first).expect("exact inner"),
            ));
            let semantic = storage_identity(module, first, case);
            let second = if ordinary_first {
                mapper.intern_map_result(module, &names, &typed, nullable, remove_span)
            } else {
                mapper.intern(module, &names, &typed, nullable, local_span)
            }
            .expect("second real mapper registration");
            assert_eq!(
                after_first,
                snapshot(module, second),
                "same mapper exact identity/N/R/type count"
            );
            assert_eq!(semantic, storage_identity(module, second, case));
            assert!(
                wrappers.insert(first),
                "different kind or K/V must have different wrappers"
            );
            assert_eq!(module.nullable_maps.len(), wrappers.len());
            summaries.push(format!(
                "{path}:{}:{}..{}:{semantic}:{call}",
                case.local,
                local_span.start(),
                local_span.end(),
                call = sources.slice(remove_span).expect("source")
            ));
        }
        let (mapped, _, _) = mapper.into_parts();
        for (frontend, storage) in cache_pairs {
            assert_eq!(
                mapped.get(&frontend),
                Some(&storage),
                "exact frontend cache identity"
            );
        }
        summaries
    }

    #[test]
    fn nullable_map_registration_order_preserves_identity_and_resource_deinit() {
        // Each observation owns a fresh FE analysis, Program and mapper. Never compare bare IDs across them.
        assert_eq!(
            observe(ORDINARY, "ordinary.ko", CASES, true),
            observe(ORDINARY, "ordinary.ko", CASES, false)
        );
        let (path, single, provider, consumer) = registration_r1_source();
        assert_eq!(path, "r1_ordinary_source_before_remove.ko");
        assert_eq!(provider, format!("package p\n{single}"));
        assert_eq!(consumer, "package q\nfun entry(): Unit { p.main() }\n");
        assert_eq!(
            observe(single, path, &[RESOURCE], true),
            observe(single, path, &[RESOURCE], false)
        );
    }
}
