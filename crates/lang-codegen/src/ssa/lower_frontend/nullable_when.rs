//! Owned nullable subject 的一次求值、证明 edge 与显式 extraction。
use super::{
    BuiltinType, Copyability, EntityId, EntityType, ExpressionLowerer, LoanKind, LoweredValue,
    LoweringError, LoweringErrorKind, NominalKind, Operation, TypeKind, builtin_type, error,
    span_key, value,
};
use crate::ssa::model::{Edge, Origin, SsaTypeKind, TerminatorKind, ValueId};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    ownership_checking::{DropPoint, NullableWhenExtractionKind},
    parser::{Expression, LiteralKind, WhenCondition, WhenEntry},
    source::Span,
    type_checking::{
        AggregateProjectionDescriptor, AggregateProjectionKind, AggregateProjectionReceiver,
        ExpressionCategory, NullableWhenSubjectCategory, WhenDomain,
    },
};

impl ExpressionLowerer<'_> {
    pub(super) fn lower_nullable_when(
        &mut self,
        expression: ExpressionId,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let plan = self
            .typed
            .nullable_when(expression)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let ownership = self
            .owned
            .nullable_when(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if !plan.native_eligible()
            || !matches!(
                plan.category(),
                NullableWhenSubjectCategory::OwnedRoot | NullableWhenSubjectCategory::Temporary
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if ownership.subject() != plan.subject()
            || ownership.category() != plan.category()
            || entries.len() != plan.entries().len()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if let Some(symbol) = plan.stable_symbol() {
            self.reject_live_while_proof(symbol, span)?;
        }
        // Defer subject ASAP cleanup until the discriminator has read its owner.
        // Group aliases share the same single evaluation and owner.
        let mut subject_expressions = vec![plan.subject()];
        let mut evaluated = plan.subject();
        while let Ok(node) = self.parsed.ast().expressions().get(evaluated) {
            let Expression::Group { expression } = node.payload() else {
                break;
            };
            evaluated = *expression;
            subject_expressions.push(evaluated);
        }
        let owner = match self.lower_expression(evaluated)? {
            LoweredValue::Value(owner) => owner,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        if plan.category() == NullableWhenSubjectCategory::Temporary {
            self.temporaries.insert(plan.subject().index(), owner);
        }
        let inner = match self.typed.types().get(plan.subject_type()) {
            Some(lang_frontend::type_checking::TypeKind::Nullable(inner)) => self
                .type_ids
                .get(&self.resolve_type(*inner, span)?)
                .copied(),
            _ => None,
        }
        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let baseline = self.bindings.clone();
        let temporaries = self.temporaries.clone();
        let carried = self.linear_binding_slots(&baseline, span)?;
        let subject_slot = carried
            .slots
            .iter()
            .position(|slot| slot.source == EntityId::Value(owner))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
        let null = self
            .function
            .add_block(types.clone(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let mut proven_types = types;
        proven_types.push(EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        });
        let non_null = self
            .function
            .add_block(proven_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Loan(view) =
            self.function.block(non_null).expect("new block").parameters[carried.slots.len()]
        else {
            unreachable!("requested loan");
        };
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null: Edge {
                        target: null,
                        arguments: carried.slots.iter().map(|slot| slot.source).collect(),
                    },
                    when_non_null: Edge {
                        target: non_null,
                        arguments: carried.slots.iter().map(|slot| slot.source).collect(),
                    },
                    view,
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let mut exits = Vec::new();
        for (block, proven) in [(null, false), (non_null, true)] {
            self.block = block;
            self.temporaries.clone_from(&temporaries);
            self.bindings = self.rebind_linear_bindings(&baseline, block, &carried, span)?;
            let branch_owner =
                value(self.function.block(block).expect("new block").parameters[subject_slot]);
            if let Some(symbol) = plan.stable_symbol() {
                self.bindings
                    .insert(symbol, LoweredValue::Value(branch_owner));
                if proven {
                    let proof = super::NonNullProof::introduced(
                        view,
                        branch_owner,
                        inner,
                        super::non_null_proof::ProofScope::When(expression),
                    );
                    self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
                    self.non_null_bindings.insert(symbol, proof);
                }
            } else {
                self.temporaries
                    .insert(plan.subject().index(), branch_owner);
                if proven {
                    self.append(Operation::BorrowEnd { loan: view }, Vec::new(), span)?;
                }
            }
            for subject in subject_expressions.iter().rev() {
                self.emit_drops(DropPoint::AfterExpression(*subject))?;
            }
            let mut reached = false;
            for (index, (entry, typed_entry)) in entries.iter().zip(plan.entries()).enumerate() {
                let mut matched = entry.else_span.is_some();
                if !matched {
                    for (alternative, (condition, typed_alt)) in entry
                        .conditions
                        .iter()
                        .zip(typed_entry.alternatives())
                        .enumerate()
                    {
                        // Evaluate expression conditions even if their matching domain is empty.
                        // A literal null has no effects and needs no temporary nullable owner.
                        if let WhenCondition::Expression(candidate) = condition
                            && !matches!(
                                self.parsed
                                    .ast()
                                    .expressions()
                                    .get(*candidate)
                                    .map(|node| node.payload()),
                                Ok(Expression::Literal(LiteralKind::Null))
                            )
                        {
                            let result = self.lower(*candidate)?;
                            if matches!(result, LoweredValue::Diverged) {
                                reached = true;
                                break;
                            }
                        }
                        let mut accepts = domain_contains(typed_alt.match_domain(), proven);
                        let mut misses = domain_contains(typed_alt.fallthrough_domain(), proven);
                        if let WhenCondition::TypeTest {
                            type_ref, negated, ..
                        } = condition
                        {
                            let target = self.typed.type_ref_type(*type_ref).ok_or_else(|| {
                                error(LoweringErrorKind::MissingFact, typed_alt.span())
                            })?;
                            if self.type_ids.get(&self.resolve_type(target, span)?) == Some(&inner)
                            {
                                accepts = proven != *negated;
                                misses = !accepts;
                            }
                        }
                        if accepts && misses {
                            // Open-domain equality/type tests need a checked comparison recipe.
                            return Err(error(
                                LoweringErrorKind::UnsupportedNode,
                                typed_alt.span(),
                            ));
                        }
                        if accepts {
                            self.emit_drops(DropPoint::WhenAlternativeMatch {
                                control: expression,
                                entry: index,
                                alternative,
                            })?;
                            matched = true;
                            break;
                        }
                    }
                }
                if reached {
                    break;
                }
                if !matched {
                    continue;
                }
                // Only the common entry domain may expose the source binding as an inner view.
                if proven
                    && !typed_entry.body_domain().is_non_null()
                    && let Some(symbol) = plan.stable_symbol()
                {
                    self.end_non_null_proof(symbol, span)?;
                }
                let result = self.lower_control_body(entry.body)?;
                if !matches!(result, LoweredValue::Diverged) {
                    self.emit_drops(DropPoint::BranchExit {
                        control: expression,
                        branch: index,
                    })?;
                    if proven && let Some(symbol) = plan.stable_symbol() {
                        self.end_non_null_proof(symbol, span)?;
                    }

                    exits.push(self.branch_exit(result));
                }
                reached = true;
                break;
            }
            if !reached {
                self.function
                    .set_terminator(self.block, TerminatorKind::Abort, Origin::Source(span))
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            }
        }
        self.temporaries = temporaries;
        // Temporary subject obligations are discharged on each branch by frontend drop facts.
        self.temporaries.remove(&plan.subject().index());
        if self.expression_is_unit(expression, span)? {
            for exit in &mut exits {
                exit.result = LoweredValue::Unit;
            }
        }
        self.merge_exits(exits, &baseline, span)
    }

    pub(super) fn lower_nullable_extraction(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Option<ValueId>, LoweringError> {
        let Some((plan, fact)) = self.owned.nullable_whens().iter().find_map(|plan| {
            plan.extractions()
                .iter()
                .find(|fact| fact.expression() == expression)
                .map(|fact| (plan, fact))
        }) else {
            return Ok(None);
        };
        let typed = self.typed.nullable_when(plan.expression()).ok_or_else(|| {
            error(
                LoweringErrorKind::MissingFact,
                self.parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .expect("checked expression")
                    .span(),
            )
        })?;
        let span = typed.span();
        let symbol = typed
            .stable_symbol()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let inner = self.expression_ssa_type(expression, span)?;
        let proof = self
            .checked_non_null_proof(symbol, inner, span)?
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.reject_live_while_proof(symbol, span)?;
        let owner = proof.owner;
        if fact.kind() != NullableWhenExtractionKind::Consume {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let (_, result) = self.append(
            Operation::NullableTake {
                owner,
                proof: proof.loan,
            },
            vec![EntityType::Value(inner)],
            span,
        )?;
        self.bindings.remove(&symbol);
        self.non_null_bindings.remove(&symbol);
        Ok(Some(value(result[0])))
    }

    /// Returning the original nullable storage does not extract or rewrap its inner.
    /// Qualify the whole transparent operand before ending the real while view.
    pub(super) fn lower_while_owner_return(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let Some(use_fact) = self.typed.non_null_use(expression) else {
            return self.lower(expression);
        };
        let root = use_fact.symbol();
        let Some(proof) = self.non_null_bindings.get(&root).copied() else {
            return self.lower(expression);
        };
        if !proof.is_while_origin() {
            return self.lower(expression);
        }
        let storage = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let declared = self.resolve_type(storage, span)?;
        if self.resolve_type(self.return_type, span)? != declared {
            return self.lower(expression);
        }
        let narrowed = self.resolve_type(use_fact.narrowed_type(), span)?;
        let nullable = self
            .type_ids
            .get(&declared)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.validate_non_null_proof(root, proof, &self.bindings, span)?;
        if !matches!(self.typed.types().get(declared), Some(TypeKind::Nullable(inner)) if *inner == narrowed)
            || self.type_ids.get(&narrowed) != Some(&proof.inner)
            || self.ssa_types.get(nullable.index())
                != Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(nullable))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if self.borrow_bindings.contains_key(&root)
            || self.function.borrow_return.is_some()
            || self.typed.copyability(declared) != Some(Copyability::MoveOnly)
            || self.typed.copyability(narrowed) != Some(Copyability::MoveOnly)
            || !matches!(
                self.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
            || self.non_null_bindings.iter().any(|(&other, candidate)| {
                other != root && (candidate.loan == proof.loan || candidate.owner == proof.owner)
            })
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let mut operand = expression;
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(node.payload(), Expression::Name | Expression::Group { .. }) {
                return self.lower(expression);
            }
            let fact = self
                .typed
                .non_null_use(operand)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if fact.expression() != operand
                || fact.symbol() != root
                || fact.declared_type() != storage
                || fact.narrowed_type() != use_fact.narrowed_type()
                || self.typed.expression_type(operand) != Some(fact.narrowed_type())
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
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
        self.ensure_no_unresolved_while_proof_carriers(span)?;
        self.end_non_null_proof(root, span)?;
        // Keep every original Name/Group AfterExpression fact and the caller's
        // ControlTransfer cleanup. Phase 3 already consumes this return operand.
        self.lower(expression)
    }

    /// These carriers may name a child or a forwarded alias (including pending None).
    /// Do not infer independence from types, absent view slots, or a removed binding.
    /// Provider element IDs remain in their context after some cleanup actions, so
    /// this first slice conservatively refuses those contexts rather than guessing.
    pub(super) fn ensure_no_unresolved_while_proof_carriers(
        &self,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !self.pending_call_loans.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.ensure_no_other_while_proof_carriers(span)
    }

    pub(super) fn ensure_no_other_while_proof_carriers(
        &self,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !self.capture_loans.is_empty() || self.borrow_bindings.values().any(|&loan| {
            // A caller-provided Boolean loan cannot contain this owned handle
            // or a view born inside this function. Keep its identity and life.
            !self.is_entry_loan(loan)
                || !matches!(
                    self.function.entity(EntityId::Loan(loan)).map(|entity| entity.ty),
                    Some(EntityType::Loan { kind: LoanKind::Shared, target })
                        if matches!(self.ssa_types.get(target.index()), Some(SsaTypeKind::Boolean))
                )
        }) || !self.result_source_loans.is_empty() || self.readonly_receiver.is_some()
            || self.loops.iter().any(|context| context.for_loop.is_some())
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    /// Invoked after authorized cleanup, never to end an unresolved child on its behalf.
    pub(super) fn retire_while_proofs(
        &mut self,
        returning: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut ending = Vec::new();
        for (&symbol, &proof) in &self.non_null_bindings {
            if !proof.is_while_origin() {
                continue;
            }
            self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
            let inherited = !returning
                && self.loops.last().is_some_and(|context| {
                    context.entry_views.get(&symbol).is_some_and(|entry| {
                        entry.origin == proof.origin && entry.inner == proof.inner
                    })
                });
            if !inherited {
                ending.push(symbol);
            }
        }
        if !ending.is_empty() {
            self.ensure_no_unresolved_while_proof_carriers(span)?;
        }
        for symbol in ending {
            self.end_non_null_proof(symbol, span)?;
        }
        Ok(())
    }

    /// Assignment must have retired its old proof through RHS facts. Retesting in
    /// nullable if must not overwrite a live while proof before reuse is supported.
    pub(super) fn reject_live_while_proof(
        &self,
        symbol: SymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self
            .non_null_bindings
            .get(&symbol)
            .is_some_and(|proof| proof.is_while_origin())
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    pub(super) fn read_while_proof_int_field(
        &mut self,
        expression: ExpressionId,
        projection: AggregateProjectionDescriptor,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let AggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Ok(None);
        };
        let Some(proof) = self.non_null_proof_for_expression(receiver, span)? else {
            return Ok(None);
        };
        if !proof.is_while_origin() && !self.is_proven_nullable_if_field_read(expression, proof) {
            return Ok(None);
        }
        let use_fact = self
            .typed
            .non_null_use(receiver)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mut operands = vec![receiver];
        let mut leaf = receiver;
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(leaf)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => {
                    leaf = *expression;
                    operands.push(leaf);
                }
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&use_fact.symbol()) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        let receiver_type = self.resolve_type(use_fact.narrowed_type(), span)?;
        let declared = self.resolve_type(use_fact.declared_type(), span)?;
        let field_type = self.resolve_type(projection.ty(), span)?;
        if projection.expression() != expression
            || projection.kind() != AggregateProjectionKind::Field
            || use_fact.expression() != receiver
            || self.typed.expression_type(receiver) != Some(use_fact.narrowed_type())
            || self.typed.expression_type(expression) != Some(projection.ty())
            || self.type_ids.get(&receiver_type) != Some(&proof.inner)
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != self.type_ids.get(&declared).copied().map(EntityType::Value)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if builtin_type(self.typed, field_type) != Some(BuiltinType::Int)
            || self.typed.copyability(field_type) != Some(Copyability::Copyable)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(TypeKind::Nominal { nominal, arguments }) = self.typed.types().get(receiver_type)
        else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .nominals()
            .iter()
            .find(|entry| entry.id() == *nominal)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if nominal.kind() != NominalKind::Class || !arguments.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let field = nominal
            .fields()
            .iter()
            .position(|field| *field == projection.field())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let target = self.expression_ssa_type(expression, span)?;
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
        let (_, result) = self.append(
            Operation::HeapFieldRead {
                receiver: proof.loan,
                field,
            },
            vec![EntityType::Value(target)],
            span,
        )?;
        // Reading consumes no owner or child loan. Replay only the bypassed Name/Group
        // cleanup after the read; lower() still owns this field expression's cleanup.
        for operand in operands.into_iter().rev() {
            self.emit_drops(DropPoint::AfterExpression(operand))?;
        }
        Ok(Some(LoweredValue::Value(value(result[0]))))
    }
}

fn domain_contains(domain: &WhenDomain, non_null: bool) -> bool {
    match domain {
        WhenDomain::Nullable => true,
        WhenDomain::NonNull => non_null,
        WhenDomain::Null => !non_null,
        WhenDomain::Empty => false,
        WhenDomain::Finite(_) => false,
    }
}
