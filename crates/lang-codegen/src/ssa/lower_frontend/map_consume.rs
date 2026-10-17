//! The checked owned-local consume path; ordinary Map borrowing stays separate.
mod nullable;

use super::{nominal::NominalTypeMapper, *};
use crate::ssa::model::{MapContainerKind, Module, SsaTypeKind};
use lang_frontend::type_checking::IntrinsicTypeConstructor;

impl ExpressionLowerer<'_> {
    pub(super) fn lower_map_consume(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        let descriptor = self
            .typed
            .map_consume(expression)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (root, storage) = descriptor
            .source()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.expression() != expression
            || self.typed.expression_type(descriptor.receiver()) != Some(descriptor.receiver_type())
            || self.typed.expression_type(expression) != Some(descriptor.result_type())
            || self.typed.symbol_type(root) != Some(storage)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_map_consume_root(descriptor.receiver(), root, span)?;
        let receiver_type = self.resolve_type(descriptor.receiver_type(), span)?;
        let result_type = self.resolve_type(descriptor.result_type(), span)?;
        let key = self.resolve_type(descriptor.key_type(), span)?;
        let val = self.resolve_type(descriptor.value_type(), span)?;
        for (ty, constructor) in [
            (receiver_type, IntrinsicTypeConstructor::MutableMap),
            (result_type, IntrinsicTypeConstructor::Map),
        ] {
            if !matches!(self.typed.types().get(ty),
                Some(TypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && arguments.as_slice() == [key, val])
                || self.typed.copyability(ty) != Some(Copyability::MoveOnly)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        let source_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let map_type = self.expression_ssa_type(expression, span)?;
        let key = self
            .type_ids
            .get(&key)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let val = self
            .type_ids
            .get(&val)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        for (ty, kind) in [
            (source_type, MapContainerKind::MutableMap),
            (map_type, MapContainerKind::Map),
        ] {
            if self.ssa_types.get(ty.index())
                != Some(&SsaTypeKind::MapContainer {
                    kind,
                    key,
                    value: val,
                })
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        if storage != descriptor.receiver_type() {
            return self.lower_nullable_map_consume(descriptor, source_type, map_type, span);
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(source_type))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.ensure_map_consume_delivery(root, span)?;
        // The validated whole-file ownership input proves the source move. This entry has
        // no independent receiver-effect table. Preserve every Name/Group cleanup boundary.
        let delivered = self.require_value(descriptor.receiver())?;
        if delivered != owner || self.bindings.get(&root) != Some(&LoweredValue::Value(owner)) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.ensure_map_consume_delivery(root, span)?;
        self.forget_delivered_owners(&[owner]);
        let (_, results) = self.append(
            Operation::MapConsume { owner, map_type },
            vec![EntityType::Value(map_type)],
            span,
        )?;
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn ensure_map_consume_delivery(&self, root: SymbolId, span: Span) -> Result<(), LoweringError> {
        self.reject_live_while_proof(root, span)?;
        if self.non_null_bindings.contains_key(&root) || self.borrow_bindings.contains_key(&root) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        // This bounded slice does not relax the existing proof/capture/provider carrier gate.
        self.ensure_no_unresolved_while_proof_carriers(span)
    }

    fn validate_map_consume_root(
        &self,
        mut expression: ExpressionId,
        root: SymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression: inner } => expression = *inner,
                Expression::Name if self.references.get(&span_key(node.span())) == Some(&root) => {
                    return Ok(());
                }
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            }
        }
    }
}

impl NominalTypeMapper {
    /// Register only the consume descriptors inside this reachable concrete instance.
    pub(super) fn intern_map_consume_types(
        &mut self,
        module: &mut Module,
        names: &NameResolution,
        typed: &TypedFile,
        parsed: &ParsedFile,
        substitutions: &BTreeMap<SymbolId, TypeId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for descriptor in typed.map_consumes() {
            let consume_span = parsed
                .ast()
                .expressions()
                .get(descriptor.expression())
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
                .span();
            if consume_span.source_id() != span.source_id()
                || consume_span.start() < span.start()
                || consume_span.end() > span.end()
            {
                continue;
            }
            for ty in [
                descriptor.receiver_type(),
                descriptor.result_type(),
                descriptor.key_type(),
                descriptor.value_type(),
            ] {
                let concrete =
                    instances::resolve_concrete_type(typed, ty, substitutions, consume_span)?;
                self.intern(module, names, typed, concrete, consume_span)?;
            }
        }
        Ok(())
    }
}
