//! Provider state occupies stable pending slots so every existing unit CFG rebinds it.
use super::*;
use crate::ssa::{
    model::{EntityId, EntityType, LoanId, LoanKind, Operation, SsaTypeId, ValueId},
    provider,
};
use lang_frontend::{
    ownership_checking::{
        UnitIterationCleanupAction as Action, UnitIterationSourceAccess, UnitLoanTarget,
    },
    type_checking::{BuiltinType, UnitSequentialIterationBinding},
};

pub(super) struct IterationContext {
    pub(super) statement: UnitStatementId,
    pub(super) retained_start: usize,
    pub(super) outer_temporaries: BTreeSet<UnitExpressionId>,
    pub(super) start: usize,
    pub(super) element: usize,
    pub(super) step: ValueId,
    pub(super) int_type: SsaTypeId,
}

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn lower_for(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.reject_live_if_proof(span)?;
        let statement_id = UnitStatementId::new(self.source_unit, statement);
        let Some(plan) = self.owned.iteration(statement_id).cloned() else {
            // Source control transfer happens before AcquireProvider; no exit plan exists yet.
            return if self.lower(source)? == LoweredValue::Diverged {
                Ok(LoweredValue::Diverged)
            } else {
                Err(lowering_error(LoweringErrorKind::MissingFact, span))
            };
        };
        let descriptor = plan.descriptor();
        if descriptor.source() != UnitExpressionId::new(self.source_unit, source)
            || self.typed.sequential_iteration(statement_id) != Some(descriptor)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if plan.source_access() == UnitIterationSourceAccess::Exclusive {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if matches!(
            descriptor.provider(),
            lang_frontend::type_checking::IterationProvider::NumericRange(_)
        ) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let container = self.iteration_type(descriptor.source_type(), span)?;
        let element_type = self.iteration_type(descriptor.element_type(), span)?;
        let int_type = self.ssa_builtin(BuiltinType::Int, span)?;
        let bool_type = self.ssa_builtin(BuiltinType::Boolean, span)?;
        let outer_temporaries = self.temporaries.keys().copied().collect();
        let retained_start = self.pending_operands.len();
        if descriptor.provider() == lang_frontend::type_checking::IterationProvider::RangeView {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let source_loan = {
            match plan
                .source()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            {
                UnitLoanTarget::Place(place) => {
                    let symbol = place.root();
                    let symbol_data = self.names.names().source_units()
                        [symbol.source_unit().index()]
                    .resolution()
                    .symbols()
                    .get(symbol.symbol().index())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    if !place.is_root()
                        || symbol_data.kind() == lang_frontend::name_resolution::SymbolKind::Field
                    {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                    match plan.source_access() {
                        UnitIterationSourceAccess::Shared => {
                            let loan = *self.borrow_bindings.get(&symbol).ok_or_else(|| {
                                lowering_error(LoweringErrorKind::MissingFact, span)
                            })?;
                            self.iteration_loan(
                                Operation::SharedReborrow { source: loan },
                                container,
                                span,
                            )?
                        }
                        UnitIterationSourceAccess::Owned => {
                            let Some(LoweredValue::Value(owner)) =
                                self.bindings.get(&symbol).copied()
                            else {
                                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                            };
                            self.iteration_owner_loan(owner, container, span)?
                        }
                        _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
                    }
                }
                UnitLoanTarget::Temporary(_) => {
                    let owner = match self.lower(source)? {
                        LoweredValue::Value(owner) => owner,
                        LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                        LoweredValue::Unit => {
                            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                        }
                    };
                    self.iteration_owner_loan(owner, container, span)?
                }
                UnitLoanTarget::This(_) => {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
        };
        let preheader = self.block;
        let origin = Origin::Source(span);
        let snapshot = provider::snapshot(
            self.function,
            preheader,
            source_loan,
            int_type,
            &origin,
            descriptor.provider(),
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let start = self.pending_operands.len();
        self.pending_operands.extend([
            EntityId::Loan(source_loan),
            EntityId::Value(snapshot.length()),
            EntityId::Value(snapshot.cursor()),
        ]);
        let baseline = self.bindings.clone();
        let mut context = self.create_loop_context(&baseline, span)?;
        let entry = carried_control_edge(context.header, &context.carried, &context.loans);
        let slot = |entity| {
            entry
                .arguments
                .iter()
                .position(|candidate| *candidate == entity)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        };
        let source_slot = slot(EntityId::Loan(source_loan))?;
        let length_slot = slot(EntityId::Value(snapshot.length()))?;
        let cursor_slot = slot(EntityId::Value(snapshot.cursor()))?;
        let header = provider::enter_header(
            self.function,
            preheader,
            context.header,
            snapshot,
            entry.arguments,
            source_slot,
            length_slot,
            cursor_slot,
            &origin,
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = context.header;
        self.bindings = self.rebind_loop(&baseline, self.block, &context, span)?;
        let header_state = self.loop_state();
        let body_block = self.add_carried_control_block(&context.carried, &context.loans, span)?;
        let false_block = self.add_carried_control_block(&context.carried, &context.loans, span)?;
        let body_edge = self.carried_edge_from(
            body_block,
            &context.carried,
            &context.loans,
            &header_state,
            span,
        )?;
        let false_edge = self.carried_edge_from(
            false_block,
            &context.carried,
            &context.loans,
            &header_state,
            span,
        )?;
        let element = provider::guard_and_begin(
            self.function,
            &header,
            bool_type,
            element_type,
            body_edge,
            false_edge,
            &origin,
            descriptor.provider(),
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = body_block;
        self.bindings = self.rebind_loop(&baseline, body_block, &context, span)?;
        let element_slot = self.pending_operands.len();
        self.pending_operands.push(EntityId::Loan(element.loan()));
        context.iteration = Some(IterationContext {
            statement: statement_id,
            retained_start,
            outer_temporaries,
            start,
            element: element_slot,
            step: header.step,
            int_type,
        });
        match descriptor.binding() {
            UnitSequentialIterationBinding::Discard => {}
            UnitSequentialIterationBinding::Name(symbol) => {
                self.borrow_bindings.insert(*symbol, element.loan());
            }
            UnitSequentialIterationBinding::Destructure(parts) => {
                for part in parts {
                    let Some(symbol) = part.symbol() else {
                        continue;
                    };
                    let ty = self.iteration_type(part.ty(), span)?;
                    let concrete = super::super::resolve_concrete_type(
                        self.typed,
                        descriptor.element_type(),
                        self.substitutions,
                        self.static_self,
                        span,
                    )?;
                    let field = *self
                        .field_indices
                        .get(&(concrete, part.field()))
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    let loan = self.iteration_loan(
                        Operation::SharedFieldLoan {
                            base: element.loan(),
                            field,
                        },
                        ty,
                        span,
                    )?;
                    self.borrow_bindings.insert(symbol, loan);
                }
            }
        }
        self.loops.push(context);
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.finish_continues(&context, context.header)?;
        self.block = false_block;
        self.current_receiver = context.entry_receiver;
        self.consumed_receiver = context.entry_consumed_receiver;
        self.bindings = self.rebind_loop(&baseline, false_block, &context, span)?;
        self.loops.push(context);
        self.emit_drops(UnitDropPoint::LoopExit(statement_id))?;
        self.pending_operands.truncate(retained_start);
        let false_exit = self.loop_state();
        let context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let mut exits = context
            .breaks
            .into_iter()
            .map(branch_exit)
            .collect::<Vec<_>>();
        exits.push(false_exit);
        self.merge_loop_exits(exits, span)?;
        Ok(LoweredValue::Unit)
    }

    pub(super) fn iteration_type(
        &self,
        ty: lang_frontend::type_checking::UnitTypeId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let concrete = super::super::resolve_concrete_type(
            self.typed,
            ty,
            self.substitutions,
            self.static_self,
            span,
        )?;
        self.type_ids
            .get(&concrete)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    pub(super) fn iteration_owner_loan(
        &mut self,
        owner: ValueId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        let (_, result) = self
            .function
            .append_instruction(
                self.block,
                Operation::RootPlace { owner },
                vec![EntityType::Place(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(place) = result[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.iteration_loan(
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            ty,
            span,
        )
    }

    pub(super) fn iteration_loan(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        let (_, result) = self
            .function
            .append_instruction(
                self.block,
                operation,
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                }],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        match result[0] {
            EntityId::Loan(loan) => Ok(loan),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    pub(super) fn advance_iteration(&mut self, span: Span) -> Result<(), LoweringError> {
        let Some(context) = self
            .loops
            .last()
            .and_then(|context| context.iteration.as_ref())
        else {
            return Ok(());
        };
        let start = context.start;
        let step = context.step;
        let int_type = context.int_type;
        let cursor_index = start + 2;
        let Some(EntityId::Value(cursor)) = self.pending_operands.get(cursor_index).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(EntityId::Loan(_)) = self.pending_operands.get(start).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self.loops.last().is_none() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let next = provider::advance_in_block(
            self.function,
            self.block,
            cursor,
            step,
            int_type,
            &Origin::Source(span),
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.pending_operands[cursor_index] = EntityId::Value(next);
        Ok(())
    }

    pub(in super::super) fn emit_iteration_cleanup(
        &mut self,
        _point: UnitDropPoint,
        actions: &[Action],
    ) -> Result<(), LoweringError> {
        for action in actions {
            match action {
                Action::Drop(fact) => {
                    self.emit_drop_fact(*fact)?;
                }
                Action::DropConditionalReceiver(fact) => {
                    if let Some(owner) = self.conditional_receiver_drop_owner(*fact)? {
                        self.consumed_receiver = self.current_receiver.map(Into::into);
                        self.current_receiver = None;
                        self.function
                            .append_instruction(
                                self.block,
                                Operation::Drop { owner },
                                Vec::new(),
                                Origin::Source(fact.value_origin()),
                            )
                            .map_err(|_| {
                                lowering_error(LoweringErrorKind::InvalidModel, fact.value_origin())
                            })?;
                    }
                }
                Action::EndBinding { statement, symbol } => {
                    let span = self.statement_span(statement.statement())?;
                    let element = self.iteration_context(*statement, span)?.element;
                    let element = self.iteration_pending_loan(element, span)?;
                    let loan = self
                        .borrow_bindings
                        .remove(symbol)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    if loan != element {
                        self.iteration_end_loan(loan, span)?;
                    }
                }
                Action::EndElement(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    let element = self.iteration_context(*statement, span)?.element;
                    let loan = self.iteration_pending_loan(element, span)?;
                    self.iteration_end_loan(loan, span)?;
                }
                Action::FinishProvider(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    let _ = self.iteration_context(*statement, span)?;
                }
                Action::EndSource(statement) => {
                    let span = self.statement_span(statement.statement())?;
                    let start = self.iteration_context(*statement, span)?.start;
                    let loan = self.iteration_pending_loan(start, span)?;
                    self.iteration_end_loan(loan, span)?;
                }
                Action::EndCallLoan(fact) => {
                    let slots = self
                        .pending_call_frames
                        .iter()
                        .rev()
                        .filter(|frame| frame.call == fact.call() && !frame.receiver)
                        .find_map(|frame| {
                            frame
                                .loan_arguments
                                .iter()
                                .find(|(argument, _)| *argument == fact.argument())
                                .map(|(_, slots)| slots.clone())
                        })
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, fact.begin_span())
                        })?;
                    for slot in slots.into_iter().rev() {
                        let loan = self.iteration_pending_loan(slot, fact.begin_span())?;
                        self.iteration_end_loan(loan, fact.end_span())?;
                    }
                }
                Action::EndReceiverLoan(fact) => {
                    let slots = self
                        .pending_call_frames
                        .iter()
                        .rev()
                        .filter(|frame| frame.call == fact.call() && frame.receiver)
                        .map(|frame| frame.created_loans.clone())
                        .next()
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, fact.begin_span())
                        })?;
                    for slot in slots.into_iter().rev() {
                        let loan = self.iteration_pending_loan(slot, fact.begin_span())?;
                        self.iteration_end_loan(loan, fact.end_span())?;
                    }
                }
                Action::EndCaptureLoan { closure, .. } => {
                    // Unit captured Borrow closures have no runtime representation yet.
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        self.parsed
                            .ast()
                            .expressions()
                            .get(closure.expression())
                            .map_err(|_| LoweringError {
                                kind: LoweringErrorKind::MissingFact,
                                span: None,
                            })?
                            .span(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn iteration_context(
        &self,
        statement: UnitStatementId,
        span: Span,
    ) -> Result<&IterationContext, LoweringError> {
        self.loops
            .iter()
            .rev()
            .filter_map(|context| context.iteration.as_ref())
            .find(|context| context.statement == statement)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    fn iteration_pending_loan(&self, index: usize, span: Span) -> Result<LoanId, LoweringError> {
        match self.pending_operands.get(index) {
            Some(EntityId::Loan(loan)) => Ok(*loan),
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }

    pub(super) fn iteration_end_loan(
        &mut self,
        loan: LoanId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.function
            .append_instruction(
                self.block,
                Operation::BorrowEnd { loan },
                Vec::new(),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(())
    }
}

#[cfg(test)]
mod provider_entry_projection_tests {
    //! Synthetic entry-state guards, not end-to-end verifier rejection tests.
    use crate::ssa::model::{
        BlockId, Definition, Edge, EntityId, EntityType, Function, LoanId, LoanKind, Operation,
        Origin, PlaceId, Program, SequentialContainerKind, SsaTypeId, SsaTypeKind, TerminatorKind,
        ValueId,
    };
    use crate::ssa::verify_ownership::aliases::provider_lifetime::{
        EntryProjection, Reference, State,
    };
    use lang_frontend::source::SourceMap;

    fn fixture() -> (Function, EntityType, SsaTypeId, SsaTypeId, Origin) {
        let mut sources = SourceMap::default();
        let source = sources
            .add_source("provider-projection.ko", "fixture")
            .unwrap();
        let origin = Origin::Source(sources.span(source, 0, 7).unwrap());
        let mut program = Program::default();
        let module_id = program.add_module("provider-projection");
        let module = program.module_mut(module_id).unwrap();
        let int = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        });
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let array = module
            .add_sequential_container_type(SequentialContainerKind::Array, int)
            .unwrap();
        let loan_type = EntityType::Loan {
            kind: LoanKind::Shared,
            target: array,
        };
        module
            .add_function("project", vec![], origin.clone())
            .unwrap();
        (module.functions.remove(0), loan_type, int, boolean, origin)
    }

    fn loan(entity: EntityId) -> LoanId {
        let EntityId::Loan(id) = entity else {
            panic!("expected loan")
        };
        id
    }
    fn place(entity: EntityId) -> PlaceId {
        let EntityId::Place(id) = entity else {
            panic!("expected place")
        };
        id
    }
    fn value(entity: EntityId) -> ValueId {
        let EntityId::Value(id) = entity else {
            panic!("expected value")
        };
        id
    }
    fn reference(tokens: &[LoanId], live: bool) -> Reference {
        Reference {
            lineage: tokens.iter().copied().collect(),
            live,
        }
    }
    fn append(function: &mut Function, block: BlockId, op: Operation, ty: EntityType) -> EntityId {
        let (instruction, results) = function
            .append_instruction(block, op, vec![ty], function.origin.clone())
            .unwrap();
        assert_eq!(
            function.entity(results[0]).unwrap().definition,
            Definition::InstructionResult {
                instruction,
                index: 0
            }
        );
        assert_eq!(function.instruction(instruction).unwrap().block, block);
        results[0]
    }
    fn terminate(function: &mut Function, block: BlockId, kind: TerminatorKind) {
        function
            .set_terminator(block, kind, function.origin.clone())
            .unwrap();
    }
    fn end_instruction(function: &mut Function, block: BlockId, id: LoanId) {
        function
            .append_instruction(
                block,
                Operation::BorrowEnd { loan: id },
                vec![],
                function.origin.clone(),
            )
            .unwrap();
    }
    fn parameters(function: &Function, block: BlockId) -> Vec<EntityId> {
        let parameters = function.block(block).unwrap().parameters.clone();
        for (index, entity) in parameters.iter().enumerate() {
            assert_eq!(
                function.entity(*entity).unwrap().definition,
                Definition::BlockParameter { block, index }
            );
        }
        parameters
    }

    #[test]
    fn place_only_seed_retains_non_dominating_ancestor_tokens() {
        let (mut function, ty, int, boolean, origin) = fixture();
        let entry = function
            .add_block(
                vec![ty, EntityType::Value(boolean), EntityType::Value(int)],
                origin.clone(),
            )
            .unwrap();
        let args = parameters(&function, entry);
        let left = function.add_block(vec![], origin.clone()).unwrap();
        let right = function.add_block(vec![], origin.clone()).unwrap();
        let join = function
            .add_block(vec![EntityType::Place(int)], origin)
            .unwrap();
        terminate(
            &mut function,
            entry,
            TerminatorKind::Conditional {
                condition: value(args[1]),
                when_true: Edge {
                    target: left,
                    arguments: vec![],
                },
                when_false: Edge {
                    target: right,
                    arguments: vec![],
                },
            },
        );
        let ancestor = loan(append(
            &mut function,
            left,
            Operation::SharedReborrow {
                source: loan(args[0]),
            },
            ty,
        ));
        let child = loan(append(
            &mut function,
            left,
            Operation::SharedReborrow { source: ancestor },
            ty,
        ));
        let old_place = append(
            &mut function,
            left,
            Operation::ContainerElementPlace {
                owner: EntityId::Loan(child),
                index: value(args[2]),
            },
            EntityType::Place(int),
        );
        end_instruction(&mut function, left, child);
        end_instruction(&mut function, left, ancestor);
        terminate(
            &mut function,
            left,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: vec![old_place],
            }),
        );
        let unrelated = loan(append(
            &mut function,
            right,
            Operation::SharedReborrow {
                source: loan(args[0]),
            },
            ty,
        ));
        let right_place = append(
            &mut function,
            right,
            Operation::ContainerElementPlace {
                owner: EntityId::Loan(unrelated),
                index: value(args[2]),
            },
            EntityType::Place(int),
        );
        end_instruction(&mut function, right, unrelated);
        terminate(
            &mut function,
            right,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: vec![right_place],
            }),
        );
        terminate(&mut function, join, TerminatorKind::Abort);
        let surviving = place(parameters(&function, join)[0]);
        let projection = EntryProjection::new(&function);
        assert!(!projection.dominators[join.index()].contains(&left.index()));
        assert!(!projection.available(&function, join, EntityId::Loan(ancestor)));
        assert!(!projection.available(&function, join, EntityId::Loan(child)));
        // The minimal synthetic place seed forces recursive discovery through child.
        let mut state = State::default();
        state.loans.insert(ancestor, reference(&[ancestor], false));
        state
            .loans
            .insert(child, reference(&[ancestor, child], false));
        state.loans.insert(unrelated, reference(&[unrelated], true));
        state.places.insert(surviving, reference(&[child], false));
        state
            .places
            .insert(place(old_place), reference(&[child], false));
        projection.retain_entry(&function, join, &mut state);
        let expected = State {
            loans: [
                (ancestor, reference(&[ancestor], false)),
                (child, reference(&[ancestor, child], false)),
            ]
            .into(),
            places: [(surviving, reference(&[child], false))].into(),
            cursors: Default::default(),
        };
        assert_eq!(state, expected);
        projection.retain_entry(&function, join, &mut state);
        assert_eq!(state, expected, "entry projection is idempotent");
    }

    #[test]
    fn inactive_alias_survives_parallel_parameter_swap_on_backedge() {
        let (mut function, ty, int, _, origin) = fixture();
        let entry = function
            .add_block(vec![ty, ty, EntityType::Value(int)], origin.clone())
            .unwrap();
        let args = parameters(&function, entry);
        let header = function
            .add_block(vec![ty, ty, EntityType::Place(int)], origin)
            .unwrap();
        let params = parameters(&function, header);
        let initial_place = append(
            &mut function,
            entry,
            Operation::ContainerElementPlace {
                owner: args[1],
                index: value(args[2]),
            },
            EntityType::Place(int),
        );
        terminate(
            &mut function,
            entry,
            TerminatorKind::Branch(Edge {
                target: header,
                arguments: vec![args[0], args[1], initial_place],
            }),
        );
        let backedge = Edge {
            target: header,
            arguments: vec![params[1], params[0], params[2]],
        };
        terminate(
            &mut function,
            header,
            TerminatorKind::Branch(backedge.clone()),
        );
        let (left, right) = (loan(args[0]), loan(args[1]));
        let (first, second, carried_place) = (loan(params[0]), loan(params[1]), place(params[2]));
        let mut predecessor = State::default();
        predecessor.loans.insert(left, reference(&[left], false));
        predecessor.loans.insert(right, reference(&[right], true));
        predecessor.loans.insert(first, reference(&[left], false));
        predecessor.loans.insert(second, reference(&[right], true));
        predecessor
            .places
            .insert(carried_place, reference(&[right], true));
        // Projection consumes an already simultaneous edge snapshot; this is not a
        // test of the provider verifier's separate edge-copy implementation.
        let mut state = predecessor.clone();
        for (from, to) in backedge.arguments.iter().zip(&params) {
            if let (EntityId::Loan(from), EntityId::Loan(to)) = (from, to) {
                state.loans.insert(*to, predecessor.loans[from].clone());
            }
        }
        EntryProjection::new(&function).retain_entry(&function, header, &mut state);
        let mut expected = predecessor;
        expected.loans.insert(first, reference(&[right], true));
        expected.loans.insert(second, reference(&[left], false));
        assert_eq!(state, expected);
        state.end(first);
        expected.loans.insert(right, reference(&[right], false));
        expected.loans.insert(first, reference(&[right], false));
        expected
            .places
            .insert(carried_place, reference(&[right], false));
        assert_eq!(
            state, expected,
            "ending the swapped alias still invalidates its place"
        );
    }

    #[test]
    fn current_block_old_definition_survives_only_when_lineage_references_it() {
        let (mut function, ty, int, _, origin) = fixture();
        let entry = function
            .add_block(vec![ty, EntityType::Value(int)], origin.clone())
            .unwrap();
        let args = parameters(&function, entry);
        let initial_alias = append(
            &mut function,
            entry,
            Operation::SharedReborrow {
                source: loan(args[0]),
            },
            ty,
        );
        let initial_place = append(
            &mut function,
            entry,
            Operation::ContainerElementPlace {
                owner: initial_alias,
                index: value(args[1]),
            },
            EntityType::Place(int),
        );
        let header = function
            .add_block(vec![ty, ty, EntityType::Place(int)], origin)
            .unwrap();
        let params = parameters(&function, header);
        terminate(
            &mut function,
            entry,
            TerminatorKind::Branch(Edge {
                target: header,
                arguments: vec![args[0], initial_alias, initial_place],
            }),
        );
        let (root, source, alias, carried_place) = (
            loan(args[0]),
            loan(params[0]),
            loan(params[1]),
            place(params[2]),
        );
        end_instruction(&mut function, header, alias);
        let old = loan(append(
            &mut function,
            header,
            Operation::SharedReborrow { source },
            ty,
        ));
        let unrelated = loan(append(
            &mut function,
            header,
            Operation::SharedReborrow { source },
            ty,
        ));
        end_instruction(&mut function, header, unrelated);
        terminate(
            &mut function,
            header,
            TerminatorKind::Branch(Edge {
                target: header,
                arguments: vec![params[0], EntityId::Loan(old), params[2]],
            }),
        );
        let projection = EntryProjection::new(&function);
        assert!(!projection.available(&function, header, EntityId::Loan(old)));
        let mut state = State::default();
        for id in [root, source] {
            state.loans.insert(id, reference(&[root], true));
        }
        for id in [old, alias] {
            state.loans.insert(id, reference(&[root, old], true));
        }
        state
            .loans
            .insert(unrelated, reference(&[root, unrelated], false));
        state
            .places
            .insert(carried_place, reference(&[root, old], true));
        let mut expected = state.clone();
        expected.loans.remove(&unrelated);
        projection.retain_entry(&function, header, &mut state);
        assert_eq!(state, expected);
        // With both observable references removed, the previous instruction result
        // is discarded even though its block dominates itself.
        let mut unreferenced = state.clone();
        unreferenced.loans.remove(&alias);
        unreferenced.places.clear();
        projection.retain_entry(&function, header, &mut unreferenced);
        assert!(!unreferenced.loans.contains_key(&old));
        state.end(alias);
        for id in [old, alias] {
            expected.loans.insert(id, reference(&[root, old], false));
        }
        expected
            .places
            .insert(carried_place, reference(&[root, old], false));
        assert_eq!(
            state, expected,
            "original token identity still drives end(alias)"
        );
    }

    #[test]
    fn nullable_non_null_proof_birth_is_retained_at_its_entry() {
        let (_, _, _, _, origin) = fixture();
        let mut program = Program::default();
        let module_id = program.add_module("nullable-projection");
        let module = program.module_mut(module_id).unwrap();
        let payload = module.add_aggregate_type("Node.payload", vec![]).unwrap();
        let inner = module.declare_heap_owner("Node").unwrap();
        module.define_heap_owner(inner, payload).unwrap();
        let nullable = module.add_nullable_handle_type(inner).unwrap();
        let id = module
            .add_function("project", vec![], origin.clone())
            .unwrap();
        let function = module.function_mut(id).unwrap();
        let entry = function
            .add_block(vec![EntityType::Value(nullable)], origin.clone())
            .unwrap();
        let owner = parameters(function, entry)[0];
        let null = function
            .add_block(vec![EntityType::Value(nullable)], origin.clone())
            .unwrap();
        let non_null = function
            .add_block(
                vec![
                    EntityType::Value(nullable),
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: inner,
                    },
                ],
                origin,
            )
            .unwrap();
        let view = loan(parameters(function, non_null)[1]);
        terminate(
            function,
            entry,
            TerminatorKind::NullableBranch {
                owner: value(owner),
                view,
                when_null: Edge {
                    target: null,
                    arguments: vec![owner],
                },
                when_non_null: Edge {
                    target: non_null,
                    arguments: vec![owner],
                },
            },
        );
        terminate(function, null, TerminatorKind::Abort);
        terminate(function, non_null, TerminatorKind::Abort);
        // This fixture also has a complete, valid verifier baseline; the synthetic
        // state below specifically isolates preservation after the proof's birth.
        crate::ssa::verify::verify_program(&program).expect("nullable baseline");
        let function = program.module(module_id).unwrap().function(id).unwrap();
        let projection = EntryProjection::new(function);
        assert!(projection.available(function, non_null, EntityId::Loan(view)));
        assert!(!projection.available(function, null, EntityId::Loan(view)));
        let mut state = State {
            loans: [(view, reference(&[view], true))].into(),
            places: Default::default(),
            cursors: Default::default(),
        };
        let born = state.clone();
        projection.retain_entry(function, non_null, &mut state);
        assert_eq!(state, born);
        projection.retain_entry(function, null, &mut state);
        assert_eq!(
            state,
            State::default(),
            "proof does not survive on null entry"
        );
    }
}
