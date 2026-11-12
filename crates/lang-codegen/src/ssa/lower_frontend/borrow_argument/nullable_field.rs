//! Call-scoped String field children of an existing nullable while proof and owned-local class field borrows.
use lang_frontend::{
    ownership_checking::{LoanFact, LoanKind as FrontendLoanKind, OwnershipPlace},
    type_checking::{
        AggregateProjectionKind, AggregateProjectionReceiver, CallableResultSource,
        ExpressionCategory,
    },
};

use super::super::*;
use crate::ssa::model::SsaTypeKind;

impl ExpressionLowerer<'_> {
    pub(super) fn lower_while_proof_field_borrow(
        &mut self,
        argument: ExpressionId,
        fact: &LoanFact,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        let LoanTarget::Place(path) = fact.target() else {
            return Ok(None);
        };
        let root = path.root();
        let Some(proof) = self.non_null_bindings.get(&root).copied() else {
            return Ok(None);
        };
        if !proof.is_while_origin() || path.is_root() {
            return Ok(None);
        }
        let [field_symbol] = path.fields() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !path.elements().is_empty() || self.borrow_bindings.contains_key(&root) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_non_null_proof(root, proof, &self.bindings, span)?;
        let call = self
            .typed
            .call(fact.call())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if call.result_source() != CallableResultSource::Owned {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let mut operands = vec![argument];
        let mut expression = argument;
        while let Expression::Group { expression: inner } = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
            .payload()
        {
            expression = *inner;
            operands.push(expression);
        }
        let projection = self
            .typed
            .aggregate_projection(expression)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let AggregateProjectionReceiver::Expression(mut receiver) = projection.receiver() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let storage = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let use_fact = self
            .typed
            .non_null_use(receiver)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let declared = self.resolve_type(storage, span)?;
        let narrowed = self.resolve_type(use_fact.narrowed_type(), span)?;
        let target = self.expression_ssa_type(argument, span)?;
        if fact.argument() != argument
            || projection.expression() != expression
            || projection.kind() != AggregateProjectionKind::Field
            || projection.field() != *field_symbol
            || self.typed.symbol_type(*field_symbol) != Some(projection.ty())
            || operands
                .iter()
                .any(|&operand| self.typed.expression_type(operand) != Some(projection.ty()))
            || !matches!(self.typed.types().get(declared), Some(TypeKind::Nullable(inner)) if *inner == narrowed)
            || self.type_ids.get(&narrowed) != Some(&proof.inner)
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != self.type_ids.get(&declared).copied().map(EntityType::Value)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if builtin_type(self.typed, self.resolve_type(projection.ty(), span)?)
            != Some(BuiltinType::String)
            || self.ssa_types.get(target.index()) != Some(&SsaTypeKind::StringOwner)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        loop {
            let fact = self
                .typed
                .non_null_use(receiver)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if fact.expression() != receiver
                || fact.symbol() != root
                || fact.declared_type() != storage
                || fact.narrowed_type() != use_fact.narrowed_type()
                || self.typed.expression_type(receiver) != Some(fact.narrowed_type())
                || self.typed.expression_category(receiver) != Some(ExpressionCategory::Place)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(receiver)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => receiver = *expression,
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&root) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        let Some(TypeKind::Nominal { nominal, arguments }) = self.typed.types().get(narrowed)
        else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .nominals()
            .iter()
            .find(|entry| entry.id() == *nominal)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || !nominal.type_parameters().is_empty()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let field = nominal
            .fields()
            .iter()
            .position(|field| field == field_symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(SsaTypeKind::HeapOwner {
            payload: Some(payload),
            ..
        }) = self.ssa_types.get(proof.inner.index())
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if self.heap_payloads.get(&proof.inner) != Some(payload)
            || !matches!(self.ssa_types.get(payload.index()), Some(SsaTypeKind::Aggregate { fields, .. }) if fields.get(field) == Some(&target))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // All qualification precedes emission. Only the child belongs to this call;
        // the current (possibly CFG-rebound) proof remains in its while scope.
        let (_, result) = self.append(
            Operation::SharedHeapFieldLoan {
                base: proof.loan,
                field,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }],
            fact.begin_span(),
        )?;
        let [EntityId::Loan(loan)] = result.as_slice() else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let loan = *loan;
        self.register_pending_field_child(fact, proof, field, target, loan, span)?;
        Ok(Some(loan))
    }

    pub(super) fn lower_owned_field_borrow_place(
        &mut self,
        expression: ExpressionId,
        path: &OwnershipPlace,
        target: SsaTypeId,
        span: Span,
    ) -> Result<PlaceId, LoweringError> {
        let [field_symbol] = path.fields() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !path.elements().is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let projection = self
            .typed
            .aggregate_projection(expression)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let AggregateProjectionReceiver::Expression(mut receiver) = projection.receiver() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if projection.expression() != expression
            || projection.kind() != AggregateProjectionKind::Field
            || projection.field() != *field_symbol
            || self.typed.expression_type(expression) != Some(projection.ty())
            || self.type_ids.get(&projection.ty()) != Some(&target)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let root = path.root();
        let symbol = self
            .names
            .symbols()
            .get(root.index())
            .filter(|symbol| symbol.id() == root)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if symbol.kind() != SymbolKind::Variable
            || self.typed.parameter_mode(root).is_some()
            || self.borrow_bindings.contains_key(&root)
            || self.non_null_bindings.contains_key(&root)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner_type = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        loop {
            if self.typed.non_null_use(receiver).is_some() {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            if self.typed.expression_type(receiver) != Some(owner_type) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(receiver)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => receiver = *expression,
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&root) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        let Some(TypeKind::Nominal { nominal, arguments }) = self.typed.types().get(owner_type)
        else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .nominals()
            .iter()
            .find(|declaration| declaration.id() == *nominal)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || !nominal.type_parameters().is_empty()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if self.typed.symbol_type(*field_symbol) != Some(projection.ty()) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let field = nominal
            .fields()
            .iter()
            .position(|candidate| candidate == field_symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        // Source sibling fields can be disjoint; the SSA alias model is still parent-wide.
        if self.owned.loans().iter().any(|active| {
            self.pending_call_loans
                .contains_key(&(active.call().index(), active.argument().index()))
                && active.kind() == FrontendLoanKind::Exclusive
                && matches!(active.target(), LoanTarget::Place(place) if place.root() == root)
        }) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner_ssa = self
            .type_ids
            .get(&owner_type)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(owner_ssa))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let Some(SsaTypeKind::HeapOwner {
            payload: Some(payload),
            ..
        }) = self.ssa_types.get(owner_ssa.index())
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if self.heap_payloads.get(&owner_ssa) != Some(payload)
            || !matches!(self.ssa_types.get(payload.index()), Some(SsaTypeKind::Aggregate { fields, .. }) if fields.get(field) == Some(&target))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // Read the existing binding directly: evaluating the receiver could run its
        // AfterExpression drop before the call. The caller owns the sole loan lifecycle.
        let (_, roots) = self.append(
            Operation::HeapPayloadPlace { owner },
            vec![EntityType::Place(*payload)],
            span,
        )?;
        let (_, fields) = self.append(
            Operation::FieldPlace {
                base: place(roots[0]),
                field,
            },
            vec![EntityType::Place(target)],
            span,
        )?;
        Ok(place(fields[0]))
    }
}
