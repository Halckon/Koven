//! Integer Range loops carry endpoint Values, independently of borrowed providers.
use super::super::{call_lifetimes::PendingCallFrame, nullable_proof::NullableProof};
use super::{
    iteration::{IterationContext, IterationState},
    *,
};
use crate::ssa::model::{
    BinaryOperator, ComparisonOperator, EntityId, EntityType, LoanKind, Operation, ScalarConstant,
    SsaTypeId, SsaTypeKind, ValueId,
};
use lang_frontend::{
    ownership_checking::{IterationSource, UnitIterationOwnershipPlan, UnitIterationSourceAccess},
    type_checking::{
        BuiltinType, Copyability, IterationProvider, UnitSequentialIterationBinding, UnitTypeKind,
    },
};

/// Qualification for this numeric loop only. This records provenance, never an
/// owner/loan pair to reinstall after a body path has consumed or retired it.
struct NumericWhileProof {
    root: UnitSymbolId,
    proof: NullableProof,
    endpoint_type: SsaTypeId,
    statement: UnitStatementId,
    entry_call_frames: Vec<PendingCallFrame>,
}

impl NumericWhileProof {
    fn qualify(
        lowerer: &UnitExpressionLowerer<'_>,
        statement: UnitStatementId,
        endpoint_type: SsaTypeId,
        span: Span,
    ) -> Result<Option<Self>, LoweringError> {
        let Some((&root, &proof)) = lowerer.nullable_proofs.first_key_value() else {
            return Ok(None);
        };
        if lowerer.nullable_proofs.len() != 1
            || proof.while_scope().is_none_or(|scope| {
                !lowerer
                    .loops
                    .iter()
                    .any(|context| context.proof_scope == Some(scope))
            })
            || lowerer.borrow_bindings.contains_key(&root)
            || !lowerer.temporaries.is_empty()
            || !matches!(
                lowerer.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let storage = lowerer
            .typed
            .symbol_type(root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let declared = super::super::resolve_concrete_type(
            lowerer.typed,
            storage,
            lowerer.substitutions,
            lowerer.static_self,
            span,
        )?;
        let Some(UnitTypeKind::Nullable(inner)) = lowerer.typed.types().get(declared) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if lowerer.type_ids.get(inner) != Some(&proof.inner)
            || lowerer.typed.copyability(declared) != Copyability::MoveOnly
            || lowerer.typed.copyability(*inner) != Copyability::MoveOnly
            || lowerer
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != lowerer
                    .type_ids
                    .get(&declared)
                    .copied()
                    .map(EntityType::Value)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let qualified = Self {
            root,
            proof,
            endpoint_type,
            statement,
            entry_call_frames: lowerer.pending_call_frames.clone(),
        };
        qualified.validate(lowerer, &lowerer.loop_state(), false, span)?;
        // Run the existing strict gate before this numeric provider adds its two
        // private endpoint slots. It still rejects children and other providers.
        lowerer.ensure_no_while_proof_carriers(span)?;
        Ok(Some(qualified))
    }

    fn validate(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        state: &BranchExit,
        endpoints: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        lowerer.validate_readonly_while_exit(self.root, self.proof, state, span)?;
        // The entry gate allows no pending prefix. Only this provider's two
        // known endpoint Values may be live on a continuing/internal edge.
        if state.pending_operands.len() != (if endpoints { 2 } else { 0 })
            || state.pending_operands.iter().any(|entity| {
                !matches!(entity, EntityId::Value(_))
                    || lowerer.function.entity(*entity).map(|data| data.ty)
                        != Some(EntityType::Value(self.endpoint_type))
            })
            || state.pending_call_frames != self.entry_call_frames
            || !state.temporaries.is_empty()
            || !state.capture_loans.is_empty()
            || !state.result_source_loans.is_empty()
            || state.receiver.is_some()
            || !lowerer.temporary_range_slots.is_empty()
            || !lowerer.short_range_ends.is_empty()
            || lowerer.loops.iter().any(|context| {
                context.iteration.as_ref().is_some_and(|iteration| {
                    iteration.statement != self.statement
                        || context.proof_scope.is_some()
                        || !matches!(iteration.state,
                            IterationState::Numeric { current: 0, last: 1, endpoint_type }
                                if endpoint_type == self.endpoint_type)
                })
            })
            || state.borrow_bindings.iter().any(|(root, &loan)| {
                !lowerer.entry_boolean_roots.contains(root)
                    || !matches!(lowerer.function.entity(EntityId::Loan(loan)).map(|data| data.ty),
                        Some(EntityType::Loan { kind: LoanKind::Shared, target })
                            if matches!(lowerer.ssa_types.get(target.index()), Some(SsaTypeKind::Boolean)))
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_numeric_for(
        &mut self,
        plan: &UnitIterationOwnershipPlan,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = plan.descriptor();
        let endpoint = match descriptor.provider() {
            IterationProvider::NumericRange(endpoint @ (BuiltinType::Int | BuiltinType::Long)) => {
                endpoint
            }
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let endpoint_type = self.iteration_type(descriptor.element_type(), span)?;
        if plan.source_access() != UnitIterationSourceAccess::Owned
            || plan.source_kind() != &IterationSource::OwnedSnapshot(descriptor.source())
            || endpoint_type != self.ssa_builtin(endpoint, span)?
            || matches!(
                descriptor.binding(),
                UnitSequentialIterationBinding::Destructure(_)
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let inherited =
            NumericWhileProof::qualify(self, descriptor.statement(), endpoint_type, span)?;
        let range_type = self.iteration_type(descriptor.source_type(), span)?;
        let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
        // Evaluate the real source once, including its AfterExpression cleanup, before
        // acquiring this provider. Copyable projections detach the iteration snapshot.
        let source = match self.lower(descriptor.source().expression())? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        if self
            .function
            .entity(EntityId::Value(source))
            .map(|value| value.ty)
            != Some(EntityType::Value(range_type))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if let Some(proof) = &inherited {
            // Source evaluation may contain CFG. Accept only its surviving current
            // pair, and recheck carriers before adding either endpoint slot.
            proof.validate(self, &self.loop_state(), false, span)?;
            self.ensure_no_while_proof_carriers(span)?;
        }
        let first = self.numeric_iteration_value(
            Operation::AggregateProject {
                aggregate: source,
                field: 0,
            },
            endpoint_type,
            span,
        )?;
        let last = self.numeric_iteration_value(
            Operation::AggregateProject {
                aggregate: source,
                field: 1,
            },
            endpoint_type,
            span,
        )?;
        let empty = self.numeric_iteration_value(
            Operation::AggregateProject {
                aggregate: source,
                field: 2,
            },
            boolean,
            span,
        )?;
        let retained_start = self.pending_operands.len();
        let outer_temporaries = self.temporaries.keys().copied().collect();
        self.pending_operands
            .extend([EntityId::Value(first), EntityId::Value(last)]);
        let baseline = self.bindings.clone();
        let mut context = if inherited.is_some() {
            self.create_carried_loop_context(&baseline, span)?
        } else {
            self.create_loop_context(&baseline, span)?
        };
        // This provider only transports the outer view. Its own jumps must not
        // retire that view; the owning nullable while keeps that responsibility.
        if inherited.is_some() && context.proof_scope.is_some() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let exhausted = self.add_carried_control_block(&context.carried, &context.loans, span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: empty,
                    when_true: carried_control_edge(exhausted, &context.carried, &context.loans),
                    when_false: carried_control_edge(
                        context.header,
                        &context.carried,
                        &context.loans,
                    ),
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let exhaustion_state = if let Some(proof) = &inherited {
            let entry_state = self.loop_state();
            // Save the real empty-edge parameters before lowering the sibling
            // body. Later terminal edges must validate the same provenance.
            self.block = exhausted;
            self.bindings = self.rebind_loop(&baseline, exhausted, &context, span)?;
            proof.validate(self, &self.loop_state(), true, span)?;
            let state = self.loop_state();
            self.restore_loop_state(&entry_state);
            Some(state)
        } else {
            None
        };
        self.block = context.header;
        self.bindings = self.rebind_loop(&baseline, self.block, &context, span)?;
        self.closure_bindings = context.entry_closure_bindings.clone();
        if let Some(proof) = &inherited {
            proof.validate(self, &self.loop_state(), true, span)?;
        }
        context.iteration = Some(IterationContext {
            statement: descriptor.statement(),
            retained_start,
            outer_temporaries,
            state: IterationState::Numeric {
                current: retained_start,
                last: retained_start + 1,
                endpoint_type,
            },
        });
        if let UnitSequentialIterationBinding::Name(symbol) = descriptor.binding() {
            let current = self.numeric_pending_value(retained_start, endpoint_type, span)?;
            let element = self.numeric_iteration_value(
                Operation::Copy { source: current },
                endpoint_type,
                span,
            )?;
            self.bindings.insert(*symbol, LoweredValue::Value(element));
        }
        self.loops.push(context);
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        if let Some(proof) = &inherited {
            for jump in &context.continues {
                proof.validate(self, &jump.state, true, jump.span)?;
            }
            for jump in &context.breaks {
                proof.validate(self, &jump.state, false, jump.span)?;
            }
        }
        self.finish_numeric_continues(&context, &baseline, exhausted, inherited.as_ref(), span)?;

        if let Some(state) = exhaustion_state {
            // Only restore the actual empty-edge state after all body exits have
            // proved they retained the view; never revive an entry owner/loan pair.
            self.restore_loop_state(&state);
        } else {
            self.block = exhausted;
            self.current_receiver = context.entry_receiver;
            self.consumed_receiver = context.entry_consumed_receiver;
            self.bindings = self.rebind_loop(&baseline, exhausted, &context, span)?;
            self.closure_bindings = context.entry_closure_bindings.clone();
        }
        self.loops.push(context);
        self.emit_drops(UnitDropPoint::LoopExit(descriptor.statement()))?;
        self.pending_operands.truncate(retained_start);
        let exhaustion = self.loop_state();
        if let Some(proof) = &inherited {
            proof.validate(self, &exhaustion, false, span)?;
        }
        let context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let mut exits = context
            .breaks
            .into_iter()
            .map(branch_exit)
            .collect::<Vec<_>>();
        exits.push(exhaustion);
        if let Some(proof) = &inherited {
            for exit in &exits {
                proof.validate(self, exit, false, span)?;
            }
        }
        self.merge_loop_exits(exits, span)?;
        if let Some(proof) = &inherited {
            proof.validate(self, &self.loop_state(), false, span)?;
        }
        Ok(LoweredValue::Unit)
    }

    fn finish_numeric_continues(
        &mut self,
        context: &LoopContext,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        exhausted: BlockId,
        inherited: Option<&NumericWhileProof>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if context.continues.is_empty() {
            return Ok(());
        }
        let Some(IterationContext {
            state:
                IterationState::Numeric {
                    current,
                    last,
                    endpoint_type,
                },
            ..
        }) = context.iteration.as_ref()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let (current_slot, last_slot, endpoint_type) = (*current, *last, *endpoint_type);
        let continuation =
            self.add_carried_control_block(&context.carried, &context.loans, span)?;
        self.finish_continues(context, continuation)?;
        if let Some(proof) = inherited {
            let first = context
                .continues
                .first()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            proof.validate(self, &first.state, true, first.span)?;
            self.restore_loop_state(&first.state);
        }
        self.block = continuation;
        self.current_receiver = context.entry_receiver;
        self.consumed_receiver = context.entry_consumed_receiver;
        self.bindings = self.rebind_loop(baseline, continuation, context, span)?;
        self.closure_bindings = context.entry_closure_bindings.clone();
        if let Some(proof) = inherited {
            proof.validate(self, &self.loop_state(), true, span)?;
        }
        let current = self.numeric_pending_value(current_slot, endpoint_type, span)?;
        let last = self.numeric_pending_value(last_slot, endpoint_type, span)?;
        let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
        let terminal = self.numeric_iteration_value(
            Operation::Compare {
                operator: ComparisonOperator::Equal,
                left: current,
                right: last,
            },
            boolean,
            span,
        )?;
        let increment = self.add_carried_control_block(&context.carried, &context.loans, span)?;
        let state = self.loop_state();
        if let Some(proof) = inherited {
            proof.validate(self, &state, true, span)?;
        }
        let when_true =
            self.carried_edge_from(exhausted, &context.carried, &context.loans, &state, span)?;
        let when_false =
            self.carried_edge_from(increment, &context.carried, &context.loans, &state, span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: terminal,
                    when_true,
                    when_false,
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        self.block = increment;
        self.bindings = self.rebind_loop(baseline, increment, context, span)?;
        if let Some(proof) = inherited {
            proof.validate(self, &self.loop_state(), true, span)?;
        }
        let current = self.numeric_pending_value(current_slot, endpoint_type, span)?;
        let one = self.numeric_iteration_value(
            Operation::Constant(ScalarConstant::Integer(1)),
            endpoint_type,
            span,
        )?;
        // A nonempty ascending range reaches this edge only when current < last.
        // The endpoint-width increment therefore cannot overflow, including MAX..MAX.
        let next = self.numeric_iteration_value(
            Operation::Binary {
                operator: BinaryOperator::Add,
                left: current,
                right: one,
            },
            endpoint_type,
            span,
        )?;
        self.pending_operands[current_slot] = EntityId::Value(next);
        let state = self.loop_state();
        if let Some(proof) = inherited {
            proof.validate(self, &state, true, span)?;
        }
        let edge = self.carried_edge_from(
            context.header,
            &context.carried,
            &context.loans,
            &state,
            span,
        )?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(edge),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(())
    }

    pub(super) fn numeric_pending_value(
        &self,
        index: usize,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        match self.pending_operands.get(index).copied() {
            Some(EntityId::Value(value))
                if self
                    .function
                    .entity(EntityId::Value(value))
                    .map(|value| value.ty)
                    == Some(EntityType::Value(ty)) =>
            {
                Ok(value)
            }
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }

    fn numeric_iteration_value(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        match self.append_scalar(operation, ty, span)? {
            LoweredValue::Value(value) => Ok(value),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }
}
