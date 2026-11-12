//! compilation-unit loop 的 owner-aware 回边、jump 与共同退出 lowering。

mod iteration;
use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};

use super::{
    LoweredValue, UnitExpressionLowerer,
    cfg::{BranchExit, CarriedAccess, CarriedBinding, carried_control_edge},
    lowering_error,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{BlockId, Definition, EntityId, Operation, Origin, ScalarConstant, TerminatorKind},
};

struct LoopJump {
    state: BranchExit,
    span: Span,
}

pub(super) struct LoopContext {
    iteration: Option<iteration::IterationContext>,
    header: BlockId,
    carried: Vec<CarriedBinding>,
    loans: Vec<CarriedAccess>,
    entry_temporaries: BTreeSet<UnitExpressionId>,
    entry_pending_count: usize,
    entry_receiver: Option<super::ReceiverBinding>,
    entry_consumed_receiver: Option<super::ConsumedReceiver>,
    entry_symbols: BTreeSet<UnitSymbolId>,
    continues: Vec<LoopJump>,
    breaks: Vec<LoopJump>,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn current_loop_exit_counts(&self) -> (usize, usize) {
        self.loops.last().map_or((0, 0), |context| {
            (context.breaks.len(), context.continues.len())
        })
    }

    pub(super) fn has_iteration_context(&self) -> bool {
        self.loops.iter().any(|context| context.iteration.is_some())
    }

    pub(super) fn lower_while(
        &mut self,
        statement: StatementId,
        condition: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.reject_live_if_proof(span)?;
        // Nullable `while` proofs lower only in `ssa::refine` (SPEC-0395).
        if !self.nullable_proofs.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if !self.supports_loop_control_prefix(span) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let baseline = self.bindings.clone();
        let context = self.create_loop_context(&baseline, span)?;
        let header = context.header;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(carried_control_edge(
                    header,
                    &context.carried,
                    &context.loans,
                )),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        self.block = header;
        self.current_receiver = context.entry_receiver;
        self.consumed_receiver = context.entry_consumed_receiver;
        self.bindings = self.rebind_loop(&baseline, header, &context, span)?;
        let ordinary_condition = self.constant_owned.is_none().then(|| {
            self.typed
                .ordinary_boolean_outcomes(UnitExpressionId::new(self.source_unit, condition))
        });
        let Some(condition) = self.lower_optional_condition_value(condition, span)? else {
            return Ok(LoweredValue::Diverged);
        };
        if !self.supports_loop_control_prefix(span) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let constant_condition = if let Some(entity) =
            self.function.entity(EntityId::Value(condition))
            && let Definition::InstructionResult { instruction, .. } = entity.definition
            && let Some(instruction) = self.function.instruction(instruction)
            && let Operation::Constant(ScalarConstant::Boolean(value)) = instruction.operation
        {
            Some(value)
        } else {
            match ordinary_condition {
                Some((true, false)) => Some(true),
                Some((false, true)) => Some(false),
                _ => None,
            }
        };
        if constant_condition == Some(false) {
            self.emit_loop_exit(statement)?;
            return Ok(LoweredValue::Unit);
        }
        let condition_state = self.loop_state();
        let body_block = self.add_carried_control_block(
            &context.carried,
            &context.loans,
            self.statement_span(body)?,
        )?;
        let when_true = self.carried_edge_from(
            body_block,
            &context.carried,
            &context.loans,
            &condition_state,
            span,
        )?;
        let (terminator, false_block) = if constant_condition == Some(true) {
            (TerminatorKind::Branch(when_true), None)
        } else {
            let block = self.add_carried_control_block(&context.carried, &context.loans, span)?;
            let when_false = self.carried_edge_from(
                block,
                &context.carried,
                &context.loans,
                &condition_state,
                span,
            )?;
            (
                TerminatorKind::Conditional {
                    condition,
                    when_true,
                    when_false,
                },
                Some(block),
            )
        };
        self.function
            .set_terminator(self.block, terminator, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        // Each reachable successor starts from the complete condition state.
        let false_exit = if let Some(false_block) = false_block {
            self.restore_loop_state(&condition_state);
            self.block = false_block;
            self.bindings =
                self.rebind_loop(&condition_state.bindings, false_block, &context, span)?;
            Some(self.loop_state())
        } else {
            None
        };

        self.restore_loop_state(&condition_state);
        self.block = body_block;
        self.bindings = self.rebind_loop(&condition_state.bindings, body_block, &context, span)?;
        self.loops.push(context);
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self.loops.pop().expect("while context must be balanced");
        self.finish_continues(&context, context.header)?;

        let mut exits = context
            .breaks
            .into_iter()
            .map(branch_exit)
            .collect::<Vec<_>>();
        exits.extend(false_exit);
        if exits.is_empty() {
            return self.merge_unit_exits(exits, span);
        }
        self.merge_loop_exits(exits, span)?;
        self.emit_loop_exit(statement)?;
        Ok(LoweredValue::Unit)
    }

    pub(super) fn lower_loop(
        &mut self,
        statement: StatementId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.reject_live_if_proof(span)?;
        if !self.nullable_proofs.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if !self.supports_loop_control_prefix(span) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let baseline = self.bindings.clone();
        let context = self.create_loop_context(&baseline, span)?;
        let header = context.header;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(carried_control_edge(
                    header,
                    &context.carried,
                    &context.loans,
                )),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = header;
        self.current_receiver = context.entry_receiver;
        self.consumed_receiver = context.entry_consumed_receiver;
        self.bindings = self.rebind_loop(&baseline, header, &context, span)?;
        self.loops.push(context);
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self.loops.pop().expect("loop context must be balanced");
        self.finish_continues(&context, context.header)?;
        if context.breaks.is_empty() {
            self.current_receiver = None;
            self.consumed_receiver = None;
            self.bindings.clear();
            self.borrow_bindings.clear();
            self.nullable_proofs.clear();
            self.temporaries.clear();
            self.pending_operands.clear();
            return Ok(LoweredValue::Diverged);
        }
        self.merge_loop_exits(context.breaks.into_iter().map(branch_exit).collect(), span)?;
        self.emit_loop_exit(statement)?;
        Ok(LoweredValue::Unit)
    }

    pub(super) fn lower_break(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.reject_live_if_proof(span)?;
        if self
            .owned
            .iteration_cleanup_at(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                self.source_unit,
                expression,
            )))
            .is_none()
        {
            self.end_pending_call_loans(self.loops.len(), span)?;
        }
        self.emit_drops(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))?;
        self.retire_nullable_when_transfers(self.loops.len(), span)?;
        self.prepare_jump(span, true)?;
        let jump = self.current_jump(span)?;
        self.loops
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?
            .breaks
            .push(jump);
        Ok(LoweredValue::Diverged)
    }

    pub(super) fn lower_continue(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.reject_live_if_proof(span)?;
        if self
            .owned
            .iteration_cleanup_at(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                self.source_unit,
                expression,
            )))
            .is_none()
        {
            self.end_pending_call_loans(self.loops.len(), span)?;
        }
        self.emit_drops(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))?;
        self.advance_iteration(span)?;
        self.retire_nullable_when_transfers(self.loops.len(), span)?;
        self.prepare_jump(span, false)?;
        let jump = self.current_jump(span)?;
        self.loops
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?
            .continues
            .push(jump);
        Ok(LoweredValue::Diverged)
    }

    fn create_loop_context(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LoopContext, LoweringError> {
        if !self.nullable_proofs.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let mut carried = self.carried_bindings(baseline, span)?;
        let mut loans = self.carried_loans(&self.borrow_bindings, span)?;
        self.carry_pending_operands(&mut carried, &mut loans, span)?;
        self.separate_loop_pending_copies(&mut carried, span)?;
        let header = self.add_carried_control_block(&carried, &loans, span)?;
        Ok(LoopContext {
            iteration: None,
            header,
            carried,
            loans,
            entry_temporaries: self.temporaries.keys().copied().collect(),
            entry_pending_count: self.pending_operands.len(),
            entry_receiver: self.current_receiver,
            entry_consumed_receiver: self.consumed_receiver,
            entry_symbols: baseline.keys().copied().collect(),
            continues: Vec::new(),
            breaks: Vec::new(),
        })
    }

    fn prepare_jump(&mut self, span: Span, breaking: bool) -> Result<(), LoweringError> {
        let entry_symbols = self
            .loops
            .last()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?
            .entry_symbols
            .clone();
        let context = self
            .loops
            .last()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (temporaries, pending_count) = match (&context.iteration, breaking) {
            (Some(iteration), true) => (&iteration.outer_temporaries, iteration.retained_start),
            _ => (&context.entry_temporaries, context.entry_pending_count),
        };
        if self
            .temporaries
            .keys()
            .copied()
            .ne(temporaries.iter().copied())
            || self.pending_operands.len() < pending_count
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        // Continue retains this provider's state; break keeps only its outer frame
        // after consuming any source-loan and hidden-owner cleanup.
        self.pending_operands.truncate(pending_count);
        self.discard_non_entry_bindings(&entry_symbols, span)
    }

    fn current_jump(&self, span: Span) -> Result<LoopJump, LoweringError> {
        if self.loops.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let mut state = self.loop_state();
        // Cleanup already ended cancelled ordinary call loans. Their Rust frames
        // still unwind in the caller, but must not reappear in the loop successor.
        state
            .pending_call_frames
            .retain(|frame| frame.loop_depth < self.loops.len());
        Ok(LoopJump { state, span })
    }

    pub(super) fn loop_state(&self) -> BranchExit {
        BranchExit {
            block: self.block,
            result: LoweredValue::Unit,
            receiver: self.current_receiver,
            consumed_receiver: self.consumed_receiver,
            bindings: self.bindings.clone(),
            borrow_bindings: self.borrow_bindings.clone(),
            nullable_proofs: self.nullable_proofs.clone(),
            pending_operands: self.pending_operands.clone(),
            pending_call_frames: self.pending_call_frames.clone(),
            temporaries: self.temporaries.clone(),
        }
    }

    pub(super) fn restore_loop_state(&mut self, state: &BranchExit) {
        self.block = state.block;
        self.bindings.clone_from(&state.bindings);
        self.borrow_bindings.clone_from(&state.borrow_bindings);
        self.nullable_proofs.clone_from(&state.nullable_proofs);
        self.current_receiver = state.receiver;
        self.consumed_receiver = state.consumed_receiver;
        self.pending_operands.clone_from(&state.pending_operands);
        self.pending_call_frames
            .clone_from(&state.pending_call_frames);
        self.temporaries.clone_from(&state.temporaries);
    }

    fn rebind_loop(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        context: &LoopContext,
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        let bindings =
            self.rebind_carried_control(baseline, block, &context.carried, &context.loans, span)?;
        self.borrow_bindings =
            self.rebind_carried_loans(block, context.carried.len(), &context.loans, span)?;
        Ok(bindings)
    }

    fn record_natural_continue(&mut self, span: Span) -> Result<(), LoweringError> {
        self.advance_iteration(span)?;
        self.prepare_jump(span, false)?;
        let jump = self.current_jump(span)?;
        self.loops
            .last_mut()
            .expect("natural continue has a loop context")
            .continues
            .push(jump);
        Ok(())
    }

    fn finish_continues(
        &mut self,
        context: &LoopContext,
        target: BlockId,
    ) -> Result<(), LoweringError> {
        for jump in &context.continues {
            if jump.state.consumed_receiver != context.entry_consumed_receiver {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    jump.span,
                ));
            }
            let edge = self.carried_edge_from(
                target,
                &context.carried,
                &context.loans,
                &jump.state,
                jump.span,
            )?;
            self.function
                .set_terminator(
                    jump.state.block,
                    TerminatorKind::Branch(edge),
                    Origin::Source(jump.span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, jump.span))?;
        }
        Ok(())
    }

    fn emit_loop_exit(&mut self, statement: StatementId) -> Result<(), LoweringError> {
        let point = UnitDropPoint::LoopExit(UnitStatementId::new(self.source_unit, statement));
        self.emit_loop_exit_drops(point, self.statement_span(statement)?)
    }

    fn merge_loop_exits(
        &mut self,
        exits: Vec<BranchExit>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(first) = exits.first() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        if exits.iter().skip(1).any(|exit| {
            exit.bindings.keys().collect::<Vec<_>>() != first.bindings.keys().collect::<Vec<_>>()
                || exit.bindings.iter().any(|(symbol, binding)| {
                    first.bindings.get(symbol).is_none_or(|first| {
                        std::mem::discriminant(first) != std::mem::discriminant(binding)
                    })
                })
        }) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.merge_unit_exits(exits, span)?;
        Ok(())
    }
}

fn branch_exit(jump: LoopJump) -> BranchExit {
    jump.state
}

pub(in crate::ssa::unit_lower) mod heap_nullable_if {
    use super::super::control::conditional::nullable_if::NullableIfEntry;
    use super::super::*;
    use crate::ssa::model::SsaTypeKind;
    use lang_frontend::{
        ownership_checking::{
            LoanKind as FrontendLoanKind, UnitCallArgumentOwnershipKind, UnitLoanTarget,
        },
        parser::BinaryOperator,
        type_checking::{
            CallableResultSource, ParameterMode, UnitCallTarget, UnitNullComparisonDescriptor,
            UnitTypeId,
        },
    };

    #[derive(Clone, Copy)]
    pub(in crate::ssa::unit_lower) struct NamedNullableIfRoot {
        pub(in crate::ssa::unit_lower) symbol: UnitSymbolId,
        pub(in crate::ssa::unit_lower) declared: UnitTypeId,
        pub(in crate::ssa::unit_lower) narrowed: UnitTypeId,
    }

    impl UnitExpressionLowerer<'_> {
        pub(in crate::ssa::unit_lower) fn prepare_heap_nullable_if_entry(
            &self,
            control: UnitExpressionId,
            condition: ExpressionId,
            span: Span,
        ) -> Result<Option<NullableIfEntry>, LoweringError> {
            if !self.nullable_proofs.is_empty()
                || !self.loops.is_empty()
                || !self.substitutions.is_empty()
            {
                return Ok(None);
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(control.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let Expression::If {
                condition: actual,
                then_branch,
                else_branch: Some(else_branch),
                ..
            } = *node.payload()
            else {
                return Ok(None);
            };
            if control.source_unit() != self.source_unit
                || actual != condition
                || node.span() != span
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let Some(comparison) = self.map_borrow_condition(condition, span)? else {
                return Ok(None);
            };
            let root = comparison.symbol();
            let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
                return Ok(None);
            };
            let Some(EntityType::Value(storage)) = self
                .function
                .entity(EntityId::Value(owner))
                .map(|data| data.ty)
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let Some(SsaTypeKind::NullableHandle { inner }) = self.ssa_types.get(storage.index())
            else {
                return Ok(None);
            };
            if !matches!(
                self.ssa_types.get(inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            ) || self.borrow_bindings.contains_key(&root)
            {
                return Ok(None);
            }
            let Some(UnitTypeKind::Nullable(narrowed)) =
                self.typed.types().get(comparison.nullable_type())
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let root = NamedNullableIfRoot {
                symbol: root,
                declared: comparison.nullable_type(),
                narrowed: *narrowed,
            };
            let (non_null, null) = if comparison.non_null_when_true() {
                (then_branch, else_branch)
            } else {
                (else_branch, then_branch)
            };
            if !self.map_if_body_is_empty(Some(null), span)? {
                return Ok(None);
            }
            let Some(call) = self.heap_if_direct_borrow_call(non_null, span)? else {
                return Ok(None);
            };
            if !self.heap_if_source_context(control, span)?
                || !self.heap_if_call_qualifies(call, root, span)?
            {
                return Ok(None);
            }
            self.validate_named_nullable_if_condition(condition, comparison, root, span)?;
            let (nullable, inner) = self.heap_nullable_storage(comparison, span)?;
            if storage != nullable
                || self.type_ids.get(&root.narrowed) != Some(&inner)
                || self.typed.copyability(root.declared) != Copyability::MoveOnly
                || self.typed.copyability(root.narrowed) != Copyability::MoveOnly
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.ensure_no_while_proof_carriers(span)?;
            if !self.pending_call_frames.is_empty() || !self.pending_operands.is_empty() {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            Ok(Some(NullableIfEntry {
                root: root.symbol,
                owner,
                inner,
                scope: control,
                non_null_when_true: comparison.non_null_when_true(),
            }))
        }

        fn heap_nullable_storage(
            &self,
            descriptor: UnitNullComparisonDescriptor,
            span: Span,
        ) -> Result<(SsaTypeId, SsaTypeId), LoweringError> {
            if !self.temporaries.is_empty() {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let symbol = descriptor.symbol();
            let nullable = resolve_concrete_type(
                self.typed,
                descriptor.nullable_type(),
                self.substitutions,
                self.static_self,
                span,
            )?;
            let nullable = *self
                .type_ids
                .get(&nullable)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let Some(SsaTypeKind::NullableHandle { inner }) = self.ssa_types.get(nullable.index())
            else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            let inner = *inner;
            if !matches!(
                self.ssa_types.get(inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            ) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let Some(LoweredValue::Value(entry_owner)) = self.bindings.get(&symbol).copied() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            if self
                .function
                .entity(EntityId::Value(entry_owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(nullable))
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            Ok((nullable, inner))
        }

        fn heap_if_source_context(
            &self,
            control: UnitExpressionId,
            span: Span,
        ) -> Result<bool, LoweringError> {
            let key = self
                .source_plan
                .source(self.source_token)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if self.function_ids.get(key) != Some(&self.function.id()) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            if key.deinit_owner().is_some() || !key.type_arguments().is_empty() {
                return Ok(false);
            }
            let UnitCallableTarget::Declaration(target) = key.target() else {
                return Ok(false);
            };
            let declaration = self
                .names
                .names()
                .index()
                .declarations()
                .get(target.index())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if declaration.id() != target
                || declaration.source_unit() != self.source_unit
                || declaration.name_span().source_id() != self.parsed.source_id()
                || span.source_id() != self.parsed.source_id()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let (_, item, _) = unwrap_modified(self.parsed, declaration.root())?;
            let Item::Function { .. } = item else {
                return Ok(false);
            };
            Ok(!self.parsed.ast().expressions().iter().any(|(id, node)| {
                id != control.expression()
                    && crate::ssa::unit_lower::span_contains(node.span(), span)
                    && !matches!(
                        node.payload(),
                        Expression::Group { .. } | Expression::If { .. } | Expression::When { .. }
                    )
            }))
        }

        fn heap_if_direct_borrow_call(
            &self,
            body: StatementId,
            span: Span,
        ) -> Result<Option<ExpressionId>, LoweringError> {
            let node = self
                .parsed
                .ast()
                .statements()
                .get(body)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let Statement::ControlBody { elements } = node.payload() else {
                return Ok(None);
            };
            let [statement] = elements.as_slice() else {
                return Ok(None);
            };
            let node = self
                .parsed
                .ast()
                .statements()
                .get(*statement)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let Statement::Expression { expression } = *node.payload() else {
                return Ok(None);
            };
            let expression = self.ungroup_map_if_expression(expression, span)?;
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            Ok(matches!(node.payload(), Expression::Call { .. }).then_some(expression))
        }

        pub(in crate::ssa::unit_lower) fn validate_named_nullable_if_condition(
            &self,
            condition: ExpressionId,
            descriptor: UnitNullComparisonDescriptor,
            root: NamedNullableIfRoot,
            span: Span,
        ) -> Result<(), LoweringError> {
            let comparison = self.ungroup_map_if_expression(condition, span)?;
            if descriptor.expression() != UnitExpressionId::new(self.source_unit, comparison)
                || root.symbol.source_unit() != self.source_unit
                || self.typed.symbol_type(root.symbol) != Some(root.declared)
                || self.expression_builtin_type(condition, span)? != Some(BuiltinType::Boolean)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let nodes = self.parsed.ast().expressions();
            let node = nodes
                .get(comparison)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Binary {
                left,
                operator,
                right,
                ..
            } = *node.payload()
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            if !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
                || descriptor.non_null_when_true() != (operator == BinaryOperator::NotEqual)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let left_leaf = self.ungroup_map_if_expression(left, span)?;
            let right_leaf = self.ungroup_map_if_expression(right, span)?;
            let is_null = |id| {
                nodes.get(id).is_ok_and(|node| {
                    matches!(node.payload(), Expression::Literal(LiteralKind::Null))
                })
            };
            let subject = match (is_null(left_leaf), is_null(right_leaf)) {
                (true, false) => right,
                (false, true) => left,
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            if !self.heap_if_root_operand(subject, root, false, span)? {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            Ok(())
        }

        fn heap_if_call_qualifies(
            &self,
            expression: ExpressionId,
            root: NamedNullableIfRoot,
            span: Span,
        ) -> Result<bool, LoweringError> {
            let call = UnitExpressionId::new(self.source_unit, expression);
            let nodes = self.parsed.ast().expressions();
            let node = nodes
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                return Ok(false);
            };
            let callee = self.ungroup_map_if_expression(*callee, span)?;
            if !nodes
                .get(callee)
                .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            {
                return Ok(false);
            }
            let descriptor = self
                .typed
                .call(call)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let UnitCallTarget::Declaration(target) = descriptor.target() else {
                return Ok(false);
            };
            if descriptor.receiver().is_some()
                || descriptor.aborts()
                || descriptor.prints_line()
                || descriptor.result_source() != CallableResultSource::Owned
                || !descriptor.instance().type_arguments().is_empty()
                || self.typed.construction(call).is_some()
                || self.typed.ownership_primitive(call).is_some()
                || arguments.is_empty()
                || descriptor.arguments().iter().any(|mapping| {
                    mapping.mode() != ParameterMode::Borrow || mapping.crosses_thread()
                })
                || self.typed.copyability(descriptor.return_type()) != Copyability::Copyable
                || builtin_type(self.typed, descriptor.return_type()) == Some(BuiltinType::Nothing)
            {
                return Ok(false);
            }
            let route = self
                .source_plan
                .call_site(self.source_token, call)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let Some(key) = route.dispatch_key() else {
                return Ok(false);
            };
            if descriptor.expression() != call
                || descriptor.arguments().len() != arguments.len()
                || key.target() != UnitCallableTarget::Declaration(target)
                || !key.type_arguments().is_empty()
                || !self.function_ids.contains_key(key)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            for (index, argument) in arguments.iter().enumerate() {
                if argument.mode_marker.is_some()
                    || !self.heap_if_root_operand(argument.value, root, true, argument.span)?
                {
                    return Ok(false);
                }
                let mut mappings = descriptor
                    .arguments()
                    .iter()
                    .filter(|mapping| mapping.argument_index() == index);
                let Some(mapping) = mappings.next() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, argument.span));
                };
                if mappings.next().is_some()
                    || mapping.category() != ExpressionCategory::Place
                    || mapping.parameter_type() != root.narrowed
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, argument.span));
                }
                let argument_id = UnitExpressionId::new(self.source_unit, argument.value);
                let argument_span = nodes
                    .get(argument.value)
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, argument.span))?
                    .span();
                let mut contracts = self
                    .owned
                    .call_argument_contracts_for(call)
                    .filter(|contract| contract.argument() == argument_id);
                let Some(contract) = contracts.next() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, argument.span));
                };
                let mut loans = self
                    .owned
                    .loans()
                    .iter()
                    .filter(|loan| loan.call() == call && loan.argument() == argument_id);
                let Some(loan) = loans.next() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, argument.span));
                };
                if contracts.next().is_some()
                    || loans.next().is_some()
                    || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
                    || contract.category() != ExpressionCategory::Place
                    || contract.parameter_index() != mapping.parameter_index()
                    || contract.parameter_type() != root.narrowed
                    || contract.crosses_thread()
                    || contract.argument_span() != argument_span
                    || contract.loan_begin_span() != argument_span
                    || contract.call_span() != node.span()
                    || contract.parameter_span().is_none()
                    || loan.kind() != FrontendLoanKind::Shared
                    || loan.begin_span() != contract.loan_begin_span()
                    || loan.end_span() != node.span()
                    || loan.parameter_span() != contract.parameter_span()
                    || !matches!(loan.target(), UnitLoanTarget::Place(place)
                        if place.is_root() && place.root() == root.symbol)
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, argument.span));
                }
            }
            Ok(true)
        }

        fn heap_if_root_operand(
            &self,
            mut expression: ExpressionId,
            root: NamedNullableIfRoot,
            narrowed: bool,
            span: Span,
        ) -> Result<bool, LoweringError> {
            loop {
                let id = UnitExpressionId::new(self.source_unit, expression);
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if !matches!(node.payload(), Expression::Name | Expression::Group { .. }) {
                    return Ok(false);
                }
                let expected = if narrowed {
                    root.narrowed
                } else {
                    root.declared
                };
                if self.typed.expression_type(id) != Some(expected)
                    || self.typed.expression_category(id) != Some(ExpressionCategory::Place)
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                let fact = self.typed.non_null_use(id);
                if narrowed {
                    if fact.is_none_or(|fact| {
                        fact.expression() != id
                            || fact.symbol() != root.symbol
                            || fact.declared_type() != root.declared
                            || fact.narrowed_type() != root.narrowed
                    }) {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                } else if fact.is_some() {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                match node.payload() {
                    Expression::Group { expression: inner } => expression = *inner,
                    Expression::Name => {
                        if self.references.get(&span_key(node.span())) != Some(&root.symbol) {
                            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                        }
                        return Ok(true);
                    }
                    _ => return Ok(false),
                }
            }
        }
    }
}
