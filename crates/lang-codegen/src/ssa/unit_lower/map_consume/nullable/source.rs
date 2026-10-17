//! Exact nullable storage/read/receiver-Move qualification, without changing live state.
use super::*;
use lang_frontend::{
    name_resolution::ScopeKind,
    parser::VariableKind,
    type_checking::{UnitMapConsumeDescriptor, UnitTypeKind},
};

pub(super) struct NullableMapConsumeSource {
    pub(super) descriptor: UnitMapConsumeDescriptor,
    pub(super) root: UnitSymbolId,
    pub(super) storage: UnitTypeId,
    pub(super) nullable: SsaTypeId,
    pub(super) inner: SsaTypeId,
    pub(super) result: SsaTypeId,
    // Runtime cleanup order: leaf Name, then each enclosing receiver Group.
    pub(super) operands: Vec<UnitExpressionId>,
}

impl UnitExpressionLowerer<'_> {
    pub(in crate::ssa::unit_lower::map_consume::nullable) fn qualify_nullable_map_consume(
        &self,
        call: UnitExpressionId,
        span: Span,
    ) -> Result<Option<NullableMapConsumeSource>, LoweringError> {
        let Some(descriptor) = self.typed.map_consume(call).copied() else {
            return Ok(None);
        };
        let (root, storage) = descriptor
            .source()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if storage == descriptor.receiver_type() {
            return Ok(None);
        }
        if call.source_unit() != self.source_unit
            || descriptor.expression() != call
            || descriptor.receiver().source_unit() != self.source_unit
            || root.source_unit() != self.source_unit
            || self.typed.symbol_type(root) != Some(storage)
            || self.typed.expression_type(descriptor.receiver()) != Some(descriptor.receiver_type())
            || self.typed.expression_type(call) != Some(descriptor.result_type())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(call.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call {
            callee,
            type_arguments,
            arguments,
            ..
        } = node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if !type_arguments.is_empty()
            || !arguments.is_empty()
            || !self
                .parsed
                .ast()
                .expressions()
                .get(*callee)
                .is_ok_and(|node| {
                    matches!(node.payload(), Expression::Member { receiver, safe: false, .. }
                    if *receiver == descriptor.receiver().expression())
                })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.qualify_nullable_map_consume_local(root, span)?;
        let fact = self
            .owned
            .receiver_fact(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if fact.call() != call
            || fact.source() != UnitCallReceiverOrigin::Expression(descriptor.receiver())
            || fact.receiver_type() != descriptor.receiver_type()
            || fact.kind() != UnitReceiverOwnershipKind::Move
            || !matches!(fact.target(), UnitReceiverOwnershipTarget::Place(place)
                if place.is_root() && place.root() == root)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let concrete =
            |ty| resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span);
        let declared = concrete(storage)?;
        let read = concrete(descriptor.receiver_type())?;
        let result_type = concrete(descriptor.result_type())?;
        let key = concrete(descriptor.key_type())?;
        let value = concrete(descriptor.value_type())?;
        if !matches!(self.typed.types().get(declared), Some(UnitTypeKind::Nullable(inner))
            if *inner == read)
            || self.typed.copyability(declared) != Copyability::MoveOnly
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        for (ty, constructor) in [
            (read, IntrinsicTypeConstructor::MutableMap),
            (result_type, IntrinsicTypeConstructor::Map),
        ] {
            if !matches!(self.typed.types().get(ty),
                Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && arguments.as_slice() == [key, value])
                || self.typed.copyability(ty) != Copyability::MoveOnly
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        let ssa = |ty| {
            self.type_ids
                .get(&ty)
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        };
        let nullable = ssa(declared)?;
        let inner = ssa(read)?;
        let result = ssa(result_type)?;
        let key = ssa(key)?;
        let value = ssa(value)?;
        if self.nullable_maps.get(&nullable) != Some(&inner)
            || self.expression_ssa_type(descriptor.receiver().expression(), span)? != inner
            || self.expression_ssa_type(call.expression(), span)? != result
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        for (ty, kind) in [
            (inner, MapContainerKind::MutableMap),
            (result, MapContainerKind::Map),
        ] {
            if self.ssa_types.get(ty.index())
                != Some(&SsaTypeKind::MapContainer { kind, key, value })
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        let mut operand = descriptor.receiver().expression();
        let mut operands = Vec::new();
        loop {
            let id = UnitExpressionId::new(self.source_unit, operand);
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let fact = self
                .typed
                .non_null_use(id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if fact.expression() != id
                || fact.symbol() != root
                || fact.declared_type() != storage
                || fact.narrowed_type() != descriptor.receiver_type()
                || self.typed.expression_type(id) != Some(descriptor.receiver_type())
                || self.typed.expression_category(id) != Some(ExpressionCategory::Place)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            operands.push(id);
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name if self.references.get(&span_key(node.span())) == Some(&root) => {
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
        }
        operands.reverse();
        Ok(Some(NullableMapConsumeSource {
            descriptor,
            root,
            storage,
            nullable,
            inner,
            result,
            operands,
        }))
    }

    fn qualify_nullable_map_consume_local(
        &self,
        root: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let source = self
            .names
            .names()
            .source_units()
            .get(self.source_unit.index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let resolution = source.resolution();
        let symbol = resolution
            .symbols()
            .get(root.symbol().index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let key = self
            .source_plan
            .source(self.source_token)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let UnitCallableTarget::Declaration(target) = key.target() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let declaration = self
            .names
            .names()
            .index()
            .declarations()
            .get(target.index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (_, item, function_span) = unwrap_modified(self.parsed, declaration.root())?;
        if self.function_ids.get(key) != Some(&self.function.id)
            || declaration.source_unit() != self.source_unit
            || source.source_unit() != self.source_unit
            || resolution.source_id() != self.parsed.source_id()
            || !matches!(item, Item::Function { .. })
            || !self.owned.formed_owned_locals().contains(&root)
            || symbol.id() != root.symbol()
            || symbol.kind() != SymbolKind::Variable
            || symbol.is_synthetic()
            || symbol.name() == "_"
            || symbol.span().source_id() != self.parsed.source_id()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let mut declarations = 0;
        for (_, statement) in self.parsed.ast().statements().iter() {
            let Statement::LocalVariable { declaration } = statement.payload() else {
                continue;
            };
            let (_, item, _) = unwrap_modified(self.parsed, *declaration)?;
            if let Item::Variable {
                kind,
                name: NameMarker::Present(name),
                ..
            } = item
                && name == symbol.span()
            {
                if !matches!(kind, VariableKind::Val | VariableKind::Var) {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                declarations += 1;
            }
        }
        if declarations != 1 {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // Formed history only proves a local declaration existed. Receiver Move above
        // and the live owner/view checks remain independent mandatory qualifications.
        let mut scope = symbol.scope();
        for _ in 0..resolution.scopes().len() {
            let data = resolution
                .scopes()
                .get(scope.index())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match data.kind() {
                ScopeKind::Function if data.span() == Some(function_span) => return Ok(()),
                ScopeKind::Function
                | ScopeKind::Lambda
                | ScopeKind::File
                | ScopeKind::Classifier
                | ScopeKind::Companion => {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                _ => {
                    scope = data
                        .parent()
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
                }
            }
        }
        Err(lowering_error(LoweringErrorKind::MissingFact, span))
    }
}
