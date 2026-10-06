//! `while` / `loop` 的 backedge、jump 与 loop-carried binding lowering。

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
    pub(super) entry_views: BTreeMap<SymbolId, super::LoanId>,
    pub(super) header: BlockId,
    pub(super) carried: LinearBindings,
    pub(super) continues: Vec<LoopJump>,
    pub(super) breaks: Vec<BranchExit>,
    pub(super) for_loop: Option<ForLoopData>,
}

impl ExpressionLowerer<'_> {
    pub(super) fn lower_while(
        &mut self,
        statement: StatementId,
        condition: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
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
        self.finish_loop_continues(&context)?;
        let mut exits = context.breaks;
        exits.push(false_exit);
        self.finish_loop_exits(statement, exits, &baseline, span)
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
        self.finish_loop_exits(statement, context.breaks, &baseline, span)
    }

    fn finish_loop_exits(
        &mut self,
        statement: StatementId,
        mut exits: Vec<BranchExit>,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        // Different paths may already have consumed an owner; discharge each exit before joining.
        for exit in &mut exits {
            self.block = exit.block;
            self.bindings.clone_from(&exit.bindings);
            self.temporaries.clone_from(&exit.temporaries);
            self.pending_call_loans.clone_from(&exit.loans);
            self.capture_loans.clone_from(&exit.capture_loans);
            self.borrow_bindings.clone_from(&exit.borrow_bindings);
            self.non_null_bindings.clone_from(&exit.views);
            self.emit_drops(lang_frontend::ownership_checking::DropPoint::LoopExit(
                statement,
            ))?;
            *exit = self.loop_exit();
        }
        self.merge_exits(exits, baseline, span)
    }

    fn loop_exit(&self) -> BranchExit {
        self.branch_exit(LoweredValue::Unit)
    }

    pub(super) fn lower_break(&mut self, span: Span) -> Result<LoweredValue, LoweringError> {
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
                .map(|slot| loop_slot_entity(slot, &jump.exit, jump.span))
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
        })
    }

    pub(super) fn lower_for(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let plan = self
            .owned
            .iteration(statement)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let descriptor = plan.descriptor();
        if descriptor.statement() != statement || descriptor.source() != source {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }

        let container_type = self.resolve_type(descriptor.source_type(), span)?;
        let container_ssa_type = self
            .type_ids
            .get(&container_type)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element_type = self.resolve_type(descriptor.element_type(), span)?;
        let element_ssa_type = self
            .type_ids
            .get(&element_type)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let int_type = self.ssa_builtin(lang_frontend::type_checking::BuiltinType::Int, span)?;
        let bool_type =
            self.ssa_builtin(lang_frontend::type_checking::BuiltinType::Boolean, span)?;

        let mut new_source_loan = None;
        let source_loan = match plan.source() {
            lang_frontend::ownership_checking::LoanTarget::Place(target) if target.is_root() => {
                let symbol = target.root();
                if let Some(&loan) = self.borrow_bindings.get(&symbol) {
                    loan
                } else if let Some(&LoweredValue::Value(owner)) = self.bindings.get(&symbol) {
                    let (_, place_res) = self.append(
                        Operation::RootPlace { owner },
                        vec![EntityType::Place(container_ssa_type)],
                        span,
                    )?;
                    let EntityId::Place(place) = place_res[0] else {
                        return Err(error(LoweringErrorKind::InvalidModel, span));
                    };
                    let (_, loan_res) = self.append(
                        Operation::BorrowBegin {
                            place,
                            kind: LoanKind::Shared,
                        },
                        vec![EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: container_ssa_type,
                        }],
                        span,
                    )?;
                    let EntityId::Loan(loan) = loan_res[0] else {
                        return Err(error(LoweringErrorKind::InvalidModel, span));
                    };
                    new_source_loan = Some(loan);
                    loan
                } else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
            lang_frontend::ownership_checking::LoanTarget::Temporary(_) => {
                let result = self.lower(source)?;
                let LoweredValue::Value(temp_owner) = result else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let (_, place_res) = self.append(
                    Operation::RootPlace { owner: temp_owner },
                    vec![EntityType::Place(container_ssa_type)],
                    span,
                )?;
                let EntityId::Place(place) = place_res[0] else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                let (_, loan_res) = self.append(
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: container_ssa_type,
                    }],
                    span,
                )?;
                let EntityId::Loan(loan) = loan_res[0] else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                new_source_loan = Some(loan);
                loan
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };

        let snapshot = crate::ssa::provider::snapshot(
            self.function,
            self.block,
            source_loan,
            int_type,
            &Origin::Source(span),
        )
        .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        let baseline = self.bindings.clone();
        let mut carried = self.linear_binding_slots(&baseline, span)?;
        for (&symbol, &binding) in &baseline {
            match binding {
                LoweredValue::Unit => {}
                LoweredValue::Value(value) => {
                    if carried.slots.iter().any(|slot| slot.symbol == Some(symbol)) {
                        continue;
                    }
                    let source_ent = EntityId::Value(value);
                    let ty = self
                        .function
                        .entity(source_ent)
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                        .ty;
                    carried
                        .slots
                        .push(LinearBindingSlot::new(Some(symbol), source_ent, ty));
                }
                LoweredValue::Diverged => return Err(error(LoweringErrorKind::MissingFact, span)),
            }
        }

        let existing_source_slot = carried
            .slots
            .iter()
            .position(|s| s.source == EntityId::Loan(source_loan));

        let (source_slot, length_slot, cursor_slot) = if let Some(slot) = existing_source_slot {
            let length_slot = carried.slots.len();
            let cursor_slot = length_slot + 1;
            (slot, length_slot, cursor_slot)
        } else {
            let source_slot = carried.slots.len();
            let length_slot = source_slot + 1;
            let cursor_slot = length_slot + 1;
            (source_slot, length_slot, cursor_slot)
        };

        let mut header_param_types = carried.slots.iter().map(|s| s.ty).collect::<Vec<_>>();
        if existing_source_slot.is_none() {
            header_param_types.push(EntityType::Loan {
                kind: LoanKind::Shared,
                target: container_ssa_type,
            });
        }
        header_param_types.push(EntityType::Value(int_type));
        header_param_types.push(EntityType::Value(int_type));

        let header_block = self
            .function
            .add_block(header_param_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        let mut header_args = carried.slots.iter().map(|s| s.source).collect::<Vec<_>>();
        if existing_source_slot.is_none() {
            header_args.push(EntityId::Loan(source_loan));
        }
        header_args.push(EntityId::Value(snapshot.length()));
        header_args.push(EntityId::Value(snapshot.cursor()));

        let provider_header = crate::ssa::provider::enter_header(
            self.function,
            self.block,
            header_block,
            snapshot,
            header_args,
            source_slot,
            length_slot,
            cursor_slot,
            &Origin::Source(span),
        )
        .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        let header_params = &self
            .function
            .block(header_block)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
            .parameters
            .clone();
        let header_source = match header_params[source_slot] {
            EntityId::Loan(loan) => loan,
            _ => unreachable!(),
        };
        let header_cursor = match header_params[cursor_slot] {
            EntityId::Value(val) => val,
            _ => unreachable!(),
        };

        let mut body_param_types = carried.slots.iter().map(|s| s.ty).collect::<Vec<_>>();
        let mut body_args = carried
            .slots
            .iter()
            .enumerate()
            .map(|(i, _)| header_params[i])
            .collect::<Vec<_>>();

        let (body_source_slot, body_cursor_slot) = if existing_source_slot.is_some() {
            let body_cursor_slot = body_param_types.len();
            body_param_types.push(EntityType::Value(int_type));
            body_args.push(EntityId::Value(header_cursor));
            (source_slot, body_cursor_slot)
        } else {
            let body_source_slot = body_param_types.len();
            let body_cursor_slot = body_source_slot + 1;
            body_param_types.push(EntityType::Loan {
                kind: LoanKind::Shared,
                target: container_ssa_type,
            });
            body_param_types.push(EntityType::Value(int_type));
            body_args.push(EntityId::Loan(header_source));
            body_args.push(EntityId::Value(header_cursor));
            (body_source_slot, body_cursor_slot)
        };

        let body_block = self
            .function
            .add_block(body_param_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        let mut exit_param_types = carried.slots.iter().map(|s| s.ty).collect::<Vec<_>>();
        let mut exit_args = carried
            .slots
            .iter()
            .enumerate()
            .map(|(i, _)| header_params[i])
            .collect::<Vec<_>>();

        let exit_source_slot = if existing_source_slot.is_none() && new_source_loan.is_some() {
            let slot = exit_param_types.len();
            exit_param_types.push(EntityType::Loan {
                kind: LoanKind::Shared,
                target: container_ssa_type,
            });
            exit_args.push(EntityId::Loan(header_source));
            Some(slot)
        } else {
            None
        };

        let exit_block = self
            .function
            .add_block(exit_param_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        let guarded_element = crate::ssa::provider::guard_and_begin(
            self.function,
            &provider_header,
            bool_type,
            element_ssa_type,
            Edge {
                target: body_block,
                arguments: body_args,
            },
            Edge {
                target: exit_block,
                arguments: exit_args,
            },
            &Origin::Source(span),
        )
        .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        let body_params = &self
            .function
            .block(body_block)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
            .parameters
            .clone();
        let body_source = match body_params[body_source_slot] {
            EntityId::Loan(loan) => loan,
            _ => unreachable!(),
        };
        let body_cursor = match body_params[body_cursor_slot] {
            EntityId::Value(val) => val,
            _ => unreachable!(),
        };

        self.block = body_block;
        self.bindings = self.rebind_linear_bindings(&baseline, body_block, &carried, span)?;

        let element_loan = guarded_element.loan();
        match descriptor.binding() {
            lang_frontend::type_checking::SequentialIterationBinding::Discard => {}
            lang_frontend::type_checking::SequentialIterationBinding::Name(symbol) => {
                self.borrow_bindings.insert(*symbol, element_loan);
            }
            lang_frontend::type_checking::SequentialIterationBinding::Destructure(components) => {
                for (field_idx, component) in components.iter().enumerate() {
                    if let Some(symbol) = component.symbol() {
                        let comp_ty = self.resolve_type(component.ty(), span)?;
                        let comp_ssa_ty = self
                            .type_ids
                            .get(&comp_ty)
                            .copied()
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        let (_, res) = self.append(
                            Operation::SharedFieldLoan {
                                base: element_loan,
                                field: field_idx,
                            },
                            vec![EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: comp_ssa_ty,
                            }],
                            span,
                        )?;
                        let EntityId::Loan(comp_loan) = res[0] else {
                            return Err(error(LoweringErrorKind::InvalidModel, span));
                        };
                        self.borrow_bindings.insert(symbol, comp_loan);
                    }
                }
            }
        }

        let active_source_loan = if new_source_loan.is_some() && existing_source_slot.is_none() {
            Some(body_source)
        } else {
            None
        };

        self.loops.push(LoopContext {
            entry_views: self.non_null_bindings.clone(),
            header: header_block,
            carried: carried.clone(),
            continues: Vec::new(),
            breaks: Vec::new(),
            for_loop: Some(ForLoopData {
                statement,
                provider_header,
                guarded_element,
                int_type,
                plan: plan.clone(),
                new_source_loan,
                active_source_loan,
                body_source,
                body_cursor,
                source_slot,
                length_slot,
                cursor_slot,
                is_carried_source: existing_source_slot.is_some(),
            }),
        });

        let body_result = self.lower_statement(body)?;
        if !matches!(body_result, LoweredValue::Diverged) {
            let fallthrough_exit = plan
                .exits()
                .iter()
                .find(|e| {
                    matches!(
                        e.kind(),
                        lang_frontend::ownership_checking::IterationExitKind::Fallthrough
                    )
                })
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.emit_iteration_exit_plan(fallthrough_exit, span)?;

            let (body_cursor, step, int_type, body_source, header_length, is_carried_source) = {
                let for_data = self
                    .loops
                    .last()
                    .and_then(|c| c.for_loop.as_ref())
                    .expect("for_loop data must be present");
                (
                    for_data.body_cursor,
                    for_data.guarded_element.step(),
                    for_data.int_type,
                    for_data.body_source,
                    for_data.provider_header.length,
                    for_data.is_carried_source,
                )
            };

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
            let mut backedge_args = carried
                .slots
                .iter()
                .map(|slot| loop_slot_entity(slot, &current_exit, span))
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
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::Branch(backedge),
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        }

        // Exhaustion path in exit_block
        self.block = exit_block;
        self.bindings = self.rebind_linear_bindings(&baseline, exit_block, &carried, span)?;

        let exit_source_loan = if let Some(slot) = exit_source_slot {
            let exit_params = &self
                .function
                .block(exit_block)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                .parameters;
            match exit_params[slot] {
                EntityId::Loan(loan) => Some(loan),
                _ => unreachable!(),
            }
        } else {
            None
        };

        if let Some(context) = self.loops.last_mut()
            && let Some(ref mut for_data) = context.for_loop
        {
            for_data.active_source_loan = exit_source_loan;
        }

        let exhaustion_exit = plan
            .exits()
            .iter()
            .find(|e| {
                matches!(
                    e.kind(),
                    lang_frontend::ownership_checking::IterationExitKind::Exhaustion
                )
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.emit_iteration_exit_plan(exhaustion_exit, span)?;

        let context = self.loops.pop().expect("for loop context must be balanced");
        let mut exits = context.breaks;
        let false_exit = self.loop_exit();
        exits.push(false_exit);

        self.merge_exits(exits, &baseline, span)
    }

    pub(super) fn lower_for_break(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
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
            .map(|slot| loop_slot_entity(slot, &current_exit, span))
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
                let owner = match fact.target() {
                    DropTarget::Named(symbol) => {
                        if let Some(loan) = self.non_null_bindings.remove(&symbol) {
                            self.append(
                                Operation::BorrowEnd { loan },
                                Vec::new(),
                                fact.value_origin(),
                            )?;
                        }
                        match self.bindings.remove(&symbol) {
                            Some(LoweredValue::Value(value)) => value,
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
                self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
                self.release_owner_capture_loans(owner, fact.value_origin())?;
            }
            Action::EndBinding { symbol, statement } => {
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
    slot: &LinearBindingSlot,
    exit: &BranchExit,
    span: Span,
) -> Result<EntityId, LoweringError> {
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
    } else if let Some(symbol) = slot.views.first() {
        exit.views.get(symbol).copied().map(EntityId::Loan)
    } else if let Some(symbol) = slot.borrow_symbols.first() {
        exit.borrow_bindings
            .get(symbol)
            .copied()
            .map(EntityId::Loan)
    } else if let Some(stmt) = slot.for_sources.first() {
        exit.for_sources.get(stmt).copied().map(EntityId::Loan)
    } else if let Some(stmt) = slot.for_elements.first() {
        exit.for_elements.get(stmt).copied().map(EntityId::Loan)
    } else {
        None
    };
    entity.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
}
