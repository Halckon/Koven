//! Snapshot, transport and cleanup of a compiler-bound shared provider.
use super::*;
impl ExpressionLowerer<'_> {
    pub(in super::super) fn lower_for(
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

        if matches!(
            descriptor.provider(),
            lang_frontend::type_checking::IterationProvider::NumericRange(_)
        ) {
            if plan.source_kind()
                != &lang_frontend::ownership_checking::IterationSource::OwnedSnapshot(
                    descriptor.source(),
                )
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            return self.lower_numeric_for(statement, source, body, span, plan);
        }
        self.qualify_borrowed_while_entry(statement, span)?;
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
        let source_loan = if descriptor.provider()
            == lang_frontend::type_checking::IterationProvider::RangeView
        {
            let loan = self.range_iteration_loan(source, container_ssa_type, span)?;
            if !self
                .borrow_bindings
                .values()
                .any(|existing| *existing == loan)
            {
                new_source_loan = Some(loan);
            }
            loan
        } else {
            match plan
                .source()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            {
                lang_frontend::ownership_checking::LoanTarget::Place(target)
                    if target.is_root() =>
                {
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
            }
        };

        let preheader = self.block;
        let snapshot = crate::ssa::provider::snapshot(
            self.function,
            self.block,
            source_loan,
            int_type,
            &Origin::Source(span),
            descriptor.provider(),
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
                    crate::ssa::carry::seed_slot(
                        &mut carried.slots,
                        LinearBindingSlot::new(Some(symbol), source_ent, ty),
                    );
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
            descriptor.provider(),
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

        self.record_map_loop_carriers(
            statement,
            [preheader, header_block, body_block, exit_block],
            &carried,
            span,
        )?;
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
            outer_borrowed_while: None,
            map_loop: None,
            header: header_block,
            carried: carried.clone(),
            continues: Vec::new(),
            breaks: Vec::new(),
            numeric_loop: None,
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
                statement,
                lang_frontend::ownership_checking::IterationPhiIncomingKind::Fallthrough,
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
        self.finish_map_loop_carriers(statement, span)?;

        let context = self.loops.pop().expect("for loop context must be balanced");
        let mut exits = context.breaks;
        let false_exit = self.loop_exit();
        exits.push(false_exit);

        let result = self.merge_exits(exits, &baseline, span)?;
        self.finish_map_loop_merge(statement, span)?;
        Ok(result)
    }
}

#[cfg(test)]
mod numeric_cleanup_tests {
    //! Real numeric-loop SSA, with direct private transport/exhaustion guard oracles.
    use crate::ssa::lower_frontend::cleanup_guard_harness::{self as harness, Fixture};
    use crate::ssa::lower_frontend::nullable_numeric_cleanup::*;
    use crate::ssa::lower_frontend::*;

    fn checkpoint<T>(
        fixture: &Fixture,
        fact: DropFact,
        check: impl FnOnce(&mut ExpressionLowerer<'_>, &mut Plan, Pair, Pair) -> T,
    ) -> T {
        let mut plan = candidate(
            &fixture.parsed,
            &fixture.names,
            &fixture.typed,
            &fixture.owned,
            fact,
        )
        .unwrap();
        let mut function = fixture.entry().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            let birth = harness::outer_context(lowerer, plan.root, plan.outer);
            let anchor = lowerer.numeric_while_birth(plan.root, birth).unwrap();
            let TerminatorKind::Conditional { when_true, .. } = &lowerer
                .function
                .block(anchor.block)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .kind
            else {
                panic!("numeric empty-range split")
            };
            let block = when_true.target;
            let owner =
                harness::before_cleanup(lowerer, block, plan.root, fact.value_origin(), Some(birth));
            let proof = lowerer.non_null_bindings[&plan.root];
            let current = Pair {
                block,
                owner,
                loan: proof.loan,
            };
            assert!(transport(
                lowerer.function,
                current,
                anchor,
                proof.inner,
                lowerer.ssa_types
            ));
            assert!(numeric_exit(lowerer, &plan, current, anchor));
            assert_eq!(
                lowerer.checked_nullable_numeric_owner(&plan).unwrap(),
                owner
            );
            check(lowerer, &mut plan, current, anchor)
        })
    }

    fn incoming_edge<'a>(lowerer: &'a mut ExpressionLowerer<'_>, current: Pair) -> &'a mut Edge {
        let (block, _) = harness::incoming(lowerer.function, current.block)
            .into_iter()
            .find(|(block, _)| *block != lowerer.loops[0].header)
            .unwrap();
        harness::edges_mut(
            &mut lowerer.function.blocks[block.index()]
                .terminator
                .as_mut()
                .unwrap()
                .kind,
        )
        .into_iter()
        .find(|edge| edge.target == current.block)
        .unwrap()
    }

    // Check every earlier transport gate independently before claiming an SCC witness.
    fn transport_shapes(
        lowerer: &ExpressionLowerer<'_>,
        current: Pair,
        anchor: Pair,
        inner: SsaTypeId,
    ) -> BTreeMap<Pair, Vec<Pair>> {
        let mut graph = BTreeMap::new();
        let mut work = vec![current];
        while let Some(pair) = work.pop() {
            if graph.contains_key(&pair) {
                continue;
            }
            let (owner, loan) = pair_columns(lowerer.function, pair, inner, lowerer.ssa_types)
                .expect("real, correctly typed parameter columns");
            if pair == anchor {
                graph.insert(pair, Vec::new());
                continue;
            }
            let edges = incoming(lowerer.function, pair.block);
            assert!(!edges.is_empty());
            let mut parents = Vec::new();
            for (block, edge) in edges {
                assert!(matches!(
                    lowerer
                        .function
                        .block(block)
                        .unwrap()
                        .terminator
                        .as_ref()
                        .unwrap()
                        .kind,
                    TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. }
                ));
                assert_eq!(
                    edge.arguments.len(),
                    lowerer
                        .function
                        .block(edge.target)
                        .unwrap()
                        .parameters
                        .len()
                );
                for (&source, &target) in edge
                    .arguments
                    .iter()
                    .zip(&lowerer.function.block(edge.target).unwrap().parameters)
                {
                    assert_eq!(
                        lowerer.function.entity(source).unwrap().ty,
                        lowerer.function.entity(target).unwrap().ty
                    );
                }
                let (EntityId::Value(value), EntityId::Loan(view)) =
                    (edge.arguments[owner], edge.arguments[loan])
                else {
                    panic!("paired edge operands")
                };
                assert_eq!(
                    parameter(lowerer.function, EntityId::Value(value))
                        .unwrap()
                        .0,
                    block
                );
                let parent = Pair {
                    block,
                    owner: value,
                    loan: view,
                };
                assert!(pair_columns(lowerer.function, parent, inner, lowerer.ssa_types).is_some());
                for entity in [EntityId::Value(value), EntityId::Loan(view)] {
                    assert_eq!(
                        edge.arguments
                            .iter()
                            .filter(|&&other| other == entity)
                            .count(),
                        1
                    );
                }
                parents.push(parent);
                work.push(parent);
            }
            graph.insert(pair, parents);
        }
        graph
    }

    #[test]
    fn nullable_numeric_cleanup_rejects_columns_ungrounded_cycles_and_false_exhaustion() {
        let original = crate::ssa::while_flow_tests::proof_cases::numeric_nested_source();
        for text in [
            original.clone(),
            original.replace("in 0..<2", "in ((0..<2))"),
        ] {
            let fixture = Fixture::analyze(&text);
            assert_eq!(fixture.program.modules[0].functions.len(), 1);
            assert!(fixture.entry().receiver.is_none());
            assert!(fixture.typed.type_parameters().is_empty());
            assert!(
                fixture
                    .parsed
                    .ast()
                    .expressions()
                    .iter()
                    .all(|(_, node)| !matches!(node.payload(), Expression::Lambda { .. }))
            );
            assert!(
                fixture
                    .entry()
                    .instructions
                    .iter()
                    .all(|instruction| !matches!(instruction.operation, Operation::DirectCall { .. }))
            );
            assert_eq!(
                fixture.entry().return_types,
                [fixture.type_ids[&fixture.typed.types().builtin(BuiltinType::Int).unwrap()]]
            );
            let facts = fixture
                .owned
                .drops()
                .iter()
                .copied()
                .filter(|&fact| {
                    candidate(
                        &fixture.parsed,
                        &fixture.names,
                        &fixture.typed,
                        &fixture.owned,
                        fact,
                    )
                    .is_some()
                })
                .collect::<Vec<_>>();
            let [fact] = facts.as_slice() else {
                panic!("one real numeric Exit owner fact")
            };
            for &other in fixture
                .owned
                .drops()
                .iter()
                .filter(|other| **other != *fact)
            {
                assert!(
                    candidate(
                        &fixture.parsed,
                        &fixture.names,
                        &fixture.typed,
                        &fixture.owned,
                        other
                    )
                    .is_none()
                );
            }
            for mutation in 0..8 {
                checkpoint(&fixture, *fact, |lowerer, plan, mut current, anchor| {
                    let proof = lowerer.non_null_bindings[&plan.root];
                    let owner_column =
                        harness::parameter(lowerer.function, EntityId::Value(current.owner)).1;
                    let loan_column =
                        harness::parameter(lowerer.function, EntityId::Loan(current.loan)).1;
                    match mutation {
                        0 => current.owner = anchor.owner,
                        1 => current.loan = anchor.loan,
                        2 | 3 => {
                            let edge = incoming_edge(lowerer, current);
                            edge.arguments.push(
                                edge.arguments[if mutation == 2 {
                                    owner_column
                                } else {
                                    loan_column
                                }],
                            );
                        }
                        4 | 5 => {
                            incoming_edge(lowerer, current)
                                .arguments
                                .remove(if mutation == 4 {
                                    owner_column
                                } else {
                                    loan_column
                                });
                        }
                        6 => lowerer.function.blocks[anchor.block.index()].terminator = None,
                        7 => {
                            // Retain an actual loop SCC but delete its only real entry.
                            lowerer.function.blocks[anchor.block.index()]
                                .terminator
                                .as_mut()
                                .unwrap()
                                .kind = TerminatorKind::Abort;
                        }
                        _ => unreachable!(),
                    }
                    assert!(
                        !transport(
                            lowerer.function,
                            current,
                            anchor,
                            proof.inner,
                            lowerer.ssa_types
                        ),
                        "transport mutation {mutation}"
                    );
                    if mutation < 2 {
                        let proof = lowerer.non_null_bindings.get_mut(&plan.root).unwrap();
                        proof.owner = current.owner;
                        proof.loan = current.loan;
                        lowerer
                            .bindings
                            .insert(plan.root, LoweredValue::Value(current.owner));
                    }
                    harness::unsupported(
                        lowerer.checked_nullable_numeric_owner(plan),
                        fact.value_origin(),
                    );
                });
            }
            checkpoint(&fixture, *fact, |lowerer, plan, current, anchor| {
                // Composite graph counterexample: one actual anchored predecessor and
                // an independent self-loop, with every paired-column check still valid.
                let first = lowerer
                    .function
                    .block(anchor.block)
                    .unwrap()
                    .terminator
                    .as_ref()
                    .unwrap()
                    .clone();
                let TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } = &first.kind
                else {
                    unreachable!()
                };
                let header = when_false.target;
                let TerminatorKind::Conditional { condition, .. } = lowerer
                    .function
                    .block(header)
                    .unwrap()
                    .terminator
                    .as_ref()
                    .unwrap()
                    .kind
                else {
                    panic!("real item comparison")
                };
                let parameters = lowerer.function.block(header).unwrap().parameters.clone();
                let current_parameters = &lowerer.function.block(current.block).unwrap().parameters;
                let exit_arguments = parameters[..current_parameters.len()].to_vec();
                for (&source, &target) in exit_arguments.iter().zip(current_parameters) {
                    assert_eq!(
                        lowerer.function.entity(source).unwrap().ty,
                        lowerer.function.entity(target).unwrap().ty
                    );
                }
                for block in &mut lowerer.function.blocks {
                    if block.id != anchor.block
                        && block.id != header
                        && block.terminator.as_ref().is_some_and(|term| {
                            harness::edges(&term.kind)
                                .iter()
                                .any(|edge| [current.block, header].contains(&edge.target))
                        })
                    {
                        block.terminator.as_mut().unwrap().kind = TerminatorKind::Abort;
                    }
                }
                lowerer.function.blocks[anchor.block.index()]
                    .terminator
                    .as_mut()
                    .unwrap()
                    .kind = TerminatorKind::Branch(when_true.clone());
                lowerer.function.blocks[header.index()]
                    .terminator
                    .as_mut()
                    .unwrap()
                    .kind = TerminatorKind::Conditional {
                    condition,
                    when_true: Edge {
                        target: current.block,
                        arguments: exit_arguments,
                    },
                    when_false: Edge {
                        target: header,
                        arguments: parameters,
                    },
                };
                let proof = lowerer.non_null_bindings[&plan.root];
                let graph = transport_shapes(lowerer, current, anchor, proof.inner);
                assert_eq!(graph.len(), 3);
                assert!(graph.contains_key(&anchor));
                let (&cycle, parents) = graph.iter().find(|(pair, _)| pair.block == header).unwrap();
                assert!(
                    parents.as_slice() == [cycle],
                    "only the independent SCC lacks a path to birth"
                );
                assert!(!transport(
                    lowerer.function,
                    current,
                    anchor,
                    proof.inner,
                    lowerer.ssa_types
                ));
                // Repair only the missing actual entry. Identical column/edge shapes
                // now succeed, isolating the all-visited-nodes anchoring guard.
                lowerer.function.blocks[anchor.block.index()].terminator = Some(first);
                transport_shapes(lowerer, current, anchor, proof.inner);
                assert!(transport(
                    lowerer.function,
                    current,
                    anchor,
                    proof.inner,
                    lowerer.ssa_types
                ));
            });
            for mutation in 0..5 {
                checkpoint(&fixture, *fact, |lowerer, plan, current, anchor| {
                    match mutation {
                        0 => plan.statement = plan.outer,
                        1 => plan.outer = plan.statement,
                        2 => plan.root = fixture.symbol("initial"),
                        3 => {
                            lowerer.function.blocks[current.block.index()].origin = Origin::Source(
                                lowerer
                                    .parsed
                                    .ast()
                                    .statements()
                                    .get(plan.outer)
                                    .unwrap()
                                    .span(),
                            )
                        }
                        4 => {
                            let block = harness::incoming(lowerer.function, current.block)
                                .into_iter()
                                .find(|(block, _)| *block != anchor.block)
                                .unwrap()
                                .0;
                            lowerer.function.blocks[block.index()]
                                .terminator
                                .as_mut()
                                .unwrap()
                                .origin = Origin::Source(fact.value_origin());
                        }
                        _ => unreachable!(),
                    }
                    if mutation != 1 {
                        assert!(!numeric_exit(lowerer, plan, current, anchor));
                    }
                    harness::unsupported(
                        lowerer.checked_nullable_numeric_owner(plan),
                        fact.value_origin(),
                    );
                });
            }
            checkpoint(&fixture, *fact, |lowerer, plan, current, anchor| {
                assert!(
                    !numeric_exit(lowerer, plan, anchor, anchor),
                    "a real same-typed birth pair is not exhaustion"
                );
                assert!(lowerer.emit_nullable_numeric_cleanup(*fact).unwrap());
                assert!(!lowerer.bindings.contains_key(&plan.root));
                assert!(!lowerer.non_null_bindings.contains_key(&plan.root));
                let ops = &lowerer.function.block(lowerer.block).unwrap().instructions;
                assert_eq!(
                    lowerer
                        .function
                        .instruction(ops[ops.len() - 2])
                        .unwrap()
                        .operation,
                    Operation::BorrowEnd { loan: current.loan }
                );
                assert_eq!(
                    lowerer
                        .function
                        .instruction(ops[ops.len() - 1])
                        .unwrap()
                        .operation,
                    Operation::Drop {
                        owner: current.owner
                    }
                );
            });
        }
    }
}
