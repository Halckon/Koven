//! Actual source/SSA checkpoints for the private borrowed-for relation.
use super::*;
use crate::ssa::unit_lower::cfg::carried_control_edge;

impl UnitExpressionLowerer<'_> {
    /// Recover the original context from its actual preheader edge. The helper
    /// creates no block or entity and checks the complete ordered carrier tuple.
    pub(in crate::ssa::unit_lower) fn guard_source_loop_context(
        &mut self,
        header: BlockId,
        live: &[UnitSymbolId],
        pending: Vec<EntityId>,
        proof_scope: Option<UnitStatementId>,
        span: Span,
    ) -> LoopContext {
        let saved = self.loop_state();
        let entry = self.function.entry_block().unwrap();
        let TerminatorKind::Branch(edge) = self
            .function
            .block(entry)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
            .clone()
        else {
            panic!("real source preheader")
        };
        assert_eq!(edge.target, header);
        let mut roots = live.to_vec();
        roots.sort();
        self.block = entry;
        self.bindings = roots
            .iter()
            .zip(&edge.arguments)
            .map(|(root, entity)| {
                let EntityId::Value(value) = entity else {
                    panic!("original named owner/value prefix")
                };
                assert_eq!(
                    self.function.entity(*entity).unwrap().ty,
                    EntityType::Value(self.type_ids[&self.typed.symbol_type(*root).unwrap()])
                );
                (*root, LoweredValue::Value(*value))
            })
            .collect();
        let booleans = self
            .function
            .block(entry)
            .unwrap()
            .parameters
            .iter()
            .filter_map(|entity| match self.function.entity(*entity).unwrap().ty {
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                } if self.ssa_types[target.index()] == SsaTypeKind::Boolean => Some(*entity),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(booleans.len(), self.entry_boolean_roots.len());
        self.borrow_bindings = self
            .entry_boolean_roots
            .iter()
            .zip(booleans)
            .map(|(root, entity)| {
                let EntityId::Loan(loan) = entity else {
                    unreachable!()
                };
                (*root, loan)
            })
            .collect();
        // At these source preheaders all owners are already named. No call,
        // nullable proof, lambda or temporary expression is in progress.
        self.nullable_proofs.clear();
        self.pending_operands = pending;
        self.pending_call_frames.clear();
        assert!(self.temporaries.is_empty());
        assert!(self.capture_loans.is_empty() && self.result_source_loans.is_empty());
        assert!(self.current_receiver.is_none() && self.consumed_receiver.is_none());
        let mut carried = self.carried_bindings(&self.bindings, span).unwrap();
        let mut loans = self.carried_loans(&self.borrow_bindings, span).unwrap();
        self.carry_pending_operands(&mut carried, &mut loans, span)
            .unwrap();
        assert_eq!(carried_control_edge(header, &carried, &loans), edge);
        let context = LoopContext {
            proof_scope,
            borrowed_for_while: None,
            borrowed_while_body: None,
            iteration: None,
            header,
            carried,
            loans,
            entry_temporaries: self.temporaries.keys().copied().collect(),
            entry_pending_count: self.pending_operands.len(),
            entry_receiver: self.current_receiver,
            entry_consumed_receiver: self.consumed_receiver,
            entry_symbols: self.bindings.keys().copied().collect(),
            entry_closure_bindings: self.closure_bindings.clone(),
            continues: Vec::new(),
            breaks: Vec::new(),
        };
        self.restore_loop_state(&saved);
        context
    }
}

use crate::ssa::model::BinaryOperator;
use crate::ssa::unit_lower::nullable_proof::guard_tests::support::{Fixture, rebound, rejected};
mod checkpoint;
use checkpoint::Shape;

#[test]
fn unit_borrowed_for_while_guards_preserve_current_roles() {
    for source in
        crate::ssa::while_flow_tests::proof_cases::borrowed_proof_cases::while_inside_borrowed_for_sources()
    {
        let fixture = Fixture::new(&source);
        let original = fixture.function();
        for case in 0..36 {
            let mut function = original.clone();
            fixture.with_lowerer(&mut function, |lowerer| {
                let shape = Shape::read(&fixture, lowerer);
                let (entry, mut token) = shape.qualify(lowerer, original);
                let span = shape.inner_span;
                if case < 4 {
                    // qualify itself has already succeeded on this exact prefix.
                    match case {
                        0 => lowerer.loops[0].borrowed_while_body = Some(Box::new(token.clone())),
                        1 => lowerer.loops[0].entry_pending_count = 4,
                        2 => lowerer.loops[0].iteration.as_mut().unwrap().statement = shape.scope,
                        3 => lowerer.loops[0].proof_scope = Some(shape.scope),
                        _ => unreachable!(),
                    }
                    rejected(
                        BorrowedForWhile::qualify(lowerer, &entry, span),
                        LoweringErrorKind::UnsupportedNode,
                        span,
                    );
                    return;
                }
                shape.active(lowerer, original, &mut token);
                if case < 10 {
                    match case {
                        4 => {
                            let mut changed = token.clone();
                            changed.scope = shape.outer;
                            lowerer.loops[0].borrowed_while_body = Some(Box::new(changed));
                        }
                        5 => lowerer.loops[0].entry_pending_count = 4,
                        6 => lowerer.loops[1].proof_scope = Some(shape.outer),
                        7 => lowerer.loops[1].header = shape.header,
                        8 => {
                            lowerer.loops[1].borrowed_for_while.as_mut().unwrap().jump =
                                UnitExpressionId::new(lowerer.source_unit, shape.condition)
                        }
                        9 => lowerer.loops[0].iteration.as_mut().unwrap().statement = shape.scope,
                        _ => unreachable!(),
                    }
                    rejected(
                        token.validate(lowerer, &lowerer.loop_state(), true, false, span),
                        LoweringErrorKind::UnsupportedNode,
                        span,
                    );
                    return;
                }
                if case < 17 {
                    let before = lowerer.loop_state();
                    let current =
                        roles(&before, token.items, token.current, token.item, true, span).unwrap();
                    let view = EntityId::Loan(token.proof.unwrap().loan);
                    token
                        .transport(
                            lowerer,
                            lowerer.block,
                            &current,
                            Some(view),
                            None,
                            &mut BTreeSet::new(),
                            span,
                        )
                        .unwrap();
                    match case {
                        10 => {
                            lowerer.pending_operands.pop();
                        }
                        11 => {
                            assert_eq!(
                                lowerer.function.entity(current[3]).unwrap().ty,
                                lowerer.function.entity(current[4]).unwrap().ty
                            );
                            lowerer.pending_operands.swap(1, 2);
                        }
                        12 => {
                            assert_ne!(current[2], shape.six[2]);
                            assert_eq!(
                                lowerer.function.entity(current[2]).unwrap().ty,
                                lowerer.function.entity(shape.six[2]).unwrap().ty
                            );
                            lowerer.pending_operands[0] = shape.six[2];
                        }
                        13 => {
                            assert_ne!(current[5], shape.six[5]);
                            assert_eq!(
                                lowerer.function.entity(current[5]).unwrap().ty,
                                lowerer.function.entity(shape.six[5]).unwrap().ty
                            );
                            lowerer.pending_operands[3] = shape.six[5];
                            let EntityId::Loan(element) = shape.six[5] else {
                                panic!()
                            };
                            lowerer.borrow_bindings.insert(shape.item, element);
                        }
                        14 => {
                            // A genuine birth tuple cannot stand in for current columns.
                            rejected(
                                token.transport(
                                    lowerer,
                                    lowerer.block,
                                    &shape.six,
                                    Some(view),
                                    None,
                                    &mut BTreeSet::new(),
                                    span,
                                ),
                                LoweringErrorKind::MissingFact,
                                span,
                            );
                            return;
                        }
                        15 | 16 => {
                            let ended = if case == 15 {
                                current[2]
                            } else {
                                view
                            };
                            let EntityId::Loan(loan) = ended else {
                                panic!()
                            };
                            lowerer
                                .function
                                .append_instruction(
                                    lowerer.block,
                                    Operation::BorrowEnd { loan },
                                    Vec::new(),
                                    Origin::Source(span),
                                )
                                .unwrap();
                        }
                        _ => unreachable!(),
                    }
                    rejected(
                        token.validate(lowerer, &lowerer.loop_state(), true, false, span),
                        LoweringErrorKind::MissingFact,
                        span,
                    );
                    return;
                }
                if case < 22 {
                    // bind_proof already accepted the real NullableBranch above.
                    match case {
                        17 => token.scope = shape.outer,
                        18 => token.inner = shape.int_type,
                        19 => {
                            let EntityId::Loan(source) = lowerer.pending_operands[0] else {
                                panic!()
                            };
                            lowerer
                                .nullable_proofs
                                .get_mut(&token.current)
                                .unwrap()
                                .loan = source;
                        }
                        20 | 21 => {
                            let term = &mut lowerer.function.blocks[shape.inner_header.index()]
                                .terminator
                                .as_mut()
                                .unwrap()
                                .kind;
                            let TerminatorKind::NullableBranch {
                                view,
                                when_null,
                                when_non_null,
                                ..
                            } = term
                            else {
                                panic!()
                            };
                            if case == 20 {
                                let EntityId::Loan(source) = shape.six[2] else {
                                    panic!()
                                };
                                *view = source;
                            } else {
                                when_non_null.target = when_null.target;
                            }
                        }
                        _ => unreachable!(),
                    }
                    rejected(
                        token.bind_proof(lowerer, span),
                        LoweringErrorKind::MissingFact,
                        span,
                    );
                    return;
                }
                let proof = token.proof.unwrap();
                lowerer.loops.truncate(1);
                token.proof = None;
                let advanced = case >= 26;
                shape.at_end(lowerer, original, &mut token, advanced);
                let span = shape.span;
                if !advanced {
                    match case {
                        22 => lowerer.function.blocks[shape.back_block.index()]
                            .instructions
                            .retain(|instruction| *instruction != shape.element_end),
                        23 => lowerer.function.blocks[shape.back_block.index()]
                            .instructions
                            .push(shape.element_end),
                        24 => {
                            let EntityId::Loan(loan) = lowerer.pending_operands[0] else {
                                panic!()
                            };
                            lowerer.function.instructions[shape.element_end.index()].operation =
                                Operation::BorrowEnd { loan };
                        }
                        25 => {
                            lowerer.pending_operands.pop();
                        }
                        _ => unreachable!(),
                    }
                    rejected(
                        token.validate_element_end(lowerer, span),
                        LoweringErrorKind::MissingFact,
                        span,
                    );
                    return;
                }
                // Real production checkpoint: the outer context is popped after
                // recording its sole natural continue and before edge emission.
                let mut context = lowerer.loops.pop().unwrap();
                let mut state = lowerer.loop_state();
                let mut edge = shape.back_edge.clone();
                token
                    .validate_backedge(lowerer, &context, &state, &edge, span)
                    .unwrap();
                match case {
                    26 => edge.target = shape.inner_header,
                    27 => context.header = shape.inner_header,
                    28 => {
                        let instruction =
                            definition(lowerer, state.pending_operands[2]).unwrap().id;
                        let Operation::Binary { right, .. } =
                            &mut lowerer.function.instructions[instruction.index()].operation
                        else {
                            panic!()
                        };
                        *right = shape.cursor_before;
                    }
                    29 => token.step = shape.cursor_before,
                    30 => {
                        let column = edge
                            .arguments
                            .iter()
                            .position(|entity| *entity == state.pending_operands[0])
                            .unwrap();
                        edge.arguments[column] = shape.six[2];
                    }
                    31 => {
                        state.nullable_proofs.insert(token.current, proof);
                    }
                    32 => state.pending_operands.push(rebound(
                        original,
                        shape.six[5],
                        shape.back_block,
                    )),
                    33 => edge.arguments.push(EntityId::Loan(proof.loan)),
                    34 => {
                        let length = edge
                            .arguments
                            .iter()
                            .position(|entity| *entity == state.pending_operands[1])
                            .unwrap();
                        let cursor = edge
                            .arguments
                            .iter()
                            .position(|entity| *entity == state.pending_operands[2])
                            .unwrap();
                        assert_eq!(
                            lowerer.function.entity(edge.arguments[length]).unwrap().ty,
                            lowerer.function.entity(edge.arguments[cursor]).unwrap().ty
                        );
                        edge.arguments.swap(length, cursor);
                    }
                    35 => {
                        let instruction =
                            definition(lowerer, EntityId::Value(shape.step)).unwrap().id;
                        lowerer.function.instructions[instruction.index()].operation =
                            Operation::Constant(ScalarConstant::Integer(0));
                    }
                    _ => unreachable!(),
                }
                rejected(
                    token.validate_backedge(lowerer, &context, &state, &edge, span),
                    LoweringErrorKind::MissingFact,
                    span,
                );
            });
        }
    }
}
