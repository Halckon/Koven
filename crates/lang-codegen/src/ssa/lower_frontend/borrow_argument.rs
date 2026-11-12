//! Value/Borrow argument evaluation and temporary-place carriers.
use super::{
    BuiltinType, EntityId, EntityType, ExpressionLowerer, LoanId, LoanKind, LoweredValue,
    LoweringError, LoweringErrorKind, NominalKind, Operation, PlaceId, SsaTypeId, SymbolKind,
    TypeKind, ValueId, builtin_type, error, place, span_key,
};
use crate::ssa::model::SsaTypeKind;
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{LoanFact, LoanKind as FrontendLoanKind, LoanTarget, OwnershipPlace},
    parser::Expression,
    source::Span,
    type_checking::{
        AggregateProjectionKind, AggregateProjectionReceiver, CallableResultSource, Copyability,
        ExpressionCategory, RcOperationKind,
    },
};

impl ExpressionLowerer<'_> {
    /// MoveOnly values are detached immediately and carried until all operands are ready.
    pub(super) fn lower_value_argument(
        &mut self,
        argument: ExpressionId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let value = self.require_value(argument)?;
        self.validate_map_value_delivery(argument, value, span)?;
        if self
            .typed
            .expression_type(argument)
            .and_then(|ty| self.typed.copyability(ty))
            == Some(Copyability::MoveOnly)
        {
            let mut source = argument;
            loop {
                self.temporaries.remove(&source.index());
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(source)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                match node.payload() {
                    Expression::Group { expression } => source = *expression,
                    Expression::Name => {
                        let symbol = self
                            .references
                            .get(&span_key(node.span()))
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, node.span()))?;
                        self.bindings.remove(symbol);
                        break;
                    }
                    _ => break,
                }
            }
            self.temporaries.insert(argument.index(), value);
        }
        Ok(value)
    }

    pub(super) fn take_pending_value_argument(
        &mut self,
        argument: ExpressionId,
        span: Span,
    ) -> Result<Option<ValueId>, LoweringError> {
        if self
            .typed
            .expression_type(argument)
            .and_then(|ty| self.typed.copyability(ty))
            != Some(Copyability::MoveOnly)
        {
            return Ok(None);
        }
        self.temporaries
            .remove(&argument.index())
            .map(Some)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    pub(super) fn lower_borrow_argument(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        span: Span,
    ) -> Result<(LoanId, bool), LoweringError> {
        if self.owned.borrow_results().range_uses().iter().any(|fact| {
            fact.expression() == argument
                && fact.site() == lang_frontend::ownership_checking::RangeUseSite::Call(call)
        }) {
            let target = self.expression_ssa_type(argument, span)?;
            return self
                .lower_short_range(argument, target, span)
                .map(|loan| (loan, true));
        }
        if let Some(proof) = self.non_null_proof_for_expression(argument, span)? {
            return Ok((proof.loan, false));
        }
        let fact = self
            .owned
            .loan_begin(argument)
            .filter(|fact| fact.call() == call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if fact.kind() != lang_frontend::ownership_checking::LoanKind::Shared {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let mut actual = argument;
        while let Expression::Group { expression } = self
            .parsed
            .ast()
            .expressions()
            .get(actual)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
            .payload()
        {
            actual = *expression;
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(actual)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if matches!(
            self.typed
                .expression_type(argument)
                .and_then(|ty| self.typed.types().get(ty)),
            Some(lang_frontend::type_checking::TypeKind::Intrinsic {
                constructor: lang_frontend::type_checking::IntrinsicTypeConstructor::View,
                ..
            })
        ) && matches!(node.payload(), Expression::Name | Expression::This)
            && let Some(symbol) = self.references.get(&super::span_key(node.span()))
            && let Some(LoweredValue::Value(view)) = self.bindings.get(symbol).copied()
        {
            let target = self.expression_ssa_type(argument, span)?;
            let (_, results) = self.append(
                Operation::RootPlace { owner: view },
                vec![EntityType::Place(target)],
                span,
            )?;
            let place = place(results[0]);
            let (_, results) = self.append(
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                span,
            )?;
            let EntityId::Loan(loan) = results[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            return Ok((loan, true));
        }
        if matches!(node.payload(), Expression::Name | Expression::This)
            && let Some(symbol) = self.references.get(&super::span_key(node.span()))
            && self
                .owned
                .borrow_results()
                .bindings()
                .iter()
                .any(|binding| binding.binding() == *symbol && binding.origin() == fact.target())
            && let Some(loan) = self.borrow_bindings.get(symbol).copied()
        {
            return Ok((loan, false));
        }
        if let Some((loan, created)) = self.readonly_view(argument)? {
            // The current call carrier stores one pending loan. Deeper borrowed projections
            // need an ancestor-loan carrier across argument CFG and remain fail-closed.
            if created.len() > 1 {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            return Ok((loan, !created.is_empty()));
        }
        if let LoanTarget::Place(target) = fact.target()
            && target.is_root()
            && let Some(loan) = self.borrow_bindings.get(&target.root()).copied()
            && self
                .function
                .entity(EntityId::Loan(loan))
                .map(|data| data.ty)
                == Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: self.expression_ssa_type(argument, span)?,
                })
        {
            return Ok((loan, false));
        }
        if let LoanTarget::Place(target) = fact.target()
            && !target.is_root()
            && self.borrow_bindings.contains_key(&target.root())
            && self.typed.aggregate_projection(argument).is_some()
        {
            let mut created = Vec::new();
            let loan = self.clone_field_loan(argument, &mut created)?;
            if created.len() != 1 {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            return Ok((loan, true));
        }
        if let Some(loan) = self.lower_while_proof_field_borrow(argument, fact, span)? {
            return Ok((loan, true));
        }
        let place = self.lower_borrow_place(argument, fact.target(), span)?;
        let target = self.expression_ssa_type(argument, span)?;
        let (_, results) = self.append(
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }],
            fact.begin_span(),
        )?;
        let EntityId::Loan(loan) = results[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, fact.begin_span()));
        };
        Ok((loan, true))
    }

    fn lower_borrow_place(
        &mut self,
        argument: ExpressionId,
        target: &LoanTarget,
        span: Span,
    ) -> Result<PlaceId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(argument)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Group { expression } = node.payload() {
            return self.lower_borrow_place(*expression, target, span);
        }
        if let LoanTarget::Temporary(temporary) = target
            && let Some(element) = self.typed.element_place(argument)
        {
            let owner = self.require_value(*temporary)?;
            let index = self.require_value(element.index())?;
            let element_type = self.expression_ssa_type(argument, span)?;
            let (_, results) = self.append(
                Operation::ContainerElementPlace {
                    owner: EntityId::Value(owner),
                    index,
                },
                vec![EntityType::Place(element_type)],
                span,
            )?;
            return Ok(place(results[0]));
        }
        if let LoanTarget::Place(target) = target
            && !target.is_root()
            && let Some(place) = self.lower_borrowed_container_element(argument, target, span)?
        {
            return Ok(place);
        }
        if let Some(operation) = self.typed.rc_operation(argument)
            && operation.kind() == RcOperationKind::Value
        {
            let owner = self.shared_owner_operand(operation.receiver(), span)?;
            let payload = self.expression_ssa_type(argument, span)?;
            let (_, results) = self.append(
                Operation::SharedPayloadPlace { owner },
                vec![EntityType::Place(payload)],
                span,
            )?;
            return Ok(place(results[0]));
        }
        if let LoanTarget::Place(path) = target
            && !path.is_root()
        {
            let target = self.expression_ssa_type(argument, span)?;
            return self.lower_owned_field_borrow_place(argument, path, target, span);
        }
        let owner = match target {
            LoanTarget::Place(place) if place.is_root() => self
                .bindings
                .get(&place.root())
                .and_then(|value| match value {
                    LoweredValue::Value(value) => Some(*value),
                    LoweredValue::Unit | LoweredValue::Diverged => None,
                })
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?,
            LoanTarget::Temporary(temporary) => self.require_value(*temporary)?,
            LoanTarget::Place(_) | LoanTarget::This(_) => {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        let target_type = self.expression_ssa_type(argument, span)?;
        // Match the validated nullable use to its whole-root query result before
        // copying the registered payload for this synchronous shared argument.
        let mut owner = owner;
        if let LoanTarget::Place(root) = target
            && root.is_root()
            && let Some(proof) = self.typed.non_null_use(argument)
            && self.is_map_result_value(owner)
        {
            let declared = self.resolve_type(proof.declared_type(), span)?;
            let narrowed = self.resolve_type(proof.narrowed_type(), span)?;
            if self.typed.copyability(narrowed)
                != Some(lang_frontend::type_checking::Copyability::Copyable)
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let actual = self
                .function
                .entity(EntityId::Value(owner))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .ty
                .semantic_type();
            if proof.symbol() != root.root()
                || self.type_ids.get(&declared) != Some(&actual)
                || self.type_ids.get(&narrowed) != Some(&target_type)
                || self.map_results.get(&actual) != Some(&target_type)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            owner = match self.unwrap_map_result(owner, span)? {
                LoweredValue::Value(value) => value,
                _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
            };
        }
        let (_, results) = self.append(
            Operation::RootPlace { owner },
            vec![EntityType::Place(target_type)],
            span,
        )?;
        Ok(place(results[0]))
    }

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
