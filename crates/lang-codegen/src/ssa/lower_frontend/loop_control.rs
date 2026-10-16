//! `while` / `loop` 的 backedge、jump 与 loop-carried binding lowering。

mod iteration;
mod nullable_while;
mod numeric_iteration;

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    source::Span,
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind,
    control::{BranchExit, LinearBindingSlot, LinearBindings},
    error,
};
use crate::ssa::model::{
    BlockId, Edge, EntityId, EntityType, LoanId, LoanKind, Operation, Origin, SsaTypeId,
    TerminatorKind, ValueId,
};

pub(super) struct LoopJump {
    exit: BranchExit,
    span: Span,
}

pub(super) struct ForLoopData {
    pub(super) statement: StatementId,
    pub(super) provider_header: crate::ssa::provider::ProviderHeader,
    pub(super) guarded_element: crate::ssa::provider::GuardedElement,
    pub(super) int_type: SsaTypeId,
    pub(super) plan: lang_frontend::ownership_checking::IterationOwnershipPlan,
    pub(super) new_source_loan: Option<LoanId>,
    pub(super) active_source_loan: Option<LoanId>,
    pub(super) body_source: LoanId,
    pub(super) body_cursor: ValueId,
    pub(super) source_slot: usize,
    pub(super) length_slot: usize,
    pub(super) cursor_slot: usize,
    pub(super) is_carried_source: bool,
}

impl ForLoopData {
    pub(super) fn rebind_source(&mut self, loan: LoanId) {
        self.body_source = loan;
        // A sibling may have ended its own source. Restore this path from ownership,
        // not the previous path's active state; borrowed sources remain caller-owned.
        self.active_source_loan =
            (self.new_source_loan.is_some() && !self.is_carried_source).then_some(loan);
    }
}

pub(super) struct LoopContext {
    pub(super) entry_views: BTreeMap<SymbolId, super::NonNullProof>,
    pub(super) header: BlockId,
    pub(super) carried: LinearBindings,
    pub(super) continues: Vec<LoopJump>,
    pub(super) breaks: Vec<BranchExit>,
    pub(super) for_loop: Option<ForLoopData>,
    pub(super) numeric_loop: Option<numeric_iteration::NumericLoopData>,
}

impl LoopContext {
    pub(super) fn iteration_plan(
        &self,
    ) -> Option<&lang_frontend::ownership_checking::IterationOwnershipPlan> {
        self.for_loop
            .as_ref()
            .map(|data| &data.plan)
            .or_else(|| self.numeric_loop.as_ref().map(|data| &data.plan))
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn lower_while(
        &mut self,
        statement: StatementId,
        condition: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let inherited = if let Some((descriptor, subject)) =
            self.direct_nullable_while_condition(condition, span)?
        {
            if self.non_null_bindings.is_empty() {
                return self
                    .lower_direct_nullable_while(statement, condition, body, descriptor, span);
            }
            let inherited = self.prepare_nested_nullable_while(descriptor, subject, span)?;
            if inherited.is_none() {
                return self
                    .lower_direct_nullable_while(statement, condition, body, descriptor, span);
            }
            inherited
        } else {
            None
        };
        let baseline = self.bindings.clone();
        let context = self.create_loop_header(&baseline, span)?;
        let condition = self.require_value(condition)?;
        let condition_bindings = self.bindings.clone();
        let carried = self.linear_binding_slots(&condition_bindings, span)?;
        let parameter_types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
        let body_block = self
            .function
            .add_block(parameter_types.clone(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let false_block = self
            .function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition,
                    when_true: loop_edge(body_block, &carried),
                    when_false: loop_edge(false_block, &carried),
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        // Save the false edge before body lowering changes any owner or proof state.
        self.block = false_block;
        self.bindings =
            self.rebind_linear_bindings(&condition_bindings, false_block, &carried, span)?;
        let false_exit = self.loop_exit();
        self.block = body_block;
        self.bindings =
            self.rebind_linear_bindings(&condition_bindings, body_block, &carried, span)?;
        self.loops.push(context);
        let body_result = self.lower_statement(body)?;
        if !matches!(body_result, LoweredValue::Diverged) {
            self.record_continue(span)?;
        }
        let context = self.loops.pop().expect("while context must be balanced");
        if let Some((root, proof)) = inherited {
            for jump in &context.continues {
                self.validate_readonly_while_exit(root, proof, &jump.exit, jump.span)?;
            }
        }
        self.finish_loop_continues(&context)?;
        let mut exits = context.breaks;
        exits.push(false_exit);
        let result = self.finish_loop_exits(statement, exits, &baseline, inherited, span)?;
        if let Some((root, proof)) = inherited {
            self.validate_readonly_while_exit(root, proof, &self.loop_exit(), span)?;
        }
        Ok(result)
    }

    pub(super) fn lower_loop(
        &mut self,
        statement: StatementId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let baseline = self.bindings.clone();
        let context = self.create_loop_header(&baseline, span)?;
        self.loops.push(context);
        let body_result = self.lower_statement(body)?;
        if !matches!(body_result, LoweredValue::Diverged) {
            self.record_continue(span)?;
        }
        let context = self.loops.pop().expect("loop context must be balanced");
        self.finish_loop_continues(&context)?;
        self.finish_loop_exits(statement, context.breaks, &baseline, None, span)
    }

    fn finish_loop_exits(
        &mut self,
        statement: StatementId,
        mut exits: Vec<BranchExit>,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        inherited: Option<(SymbolId, super::NonNullProof)>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        // Different paths may already have consumed an owner; discharge each exit before joining.
        for exit in &mut exits {
            if let Some((root, proof)) = inherited {
                self.validate_readonly_while_exit(root, proof, exit, span)?;
            }
            self.block = exit.block;
            self.bindings.clone_from(&exit.bindings);
            self.temporaries.clone_from(&exit.temporaries);
            self.pending_call_loans.clone_from(&exit.loans);
            self.capture_loans.clone_from(&exit.capture_loans);
            self.borrow_bindings.clone_from(&exit.borrow_bindings);
            self.result_source_loans
                .clone_from(&exit.result_source_loans);
            self.non_null_bindings.clone_from(&exit.views);
            self.emit_drops(lang_frontend::ownership_checking::DropPoint::LoopExit(
                statement,
            ))?;
            *exit = self.loop_exit();
            if let Some((root, proof)) = inherited {
                self.validate_readonly_while_exit(root, proof, exit, span)?;
            }
        }
        self.merge_exits(exits, baseline, span)
    }

    fn loop_exit(&self) -> BranchExit {
        self.branch_exit(LoweredValue::Unit)
    }

    pub(super) fn lower_break(&mut self, span: Span) -> Result<LoweredValue, LoweringError> {
        self.retire_while_proofs(false, span)?;
        let exit = self.loop_exit();
        let context = self
            .loops
            .last_mut()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        context.breaks.push(exit);
        Ok(LoweredValue::Diverged)
    }

    pub(super) fn lower_continue(&mut self, span: Span) -> Result<LoweredValue, LoweringError> {
        self.record_continue(span)?;
        Ok(LoweredValue::Diverged)
    }

    fn record_continue(&mut self, span: Span) -> Result<(), LoweringError> {
        self.retire_while_proofs(false, span)?;
        let jump = LoopJump {
            exit: self.loop_exit(),
            span,
        };
        let context = self
            .loops
            .last_mut()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        context.continues.push(jump);
        Ok(())
    }

    fn finish_loop_continues(&mut self, context: &LoopContext) -> Result<(), LoweringError> {
        for jump in &context.continues {
            let arguments = context
                .carried
                .slots
                .iter()
                .map(|slot| loop_slot_entity(self, slot, &jump.exit, jump.span))
                .collect::<Result<Vec<_>, _>>()?;
            self.function
                .set_terminator(
                    jump.exit.block,
                    TerminatorKind::Branch(Edge {
                        target: context.header,
                        arguments,
                    }),
                    Origin::Source(jump.span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, jump.span))?;
        }
        Ok(())
    }

    fn create_loop_header(
        &mut self,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LoopContext, LoweringError> {
        let mut carried = self.linear_binding_slots(baseline, span)?;
        // Scalar bindings may change on each iteration, so loops carry them too.
        for (&symbol, &binding) in baseline {
            match binding {
                LoweredValue::Unit => {}
                LoweredValue::Value(value) => {
                    if carried.slots.iter().any(|slot| slot.symbol == Some(symbol)) {
                        continue;
                    }
                    let source = EntityId::Value(value);
                    let ty = self
                        .function
                        .entity(source)
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                        .ty;
                    carried
                        .slots
                        .push(LinearBindingSlot::new(Some(symbol), source, ty));
                }
                LoweredValue::Diverged => {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
        }
        let header = self
            .function
            .add_block(
                carried.slots.iter().map(|slot| slot.ty).collect(),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(loop_edge(header, &carried)),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.bindings = self.rebind_linear_bindings(baseline, header, &carried, span)?;
        self.block = header;
        Ok(LoopContext {
            entry_views: self.non_null_bindings.clone(),
            header,
            carried,
            continues: Vec::new(),
            breaks: Vec::new(),
            for_loop: None,
            numeric_loop: None,
        })
    }

    pub(super) fn lower_for_break(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if self
            .loops
            .last()
            .is_some_and(|context| context.numeric_loop.is_some())
        {
            return self.lower_numeric_break(expression, span);
        }
        let (plan, statement) = {
            let context = self
                .loops
                .last()
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let for_data = context
                .for_loop
                .as_ref()
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            (for_data.plan.clone(), for_data.statement)
        };
        let break_exit = plan
            .exits()
            .iter()
            .find(|e| {
                matches!(
                    e.kind(),
                    lang_frontend::ownership_checking::IterationExitKind::Break(expr) if expr == expression
                )
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.emit_iteration_exit_plan(break_exit, span)?;
        self.finish_map_loop_break(statement, expression, span)?;
        let mut exit = self.loop_exit();
        exit.for_sources.remove(&statement.index());
        exit.for_elements.remove(&statement.index());
        let context = self.loops.last_mut().expect("loop context present");
        context.breaks.push(exit);
        Ok(LoweredValue::Diverged)
    }

    pub(super) fn lower_for_continue(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if self
            .loops
            .last()
            .is_some_and(|context| context.numeric_loop.is_some())
        {
            return self.lower_numeric_continue(expression, span);
        }
        let (
            plan,
            header_block,
            int_type,
            body_cursor,
            step,
            body_source,
            is_carried_source,
            header_length,
        ) = {
            let context = self
                .loops
                .last()
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let for_data = context
                .for_loop
                .as_ref()
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            (
                for_data.plan.clone(),
                context.header,
                for_data.int_type,
                for_data.body_cursor,
                for_data.guarded_element.step(),
                for_data.body_source,
                for_data.is_carried_source,
                for_data.provider_header.length,
            )
        };
        let continue_exit = plan
            .exits()
            .iter()
            .find(|e| {
                matches!(
                    e.kind(),
                    lang_frontend::ownership_checking::IterationExitKind::Continue(expr) if expr == expression
                )
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.emit_iteration_exit_plan(continue_exit, span)?;

        let (_, next_res) = self.append(
            Operation::Binary {
                operator: crate::ssa::model::BinaryOperator::Add,
                left: body_cursor,
                right: step,
            },
            vec![EntityType::Value(int_type)],
            span,
        )?;
        let EntityId::Value(next_cursor) = next_res[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };

        let current_exit = self.loop_exit();
        let context = self.loops.last().expect("loop context present");
        let mut backedge_args = context
            .carried
            .slots
            .iter()
            .map(|slot| loop_slot_entity(self, slot, &current_exit, span))
            .collect::<Result<Vec<_>, _>>()?;
        if !is_carried_source {
            backedge_args.push(EntityId::Loan(body_source));
        }
        backedge_args.push(EntityId::Value(header_length));
        backedge_args.push(EntityId::Value(next_cursor));

        let backedge = Edge {
            target: header_block,
            arguments: backedge_args,
        };
        self.record_map_loop_backedge(
            plan.descriptor().statement(),
            lang_frontend::ownership_checking::IterationPhiIncomingKind::Continue(expression),
            &backedge,
            span,
        )?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(backedge),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        Ok(LoweredValue::Diverged)
    }

    pub(super) fn emit_iteration_exit_plan(
        &mut self,
        exit: &lang_frontend::ownership_checking::IterationExitPlan,
        span: Span,
    ) -> Result<(), LoweringError> {
        for action in exit.actions() {
            self.emit_iteration_cleanup_action(action, span)?;
        }
        Ok(())
    }

    pub(super) fn emit_iteration_cleanup_action(
        &mut self,
        action: &lang_frontend::ownership_checking::IterationCleanupAction,
        span: Span,
    ) -> Result<(), LoweringError> {
        use lang_frontend::ownership_checking::{DropTarget, IterationCleanupAction as Action};
        match action {
            Action::Drop(fact) => {
                if fact.condition().is_some()
                    && !super::map_loop_owner::supported_drop(
                        self.parsed,
                        self.names,
                        self.typed,
                        self.owned,
                        *fact,
                    )
                {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
                let owner = match fact.target() {
                    DropTarget::Named(symbol) => {
                        self.end_non_null_proof(symbol, fact.value_origin())?;
                        match self.bindings.remove(&symbol) {
                            Some(LoweredValue::Value(value)) => value,
                            _ if fact.condition().is_some()
                                && !super::constant_presence::constant_drop(
                                    self.parsed,
                                    self.names,
                                    self.typed,
                                    self.owned,
                                    *fact,
                                ) =>
                            {
                                return Err(error(
                                    LoweringErrorKind::MissingFact,
                                    fact.value_origin(),
                                ));
                            }
                            _ => return Ok(()),
                        }
                    }
                    DropTarget::Temporary(expr) => match self.temporaries.remove(&expr.index()) {
                        Some(value) => {
                            self.temporaries.retain(|_, candidate| *candidate != value);
                            value
                        }
                        None => return Ok(()),
                    },
                    _ => return Ok(()),
                };
                self.record_map_loop_drop(*fact, owner)?;
                self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
                self.release_owner_capture_loans(owner, fact.value_origin())?;
            }
            Action::EndBinding { symbol, statement } => {
                if let Some(plan) = self.owned.iteration(*statement)
                    && let lang_frontend::type_checking::IterationProvider::NumericRange(endpoint) =
                        plan.descriptor().provider()
                {
                    if plan.source_kind()
                        != &lang_frontend::ownership_checking::IterationSource::OwnedSnapshot(
                            plan.descriptor().source(),
                        )
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    let endpoint_type = self.ssa_builtin(endpoint, span)?;
                    let Some(LoweredValue::Value(value)) = self.bindings.remove(symbol) else {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    };
                    if self
                        .function
                        .entity(EntityId::Value(value))
                        .map(|entity| entity.ty)
                        != Some(EntityType::Value(endpoint_type))
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    return Ok(());
                }
                if let Some(loan) = self.borrow_bindings.remove(symbol) {
                    let is_element_loan = self.loops.iter().rev().any(|c| {
                        c.for_loop.as_ref().is_some_and(|f| {
                            f.statement == *statement && f.guarded_element.loan() == loan
                        })
                    });
                    if !is_element_loan {
                        self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                    }
                }
            }
            Action::EndElement(stmt) => {
                if let Some(context) = self
                    .loops
                    .iter()
                    .rev()
                    .find(|c| c.for_loop.as_ref().is_some_and(|f| f.statement == *stmt))
                    && let Some(ref for_data) = context.for_loop
                {
                    let loan = for_data.guarded_element.loan();
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                }
            }
            Action::FinishProvider(_) => {}
            Action::EndSource(stmt) => {
                if let Some(context) = self
                    .loops
                    .iter_mut()
                    .rev()
                    .find(|c| c.for_loop.as_ref().is_some_and(|f| f.statement == *stmt))
                    && let Some(ref mut for_data) = context.for_loop
                    && let Some(loan) = for_data.active_source_loan.take()
                {
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                }
                if let Some(source) = self
                    .owned
                    .iteration(*stmt)
                    .map(|plan| plan.descriptor().source())
                    && self
                        .owned
                        .borrow_results()
                        .range_uses()
                        .iter()
                        .any(|fact| fact.expression() == source)
                {
                    self.finish_short_range(source, span)?;
                }
            }
            Action::EndCallLoan(fact) => {
                let key = (fact.call().index(), fact.argument().index());
                if let Some(Some(loan)) = self.pending_call_loans.remove(&key) {
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn loop_edge(target: BlockId, carried: &LinearBindings) -> Edge {
    Edge {
        target,
        arguments: carried.slots.iter().map(|slot| slot.source).collect(),
    }
}

/// Resolve header slots from the exit snapshot, after nested CFG has rebound IDs.
fn loop_slot_entity(
    lowerer: &ExpressionLowerer<'_>,
    slot: &LinearBindingSlot,
    exit: &BranchExit,
    span: Span,
) -> Result<EntityId, LoweringError> {
    for &(symbol, entry_proof) in &slot.views {
        let proof = exit
            .views
            .get(&symbol)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        lowerer.validate_non_null_proof(symbol, proof, &exit.bindings, span)?;
        if entry_proof.inner != proof.inner
            || entry_proof.origin != proof.origin
            || slot.source != EntityId::Loan(entry_proof.loan)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
    }
    let entity = if let Some(symbol) = slot.symbol {
        match exit.bindings.get(&symbol) {
            Some(LoweredValue::Value(value)) => Some(EntityId::Value(*value)),
            _ => None,
        }
    } else if let Some(key) = slot.temporaries.first() {
        exit.temporaries.get(key).copied().map(EntityId::Value)
    } else if let Some(key) = slot.loans.first() {
        exit.loans.get(key).copied().flatten().map(EntityId::Loan)
    } else if let Some(key) = slot.captures.first() {
        exit.capture_loans.get(key).copied().map(EntityId::Loan)
    } else if let Some((symbol, _)) = slot.views.first() {
        exit.views
            .get(symbol)
            .map(|proof| EntityId::Loan(proof.loan))
    } else if let Some(symbol) = slot.borrow_symbols.first() {
        exit.borrow_bindings
            .get(symbol)
            .copied()
            .map(EntityId::Loan)
    } else if let Some((symbol, index)) = slot.result_sources.first() {
        exit.result_source_loans
            .get(symbol)
            .and_then(|loans| loans.get(*index))
            .copied()
            .map(EntityId::Loan)
    } else if let Some(stmt) = slot.for_sources.first() {
        exit.for_sources.get(stmt).copied().map(EntityId::Loan)
    } else if let Some(stmt) = slot.for_elements.first() {
        exit.for_elements.get(stmt).copied().map(EntityId::Loan)
    } else {
        None
    };
    let entity = entity.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
    if slot.views.iter().any(|(symbol, _)| {
        exit.views
            .get(symbol)
            .is_none_or(|proof| entity != EntityId::Loan(proof.loan))
    }) {
        return Err(error(LoweringErrorKind::MissingFact, span));
    }
    Ok(entity)
}
