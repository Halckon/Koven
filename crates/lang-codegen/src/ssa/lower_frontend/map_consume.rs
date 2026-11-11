//! The checked owned-local consume path; ordinary Map borrowing stays separate.
mod nullable;

use super::{nominal::NominalTypeMapper, *};
use crate::ssa::model::{Definition, LoanKind, MapContainerKind, Module, SsaTypeKind};
use lang_frontend::{
    ownership_checking::{BorrowSourceLoan, OwnershipBindingKind},
    parser::VariableKind,
    type_checking::IntrinsicTypeConstructor,
};

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
        // Preserve the original gate, including its Boolean-entry compatibility.
        match self.ensure_no_unresolved_while_proof_carriers(span) {
            Ok(()) => Ok(()),
            Err(previous) if previous.kind == LoweringErrorKind::UnsupportedNode => {
                self.ensure_map_consume_entry_borrows(root, span)
            }
            Err(previous) => Err(previous),
        }
    }

    /// Only the nonnullable owned-local consume calls this, before and after its
    /// original receiver evaluation. No loan is ended, rebuilt or rebound here.
    fn ensure_map_consume_entry_borrows(
        &self,
        root: SymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        if !self.pending_call_loans.is_empty()
            || !self.capture_loans.is_empty()
            || !self.result_source_loans.is_empty()
            || !self.non_null_bindings.is_empty()
            || self.readonly_receiver.is_some()
            || self.thunk_expression.is_some()
            || !self.loops.is_empty()
            || self.function.receiver().is_some()
            || self.function.borrow_return.is_some()
            || self.function.carrier_return.is_some()
        {
            return Err(unsupported());
        }
        let source = self.names.symbols().get(root.index()).ok_or_else(missing)?;
        if source.kind() != SymbolKind::Variable {
            return Err(unsupported());
        }
        if source.id() != root || source.span().source_id() != self.parsed.source_id() {
            return Err(missing());
        }
        if source.is_synthetic() || source.name() == "_" {
            return Err(unsupported());
        }
        let mut declarations = 0;
        for (_, node) in self.parsed.ast().statements().iter() {
            let Statement::LocalVariable { declaration } = node.payload() else {
                continue;
            };
            let (item, _) = orchestrate::unwrap_modified(self.parsed, *declaration)?;
            if let Item::Variable {
                name: NameMarker::Present(name),
                kind,
                ..
            } = item
                && name == source.span()
            {
                if !matches!(kind, VariableKind::Val | VariableKind::Var) {
                    return Err(unsupported());
                }
                declarations += 1;
            }
        }
        // Formation is only historical identity, never current liveness/permission.
        // Current ownership is still checked by the original consume delivery path.
        if declarations != 1 || !self.owned.formed_owned_locals().contains(&root) {
            return Err(missing());
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Err(missing());
        };
        if self
            .bindings
            .iter()
            .any(|(&symbol, &binding)| symbol != root && binding == LoweredValue::Value(owner))
            || self.temporaries.values().any(|&value| value == owner)
        {
            return Err(unsupported());
        }
        let mut instances = self
            .function_ids
            .iter()
            .filter(|(_, id)| **id == self.function.id());
        let (instance, _) = instances.next().ok_or_else(missing)?;
        if instances.next().is_some() {
            return Err(missing());
        }
        let mut callables = self
            .typed
            .callables()
            .iter()
            .filter(|callable| callable.symbol() == instance.symbol());
        let callable = callables.next().ok_or_else(missing)?;
        if callables.next().is_some() {
            return Err(missing());
        }
        // No hidden receiver/callable slot or specialized parameter ordering.
        if callable.owner().is_some()
            || callable.receiver().is_some()
            || callable.extension_receiver_symbol().is_some()
            || !callable.type_parameters().is_empty()
            || !instance.type_arguments().is_empty()
            || !instance.callable_arguments().is_empty()
        {
            return Err(unsupported());
        }
        let entry = self.function.entry_block().ok_or_else(missing)?;
        let parameters = &self.function.block(entry).ok_or_else(missing)?.parameters;
        if parameters.len() != callable.parameters().len()
            || callable.parameter_symbols().len() != callable.parameters().len()
        {
            return Err(missing());
        }
        if self.borrow_bindings.is_empty() {
            return Err(unsupported());
        }
        for (&symbol, &loan) in &self.borrow_bindings {
            let data = self
                .names
                .symbols()
                .get(symbol.index())
                .ok_or_else(missing)?;
            if symbol == root || data.kind() != SymbolKind::ValueParameter {
                return Err(unsupported());
            }
            let mut slots = callable
                .parameter_symbols()
                .iter()
                .enumerate()
                .filter(|(_, actual)| **actual == Some(symbol));
            let (slot, _) = slots.next().ok_or_else(missing)?;
            if slots.next().is_some() {
                return Err(missing());
            }
            let parameter = callable.parameters().get(slot).ok_or_else(missing)?;
            if parameter.mode != ParameterMode::Borrow {
                return Err(unsupported());
            }
            if self.typed.parameter_mode(symbol) != Some(ParameterMode::Borrow)
                || self.typed.symbol_type(symbol) != Some(parameter.ty)
                || self.owned.binding_kind(symbol) != Some(OwnershipBindingKind::Shared)
                || self.bindings.contains_key(&symbol)
            {
                return Err(missing());
            }
            let ty = self.resolve_type(parameter.ty, span)?;
            // Ordinary parameter planning has already admitted this concrete
            // representation. Match its original Shared slot exactly, including
            // a Borrow Map with the same K/V as the independently owned local.
            let target = *self.type_ids.get(&ty).ok_or_else(missing)?;
            if self.ssa_types.get(target.index()).is_none() {
                return Err(missing());
            }
            let entity = self
                .function
                .entity(EntityId::Loan(loan))
                .ok_or_else(missing)?;
            let Definition::BlockParameter { block, index } = entity.definition else {
                return Err(unsupported());
            };
            if block != entry {
                return Err(unsupported());
            }
            if index != slot
                || parameters.get(slot) != Some(&EntityId::Loan(loan))
                || entity.ty
                    != (EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    })
            {
                return Err(missing());
            }
        }
        Ok(())
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

    pub(super) fn lower_map_with_value(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        let d = *self
            .typed
            .map_with_value(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mut loans = Vec::new();
        for operand in [d.receiver(), d.key(), d.action()] {
            let operand_span = self.expression_span(operand)?;
            if self.runtime_operand_exits(operand, operand_span)? {
                return Ok(LoweredValue::Diverged);
            }
            let (loan, new) = self.lower_borrow_argument(expression, operand, operand_span)?;
            loans.push(loan);
            self.pending_call_loans
                .insert((expression.index(), operand.index()), new.then_some(loan));
        }
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::MapWithValue {
                source: loans[0],
                key: EntityId::Loan(loans[1]),
                action: loans[2],
            },
            vec![EntityType::Value(ty)],
            span,
        )?;
        self.finish_borrowed_call(expression, span)?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    pub(super) fn lower_map_require_result(
        &mut self,
        expression: ExpressionId,
        source: BorrowSourceLoan<ExpressionId>,
        span: Span,
    ) -> Result<(LoanId, Vec<LoanId>), LoweringError> {
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
            .span();
        let descriptor = *self
            .typed
            .map_require_value(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if source.call() != expression || source.argument() != descriptor.receiver() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let receiver_span = self.expression_span(descriptor.receiver())?;
        let (source_loan, created) =
            self.lower_borrow_argument(expression, descriptor.receiver(), receiver_span)?;
        let key_span = self.expression_span(descriptor.key())?;
        let (key_loan, key_created) =
            self.lower_borrow_argument(expression, descriptor.key(), key_span)?;
        let target = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::MapRequireValue {
                source: source_loan,
                key: EntityId::Loan(key_loan),
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }],
            span,
        )?;
        let [EntityId::Loan(result)] = results.as_slice() else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        if key_created {
            self.append(Operation::BorrowEnd { loan: key_loan }, Vec::new(), span)?;
        }
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok((
            *result,
            if created {
                vec![source_loan]
            } else {
                Vec::new()
            },
        ))
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
