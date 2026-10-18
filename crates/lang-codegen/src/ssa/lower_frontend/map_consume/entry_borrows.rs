//! Ordinary consume can retain independently anchored String or Int entry loans.
//! These are closed representations, not arbitrary HeapOwner/type inequalities.
use super::*;
use crate::ssa::model::{Definition, LoanKind};
use lang_frontend::{ownership_checking::OwnershipBindingKind, parser::VariableKind};

impl ExpressionLowerer<'_> {
    /// Only the nonnullable owned-local consume calls this, before and after its
    /// original receiver evaluation. No loan is ended, rebuilt or rebound here.
    pub(super) fn ensure_map_consume_entry_borrows(
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
        let mut represented_entry = false;
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
            let expected = match builtin_type(self.typed, ty) {
                Some(BuiltinType::String) => {
                    represented_entry = true;
                    SsaTypeKind::StringOwner
                }
                Some(BuiltinType::Int) => {
                    represented_entry = true;
                    SsaTypeKind::Integer {
                        bits: 32,
                        signed: true,
                    }
                }
                Some(BuiltinType::Boolean) => SsaTypeKind::Boolean,
                _ => return Err(unsupported()),
            };
            let target = *self.type_ids.get(&ty).ok_or_else(missing)?;
            if self.ssa_types.get(target.index()) != Some(&expected) {
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
        if !represented_entry {
            return Err(unsupported());
        }
        Ok(())
    }
}
