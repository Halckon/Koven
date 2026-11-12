//! Real nullable When entry, two independent successors and ordinary CFG merge.
mod source;

use super::control::ControlResult;
use super::*;
use crate::ssa::model::SsaTypeKind;
use lang_frontend::{
    ownership_checking::{
        LoanKind as FrontendLoanKind, OwnershipBindingKind, UnitCallArgumentOwnershipKind,
        UnitDropTarget, UnitLoanFact, UnitLoanTarget, UnitNullableWhenUseFact,
        UnitNullableWhenUseKind, UnitValueDeliveryKind,
    },
    parser::{WhenCondition, WhenEntry},
    type_checking::{
        CallableResultSource, NullableWhenSubjectCategory, UnitCallTarget,
        UnitNullableWhenDescriptor, UnitNullableWhenDomain, UnitNullableWhenUseWitness,
    },
};

/// The source version is frozen by the typed subject, never sampled in the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NullableWhenScope {
    control: UnitExpressionId,
    entry: usize,
    generation: u64,
    loop_depth: usize,
}

struct NullableWhenSource {
    operands: Vec<UnitExpressionId>,
    root: Option<UnitSymbolId>,
    inner: SsaTypeId,
    nullable: SsaTypeId,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_nullable_when(
        &mut self,
        control: UnitExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .nullable_when(control)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            .clone();
        let source = self.prepare_nullable_when_source(&descriptor, span)?;
        let result_plan = self.control_result(control.expression(), None, span)?;
        let result_type = self
            .typed
            .expression_type(control)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.copyability(result_type) == Copyability::MoveOnly {
            // This slice does not turn a When tail into an implicit owned extraction.
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let checkpoint = self.loop_state();
        let result = self.lower_nullable_when_source(&descriptor, &source, result_plan, span);
        if result.is_err() {
            // No failed entry leaves an installed proof or half-retired binding.
            // The enclosing lowering error discards the unpublished SSA function.
            self.restore_loop_state(&checkpoint);
        }
        result
    }

    fn lower_nullable_when_source(
        &mut self,
        descriptor: &UnitNullableWhenDescriptor,
        source: &NullableWhenSource,
        result_plan: ControlResult,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let owner = match self.evaluate_nullable_when_subject(descriptor, source, span)? {
            LoweredValue::Value(owner) => owner,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let baseline = self.loop_state();
        let mut carried = self.carried_bindings(&baseline.bindings, span)?;
        let mut loans = self.carried_loans(&baseline.borrow_bindings, span)?;
        self.carry_pending_operands(&mut carried, &mut loans, span)?;
        let column = match source.root {
            Some(root) => self.nullable_owner_column(&carried, root, owner, span)?,
            None => {
                self.nullable_temporary_owner_column(&carried, descriptor.subject(), owner, span)?
            }
        };
        let null =
            self.add_carried_control_block(&carried, &loans, descriptor.entries()[0].span())?;
        let (non_null, view) = self.add_map_borrow_block(
            &carried,
            &loans,
            source.inner,
            descriptor.entries()[1].span(),
        )?;
        let header = self.block;
        self.function
            .set_terminator(
                header,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null: cfg::carried_control_edge(null, &carried, &loans),
                    when_non_null: cfg::carried_control_edge(non_null, &carried, &loans),
                    view,
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let mut exits = Vec::with_capacity(2);
        for (index, block) in [(0, null), (1, non_null)] {
            // Never start the second arm with the first arm's state or transfer effects.
            self.restore_loop_state(&baseline);
            self.block = block;
            if index == 1 {
                self.rebind_map_borrow_control(
                    &baseline.bindings,
                    block,
                    &carried,
                    &loans,
                    view,
                    source.inner,
                    span,
                )?;
                if let Some(root) = source.root {
                    let generation = descriptor
                        .source_generation()
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    self.introduce_nullable_proof(
                        header,
                        column,
                        root,
                        source.inner,
                        nullable_proof::NullableProofScope::When(NullableWhenScope {
                            control: descriptor.expression(),
                            entry: index,
                            generation,
                            loop_depth: self.loops.len(),
                        }),
                        span,
                    )?;
                } else {
                    let (actual_owner, actual_view) =
                        self.nullable_branch_view_owner(header, column, source.inner, span)?;
                    if actual_view != view
                        || self.temporaries.get(&descriptor.subject()) != Some(&actual_owner)
                    {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                    // No source symbol/use exists for a temporary subject. Its unused
                    // view can end now, while the real owner lives to the When boundary.
                    self.function
                        .append_instruction(
                            block,
                            Operation::BorrowEnd { loan: actual_view },
                            Vec::new(),
                            Origin::Source(span),
                        )
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                }
            } else {
                self.bindings =
                    self.rebind_carried_control(&baseline.bindings, block, &carried, &loans, span)?;
                self.borrow_bindings =
                    self.rebind_carried_loans(block, carried.len(), &loans, span)?;
            }
            self.emit_nullable_when_operand_drops(&source.operands)?;
            let entry = &descriptor.entries()[index];
            let before = self.current_loop_exit_counts();
            let branch = self.lower_if_branch(
                block,
                entry.body().statement(),
                self.current_receiver,
                self.consumed_receiver,
                self.bindings.clone(),
                self.borrow_bindings.clone(),
                self.nullable_proofs.clone(),
                self.pending_operands.clone(),
                self.pending_call_frames.clone(),
                self.temporaries.clone(),
                UnitDropPoint::BranchExit {
                    control: descriptor.expression(),
                    branch: index,
                },
                result_plan,
            )?;
            let owned_entry = self
                .owned
                .nullable_when(descriptor.expression())
                .and_then(|plan| plan.entries().get(index))
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let after = self.current_loop_exit_counts();
            if branch.is_some() != owned_entry.falls_through()
                || (after.0 > before.0) != owned_entry.breaks()
                || (after.1 > before.1) != owned_entry.continues()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, entry.span()));
            }
            if let Some(mut exit) = branch {
                self.restore_loop_state(&exit);
                self.retire_nullable_when_scope(descriptor.expression(), span)?;
                exit.nullable_proofs.clone_from(&self.nullable_proofs);
                exits.push(exit);
            }
        }
        // Empty exits use the existing double-escaping cleanup; no synthetic merge.
        self.merge_unit_exits(exits, span)
    }

    pub(super) fn valid_nullable_when_scope(
        &self,
        root: UnitSymbolId,
        scope: NullableWhenScope,
        inner: SsaTypeId,
    ) -> bool {
        let Some(descriptor) = self.typed.nullable_when(scope.control) else {
            return false;
        };
        descriptor.expression() == scope.control
            && scope.control.source_unit() == self.source_unit
            && descriptor.stable_symbol() == Some(root)
            && descriptor.source_generation() == Some(scope.generation)
            && descriptor.source_category() == NullableWhenSubjectCategory::OwnedRoot
            && self.typed.symbol_type(root) == Some(descriptor.nullable_type())
            && resolve_concrete_type(
                self.typed,
                descriptor.inner_type(),
                self.substitutions,
                self.static_self,
                self.function.origin.span(),
            )
            .is_ok_and(|ty| self.type_ids.get(&ty) == Some(&inner))
            && scope.loop_depth <= self.loops.len()
            && descriptor.entries().get(scope.entry).is_some_and(|entry| {
                entry.index() == scope.entry
                    && entry.body().source_unit() == self.source_unit
                    && entry.body_domain() == UnitNullableWhenDomain::NonNull
            })
            && self.owned.nullable_when(scope.control).is_some_and(|plan| {
                plan.descriptor() == descriptor
                    && plan
                        .source_place()
                        .is_some_and(|path| path.is_root() && path.root() == root)
            })
    }
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn emit_nullable_when_operand_drops(
        &mut self,
        operands: &[UnitExpressionId],
    ) -> Result<(), LoweringError> {
        for operand in operands.iter().rev() {
            self.emit_drops(UnitDropPoint::AfterExpression(*operand))?;
        }
        Ok(())
    }

    pub(super) fn validate_nullable_when_retirement(
        &self,
        root: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let proof = self
            .nullable_proofs
            .get(&root)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if proof.when_scope().is_none() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        let aliases_owner = |entity| entity == EntityId::Value(proof.owner);
        let aliases_view = |entity| entity == EntityId::Loan(proof.loan);
        if self
            .pending_operands
            .iter()
            .copied()
            .any(|entity| aliases_owner(entity) || aliases_view(entity))
            || self.current_receiver.is_some_and(|receiver| {
                aliases_owner(receiver.entity) || aliases_view(receiver.entity)
            })
            || self
                .borrow_bindings
                .values()
                .any(|loan| *loan == proof.loan)
            || self.nullable_proofs.iter().any(|(other, candidate)| {
                *other != root && (candidate.loan == proof.loan || candidate.owner == proof.owner)
            })
            || self
                .bindings
                .iter()
                .any(|(other, value)| *other != root && *value == LoweredValue::Value(proof.owner))
            || self.pending_call_frames.iter().any(|frame| {
                self.pending_operands.len() > frame.pending_start
                    && frame.loan_arguments.iter().any(|(argument, _)| {
                        self.owned.loans().iter().any(|fact| {
                            fact.call() == frame.call
                                && fact.argument() == *argument
                                && matches!(fact.target(), UnitLoanTarget::Place(path)
                                    if path.root() == root)
                        })
                    })
            })
        {
            // This checks dependency, not global carrier emptiness. An outer call
            // prefix with a different owner/loan survives normal and loop exits.
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    pub(super) fn retire_nullable_when_scope(
        &mut self,
        control: UnitExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let roots = self
            .nullable_proofs
            .iter()
            .filter_map(|(&root, proof)| {
                proof
                    .when_scope()
                    .is_some_and(|scope| scope.control == control)
                    .then_some(root)
            })
            .collect::<Vec<_>>();
        for &root in &roots {
            self.validate_nullable_when_retirement(root, span)?;
        }
        for root in roots {
            self.end_nullable_proof(root, span)?;
        }
        Ok(())
    }

    /// Called after exiting call-frame children and exact ControlTransfer drops.
    /// Birth depth distinguishes a leaving When from any retained outer scope.
    pub(super) fn retire_nullable_when_transfers(
        &mut self,
        minimum_loop_depth: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        let roots = self
            .nullable_proofs
            .iter()
            .filter_map(|(&root, proof)| {
                proof
                    .when_scope()
                    .is_some_and(|scope| scope.loop_depth >= minimum_loop_depth)
                    .then_some(root)
            })
            .collect::<Vec<_>>();
        for &root in &roots {
            self.validate_nullable_when_retirement(root, span)?;
        }
        for root in roots {
            self.end_nullable_proof(root, span)?;
        }
        Ok(())
    }

    /// The producer's pure Name/Group Consume path only takes the named state.
    /// Check every original point, leaf first, before any SSA or owner mutation.
    /// Empty drop facts also make closure validation and late-Map drop dispatch
    /// structurally empty; empty end facts make end_result_bindings a no-op.
    pub(in crate::ssa::unit_lower) fn preflight_nullable_take_drops(
        &self,
        operands: &[UnitExpressionId],
        span: Span,
    ) -> Result<(), LoweringError> {
        for operand in operands.iter().rev() {
            let point = UnitDropPoint::AfterExpression(*operand);
            if self.owned.drops().iter().any(|fact| fact.point() == point)
                || self
                    .owned
                    .borrow_results()
                    .ends()
                    .iter()
                    .any(|fact| fact.point() == point)
                || self.owned.iteration_cleanup_at(point).is_some()
                || self
                    .owned
                    .conditional_receiver_drops()
                    .iter()
                    .any(|fact| fact.point() == point)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }
}

struct NullableWhenArgument {
    witness: UnitNullableWhenUseWitness,
    proof: nullable_proof::NullableProof,
    operands: Vec<UnitExpressionId>,
    fact: UnitNullableWhenUseFact,
}

impl UnitExpressionLowerer<'_> {
    fn prepare_nullable_when_argument(
        &self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        mode: ParameterMode,
        span: Span,
    ) -> Result<Option<NullableWhenArgument>, LoweringError> {
        let root = self.direct_name_symbol(argument.expression(), span)?;
        let uses_when = self.typed.nullable_when_use(argument).is_some()
            || root.is_some_and(|root| {
                self.nullable_proofs
                    .get(&root)
                    .is_some_and(|proof| proof.when_scope().is_some())
            });
        if !uses_when {
            return Ok(None);
        }
        let (witness, proof, operands) = self.validate_nullable_when_use(argument, span)?;
        let descriptor = self
            .typed
            .call(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let UnitCallTarget::Declaration(target) = descriptor.target() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if descriptor.receiver().is_some()
            || descriptor.result_source() != CallableResultSource::Owned
            || descriptor.aborts()
            || descriptor.prints_line()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let route = self
            .source_plan
            .call_site(self.source_token, call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if call.source_unit() != self.source_unit
            || descriptor.expression() != call
            || route.key().target() != UnitCallableTarget::Declaration(target)
            || route
                .dispatch_key()
                .is_some_and(|key| !self.function_ids.contains_key(key))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if !route.delegation().is_empty() || !route.key().callable_arguments().is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let call_node = self
            .parsed
            .ast()
            .expressions()
            .get(call.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call { arguments, .. } = call_node.payload() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let mut actuals = arguments.iter().enumerate().filter(|(_, actual)| {
            UnitExpressionId::new(self.source_unit, actual.value) == argument
        });
        let (index, _) = actuals
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let mut mappings = descriptor
            .arguments()
            .iter()
            .filter(|mapping| mapping.argument_index() == index);
        let mapping = mappings
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let mut contracts = self
            .owned
            .call_argument_contracts_for(call)
            .filter(|contract| contract.argument() == argument);
        let contract = contracts
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let fact = self
            .owned
            .nullable_when_use(argument)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let plan = self
            .owned
            .nullable_when(witness.control())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(argument.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let expected = match mode {
            ParameterMode::Value => UnitCallArgumentOwnershipKind::Value,
            ParameterMode::Borrow => UnitCallArgumentOwnershipKind::SharedLoan,
            ParameterMode::Inout => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        if actuals.next().is_some()
            || mappings.next().is_some()
            || contracts.next().is_some()
            || mapping.mode() != mode
            || mapping.crosses_thread()
            || contract.crosses_thread()
            || contract.parameter_index() != mapping.parameter_index()
            || contract.parameter_type() != mapping.parameter_type()
            || contract.category() != ExpressionCategory::Place
            || mapping.category() != contract.category()
            || contract.kind() != expected
            || contract.argument_span() != node.span()
            || contract.call_span() != call_node.span()
            || fact.call() != call
            || fact.argument() != argument
            || fact.reference() != witness.reference()
            || fact.witness() != witness
            || plan
                .uses()
                .iter()
                .filter(|candidate| *candidate == fact)
                .count()
                != 1
            || root != Some(witness.symbol())
            || self.bindings.get(&witness.symbol()) != Some(&LoweredValue::Value(proof.owner))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let concrete =
            |ty| resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span);
        let inner = concrete(witness.inner_type())?;
        let nullable = concrete(witness.nullable_type())?;
        if concrete(mapping.parameter_type())? != inner
            || self.type_ids.get(&inner) != Some(&proof.inner)
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|entity| entity.ty)
                != self.type_ids.get(&nullable).copied().map(EntityType::Value)
            || self.typed.copyability(inner) != Copyability::MoveOnly
            || !matches!(
                self.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. } | SsaTypeKind::SharedOwner { .. })
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        match mode {
            ParameterMode::Borrow => {
                let loan = fact
                    .loan()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let loans = self
                    .owned
                    .loans()
                    .iter()
                    .filter(|loan| loan.call() == call && loan.argument() == argument)
                    .collect::<Vec<_>>();
                if fact.kind() != UnitNullableWhenUseKind::Observe
                    || fact.delivery().is_some()
                    || fact.moved_span().is_some()
                    || self
                        .owned
                        .value_deliveries()
                        .iter()
                        .any(|delivery| delivery.call() == call && delivery.argument() == argument)
                    || loans.as_slice() != [loan]
                    || loan.kind() != FrontendLoanKind::Shared
                    || loan.begin_span() != contract.loan_begin_span()
                    || loan.end_span() != call_node.span()
                    || loan.parameter_span() != contract.parameter_span()
                    || !matches!(loan.target(), UnitLoanTarget::Place(path)
                        if path.is_root() && path.root() == witness.symbol())
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
            ParameterMode::Value => {
                let delivery = fact
                    .delivery()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let deliveries = self
                    .owned
                    .value_deliveries()
                    .iter()
                    .filter(|delivery| delivery.call() == call && delivery.argument() == argument)
                    .collect::<Vec<_>>();
                let leaf = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(witness.reference().expression())
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let owned = self.owned.binding_kind(witness.symbol())
                    == Some(OwnershipBindingKind::Owned)
                    || (self.owned.binding_kind(witness.symbol()).is_none()
                        && self.owned.formed_owned_locals().contains(&witness.symbol()));
                if fact.kind() != UnitNullableWhenUseKind::Consume
                    || fact.loan().is_some()
                    || self
                        .owned
                        .loans()
                        .iter()
                        .any(|loan| loan.call() == call && loan.argument() == argument)
                    || deliveries.as_slice() != [delivery]
                    || delivery.kind() != UnitValueDeliveryKind::Move
                    || delivery.span() != node.span()
                    || delivery.parameter_span() != contract.parameter_span()
                    || delivery
                        .place()
                        .is_none_or(|path| !path.is_root() || path.root() != witness.symbol())
                    || fact.moved_span() != Some(leaf.span())
                    || !owned
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
            ParameterMode::Inout => unreachable!("mode was rejected above"),
        }
        Ok(Some(NullableWhenArgument {
            witness,
            proof,
            operands,
            fact: fact.clone(),
        }))
    }

    pub(super) fn lower_nullable_when_borrow(
        &self,
        call: UnitExpressionId,
        argument: ExpressionId,
        fact: &UnitLoanFact,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        if let UnitLoanTarget::Place(path) = fact.target()
            && !path.is_root()
            && self
                .nullable_proofs
                .get(&path.root())
                .is_some_and(|proof| proof.when_scope().is_some())
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let argument = UnitExpressionId::new(self.source_unit, argument);
        let Some(prepared) =
            self.prepare_nullable_when_argument(call, argument, ParameterMode::Borrow, span)?
        else {
            return Ok(None);
        };
        if prepared.fact.loan() != Some(fact) || prepared.proof.inner != target {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // The persistent When scope owns this view. A synchronous callee must not
        // count it as a newly-created child, Take it, or implicitly retain its Rc.
        Ok(Some(prepared.proof.loan))
    }

    pub(super) fn lower_nullable_when_value(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        parameter_type: UnitTypeId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let argument = UnitExpressionId::new(self.source_unit, argument);
        let Some(prepared) =
            self.prepare_nullable_when_argument(call, argument, ParameterMode::Value, span)?
        else {
            return Ok(None);
        };
        let expected = resolve_concrete_type(
            self.typed,
            parameter_type,
            self.substitutions,
            self.static_self,
            span,
        )?;
        if self.type_ids.get(&expected) != Some(&prepared.proof.inner) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_when_retirement(prepared.witness.symbol(), span)?;
        self.preflight_nullable_take_drops(&prepared.operands, span)?;
        let (_, values) = self
            .function
            .append_instruction(
                self.block,
                Operation::NullableTake {
                    owner: prepared.proof.owner,
                    proof: prepared.proof.loan,
                },
                vec![EntityType::Value(prepared.proof.inner)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let inner = require_value(values[0], span)?;
        // Every fallible fact/storage check preceded Take. It consumes the view;
        // there is deliberately no BorrowEnd, no wrapper Drop and no second transfer.
        self.nullable_proofs.remove(&prepared.witness.symbol());
        self.bindings.remove(&prepared.witness.symbol());
        self.temporaries
            .retain(|_, owner| *owner != prepared.proof.owner);
        // The original Name/Group cleanup points were all checked empty before
        // Take. No fact consumer, including result/closure/Map cleanup, runs here.
        // The original argument frame now registers this inner as its pending Value,
        // and removes that obligation only at the original successful call commit.
        Ok(Some(LoweredValue::Value(inner)))
    }

    pub(super) fn emit_nullable_when_borrow_drops(
        &mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
    ) -> Result<bool, LoweringError> {
        let Some(fact) = self.owned.nullable_when_use(argument) else {
            return Ok(false);
        };
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(argument.expression())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        if fact.call() != call || fact.kind() != UnitNullableWhenUseKind::Observe {
            return Err(lowering_error(LoweringErrorKind::MissingFact, node.span()));
        }
        let (_, _, operands) = self.validate_nullable_when_use(argument, node.span())?;
        self.emit_nullable_when_operand_drops(&operands)?;
        Ok(true)
    }
}
