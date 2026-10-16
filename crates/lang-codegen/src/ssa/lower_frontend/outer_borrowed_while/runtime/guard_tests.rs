//! CI-only guards recover consumer checkpoints from a successful ordinary-source graph.
//! This is a graph-derived checkpoint, not a captured production lowering stack.
use super::*;
use crate::ssa::{
    lower_frontend::{
        cleanup_guard_harness::{Fixture, incoming, parameter, unsupported},
        control::LinearBindingSlot,
        loop_control::{ForLoopData, LoopContext},
        non_null_proof::ProofScope,
    },
    model::{BinaryOperator, InstructionId},
    provider::{GuardedElement, ProviderHeader, guard_and_begin},
    while_flow_tests::proof_cases::borrowed_proof_cases::while_inside_borrowed_for_sources,
};

struct Shape {
    relation: Relation,
    preheader: BlockId,
    header: BlockId,
    body: Edge,
    exhausted: Edge,
    initial: ValueId,
    items: ValueId,
    source: LoanId,
    element: LoanId,
    body_source: LoanId,
    body_cursor: ValueId,
    length: ValueId,
    cursor: ValueId,
    step: ValueId,
    span: Span,
    breaking: InstructionId,
}

impl Shape {
    fn read(fixture: &Fixture) -> Self {
        let function = fixture.entry();
        let relations = fixture
            .owned
            .drops()
            .iter()
            .filter_map(|fact| {
                candidate(
                    &fixture.parsed,
                    &fixture.names,
                    &fixture.typed,
                    &fixture.owned,
                    *fact,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(relations.len(), 2);
        assert!(relations.iter().all(|relation| relation == &relations[0]));
        let relation = relations[0].clone();
        let span = fixture
            .parsed
            .ast()
            .statements()
            .get(relation.outer)
            .unwrap()
            .span();
        let initial =
            provider::value(function.blocks[0].parameters[relation.parameter_slot]).unwrap();
        let construct = unique(function.instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::ContainerConstruct { .. })
        }))
        .unwrap();
        let items = provider::value(construct.results[0]).unwrap();
        let root =
            unique(function.instructions.iter().filter(|instruction| {
                instruction.operation == Operation::RootPlace { owner: items }
            }))
            .unwrap();
        let EntityId::Place(place) = root.results[0] else {
            unreachable!()
        };
        let begin = unique(function.instructions.iter().filter(|instruction| {
            instruction.operation
                == Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                }
        }))
        .unwrap();
        let source = provider::loan(begin.results[0]).unwrap();
        let preheader = root.block;
        let TerminatorKind::Branch(entry) = &function
            .block(preheader)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
        else {
            unreachable!()
        };
        let header = entry.target;
        let header_term = function.block(header).unwrap().terminator.as_ref().unwrap();
        let TerminatorKind::Conditional {
            condition,
            when_true: body,
            when_false: exhausted,
        } = &header_term.kind
        else {
            unreachable!()
        };
        let compare = super::super::carrier::result(function, EntityId::Value(*condition)).unwrap();
        let Operation::Compare {
            operator: ComparisonOperator::LessThan,
            left: cursor,
            right: length,
        } = compare.operation
        else {
            unreachable!()
        };
        let step = provider::value(
            unique(
                instructions(function, preheader)
                    .unwrap()
                    .into_iter()
                    .filter(|instruction| {
                        instruction.operation == Operation::Constant(ScalarConstant::Integer(1))
                            && instruction.origin == Origin::Source(span)
                    }),
            )
            .unwrap()
            .results[0],
        )
        .unwrap();
        let element_place = unique(
            instructions(function, body.target)
                .unwrap()
                .into_iter()
                .filter(|instruction| {
                    matches!(
                        instruction.operation,
                        Operation::ContainerElementPlace { .. }
                    )
                }),
        )
        .unwrap();
        let Operation::ContainerElementPlace {
            owner: EntityId::Loan(body_source),
            index: body_cursor,
        } = element_place.operation
        else {
            unreachable!()
        };
        let EntityId::Place(place) = element_place.results[0] else {
            unreachable!()
        };
        let element = provider::loan(
            unique(
                instructions(function, body.target)
                    .unwrap()
                    .into_iter()
                    .filter(|instruction| {
                        instruction.operation
                            == Operation::BorrowBegin {
                                place,
                                kind: LoanKind::Shared,
                            }
                    }),
            )
            .unwrap()
            .results[0],
        )
        .unwrap();
        let break_span = fixture
            .parsed
            .ast()
            .expressions()
            .get(relation.breaking)
            .unwrap()
            .span();
        let breaking = unique(function.instructions.iter().filter(|instruction| {
            instruction.origin == Origin::Source(break_span)
                && matches!(instruction.operation, Operation::BorrowEnd { .. })
        }))
        .unwrap()
        .id;
        Self {
            relation,
            preheader,
            header,
            body: body.clone(),
            exhausted: exhausted.clone(),
            initial,
            items,
            source,
            element,
            body_source,
            body_cursor,
            length,
            cursor,
            step,
            span,
            breaking,
        }
    }

    fn header(&self, function: &Function) -> ProviderHeader {
        let entry = function
            .block(self.preheader)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap();
        let TerminatorKind::Branch(edge) = &entry.kind else {
            unreachable!()
        };
        let source_slot = edge
            .arguments
            .iter()
            .position(|entity| *entity == EntityId::Loan(self.source))
            .unwrap();
        let source =
            provider::loan(function.block(self.header).unwrap().parameters[source_slot]).unwrap();
        ProviderHeader {
            block: self.header,
            source,
            length: self.length,
            cursor: self.cursor,
            source_slot,
            length_slot: parameter(function, EntityId::Value(self.length)).1,
            cursor_slot: parameter(function, EntityId::Value(self.cursor)).1,
            step: self.step,
        }
    }

    /// The production constructor is the only creator of this sealed token.
    /// Its five transport fields come exclusively from the original header/edges;
    /// the scratch-produced loan is replaced with the original element producer.
    fn element(
        &self,
        fixture: &Fixture,
        function: &Function,
        header: &ProviderHeader,
    ) -> GuardedElement {
        let mut scratch = function.clone();
        scratch.blocks[self.header.index()].terminator = None;
        scratch.blocks[self.body.target.index()].terminator = None;
        let descriptor = fixture
            .owned
            .iteration(self.relation.outer)
            .unwrap()
            .descriptor();
        let mut token = guard_and_begin(
            &mut scratch,
            header,
            fixture.type_ids[&fixture.typed.types().builtin(BuiltinType::Boolean).unwrap()],
            fixture.type_ids[&descriptor.element_type()],
            self.body.clone(),
            self.exhausted.clone(),
            &Origin::Source(self.span),
            descriptor.provider(),
        )
        .unwrap();
        assert_ne!(token.loan(), self.element, "scratch loan cannot escape");
        token.set_loan(self.element);
        assert_eq!(
            (
                token.header(),
                token.body(),
                token.source(),
                token.cursor(),
                token.step(),
                token.loan()
            ),
            (
                self.header,
                self.body.target,
                self.body_source,
                self.body_cursor,
                self.step,
                self.element
            )
        );
        assert!(function.entity(EntityId::Loan(token.loan())).is_some());
        token
    }

    fn seed(&self, fixture: &Fixture, lowerer: &mut ExpressionLowerer<'_>) {
        lowerer
            .bindings
            .insert(self.relation.root, LoweredValue::Value(self.initial));
        lowerer
            .bindings
            .insert(self.relation.items, LoweredValue::Value(self.items));
        let mut carried = lowerer
            .linear_binding_slots(&lowerer.bindings, self.span)
            .unwrap();
        let header = self.header(lowerer.function);
        let element = self.element(fixture, lowerer.function, &header);
        // Match the original carried slots exactly, including the scalar total slot.
        let TerminatorKind::Branch(edge) = &lowerer
            .function
            .block(self.preheader)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
        else {
            unreachable!()
        };
        for entity in edge.arguments.iter().take(header.source_slot) {
            if !carried.slots.iter().any(|slot| slot.source == *entity) {
                carried.slots.push(LinearBindingSlot::new(
                    None,
                    *entity,
                    lowerer.function.entity(*entity).unwrap().ty,
                ));
            }
        }
        let plan = fixture
            .owned
            .iteration(self.relation.outer)
            .unwrap()
            .clone();
        let int_type = fixture.type_ids[&fixture.typed.types().builtin(BuiltinType::Int).unwrap()];
        lowerer.loops.push(LoopContext {
            entry_views: BTreeMap::new(),
            outer_borrowed_while: None,
            header: self.header,
            carried,
            continues: Vec::new(),
            breaks: Vec::new(),
            numeric_loop: None,
            for_loop: Some(ForLoopData {
                statement: self.relation.outer,
                source_slot: header.source_slot,
                length_slot: header.length_slot,
                cursor_slot: header.cursor_slot,
                provider_header: header,
                guarded_element: element,
                int_type,
                plan,
                new_source_loan: Some(self.source),
                active_source_loan: Some(self.body_source),
                body_source: self.body_source,
                body_cursor: self.body_cursor,
                is_carried_source: false,
            }),
        });
        let provider = provider::shape(lowerer, &self.relation)
            .expect("all original producer identities must validate before recovery");
        self.state(lowerer, &provider.body);
        lowerer
            .validate_outer_borrowed_state(
                &self.relation,
                &lowerer.branch_exit(LoweredValue::Unit),
                false,
                self.span,
            )
            .unwrap();
    }

    fn state(&self, lowerer: &mut ExpressionLowerer<'_>, columns: &Columns) {
        lowerer.block = columns.block;
        lowerer.bindings.insert(
            self.relation.root,
            LoweredValue::Value(provider::value(columns.entities[0]).unwrap()),
        );
        lowerer.bindings.insert(
            self.relation.items,
            LoweredValue::Value(provider::value(columns.entities[1]).unwrap()),
        );
        let data = lowerer.loops[0].for_loop.as_mut().unwrap();
        data.rebind_source(provider::loan(columns.entities[2]).unwrap());
        if let Some(element) = columns.entities.get(3).copied() {
            let element = provider::loan(element).unwrap();
            data.guarded_element.set_loan(element);
            lowerer.borrow_bindings.insert(self.relation.item, element);
        } else {
            lowerer.borrow_bindings.remove(&self.relation.item);
        }
    }

    fn before_break(&self, lowerer: &mut ExpressionLowerer<'_>) -> (Span, NonNullProof) {
        let provider = provider::shape(lowerer, &self.relation).unwrap();
        let guard = provider::guard(lowerer, &self.relation, &provider).unwrap();
        let end = lowerer.function.instruction(self.breaking).unwrap().clone();
        let span = end.origin.span();
        let block = &lowerer.function.blocks[end.block.index()];
        let columns =
            Columns {
                block: end.block,
                entities: guard
                    .non_null
                    .entities
                    .iter()
                    .map(|entity| {
                        let ty = lowerer.function.entity(*entity).unwrap().ty;
                        unique(block.parameters.iter().copied().filter(|parameter| {
                            lowerer.function.entity(*parameter).unwrap().ty == ty
                        }))
                        .unwrap()
                    })
                    .collect(),
            };
        let Operation::BorrowEnd { loan } = end.operation else {
            unreachable!()
        };
        assert_eq!(columns.entities[4], EntityId::Loan(loan));
        assert_ne!(
            loan, guard.view,
            "break uses the checked-arithmetic rebound view"
        );
        let prefix = block
            .instructions
            .iter()
            .position(|id| *id == self.breaking)
            .unwrap();
        lowerer.function.blocks[end.block.index()]
            .instructions
            .truncate(prefix);
        lowerer.function.blocks[end.block.index()].terminator = None;
        self.state(lowerer, &columns);
        let EntityType::Loan { target: inner, .. } =
            lowerer.function.entity(EntityId::Loan(loan)).unwrap().ty
        else {
            unreachable!()
        };
        let proof = NonNullProof {
            loan,
            owner: provider::value(columns.entities[0]).unwrap(),
            ..NonNullProof::introduced(
                guard.view,
                provider::value(guard.non_null.entities[0]).unwrap(),
                inner,
                ProofScope::While(self.relation.inner),
            )
        };
        lowerer.non_null_bindings.insert(self.relation.root, proof);
        let carried = lowerer
            .linear_binding_slots(&lowerer.bindings, span)
            .unwrap();
        lowerer.loops.push(LoopContext {
            entry_views: BTreeMap::new(),
            outer_borrowed_while: Some(self.relation.clone()),
            header: guard.header,
            carried,
            continues: Vec::new(),
            breaks: Vec::new(),
            for_loop: None,
            numeric_loop: None,
        });
        lowerer
            .validate_outer_borrowed_state(
                &self.relation,
                &lowerer.branch_exit(LoweredValue::Unit),
                true,
                span,
            )
            .unwrap();
        (span, proof)
    }

    fn exhaustion(&self, lowerer: &mut ExpressionLowerer<'_>) {
        let provider = provider::shape(lowerer, &self.relation).unwrap();
        assert_eq!(
            provider::completed_backedge(lowerer, &self.relation, &provider),
            Some(())
        );
        // The consumer sees the real exhaustion block before EndSource and drops.
        lowerer.function.blocks[provider.exhausted.block.index()]
            .instructions
            .clear();
        lowerer.function.blocks[provider.exhausted.block.index()].terminator = None;
        self.state(lowerer, &provider.exhausted);
        lowerer
            .checked_outer_borrowed_exhaustion(&self.relation, self.span)
            .unwrap();
    }
}

fn ambient(lowerer: &ExpressionLowerer<'_>) -> (LoanId, Option<LoanId>, LoanId) {
    let data = lowerer.loops[0].for_loop.as_ref().unwrap();
    (
        data.body_source,
        data.active_source_loan,
        data.guarded_element.loan(),
    )
}

#[test]
fn single_outer_borrowed_runtime_guards_preserve_snapshot_break_and_exhaustion() {
    for source in while_inside_borrowed_for_sources() {
        let fixture = Fixture::analyze(&source);
        let shape = Shape::read(&fixture);
        let relation = &shape.relation;
        let span = shape.span;
        let mut function = fixture.entry().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            shape.seed(&fixture, lowerer);
            let state = lowerer.branch_exit(LoweredValue::Unit);
            lowerer
                .validate_outer_borrowed_exit_states(
                    relation,
                    &[lowerer.branch_exit(LoweredValue::Unit)],
                    span,
                )
                .unwrap();
            lowerer
                .restore_outer_borrowed_state(relation, &state, span)
                .unwrap();
        });
        for mutation in 0..8 {
            let mut function = fixture.entry().clone();
            fixture.with_lowerer(&mut function, |lowerer| {
                shape.seed(&fixture, lowerer);
                let mut state = lowerer.branch_exit(LoweredValue::Unit);
                match mutation {
                    0 => {
                        state.for_sources.clear();
                    }
                    1 => {
                        state.for_elements.clear();
                    }
                    2 => {
                        state
                            .for_sources
                            .insert(relation.inner.index(), shape.body_source);
                    }
                    3 => {
                        state
                            .for_elements
                            .insert(relation.inner.index(), shape.element);
                    }
                    4 => {
                        state
                            .for_sources
                            .insert(relation.outer.index(), shape.source);
                    }
                    5 => {
                        state.for_elements.insert(
                            relation.outer.index(),
                            provider::loan(
                                fixture
                                    .entry()
                                    .block(
                                        fixture.entry().instruction(shape.breaking).unwrap().block,
                                    )
                                    .unwrap()
                                    .parameters
                                    .iter()
                                    .copied()
                                    .find(|entity| {
                                        *entity != EntityId::Loan(shape.element)
                                            && fixture.entry().entity(*entity).unwrap().ty
                                                == fixture
                                                    .entry()
                                                    .entity(EntityId::Loan(shape.element))
                                                    .unwrap()
                                                    .ty
                                    })
                                    .unwrap(),
                            )
                            .unwrap(),
                        );
                    }
                    6 => {
                        state
                            .bindings
                            .insert(relation.root, LoweredValue::Value(shape.initial));
                    }
                    7 => {
                        let block = state.block;
                        lowerer.function.blocks[block.index()].terminator = None;
                        lowerer
                            .function
                            .append_instruction(
                                block,
                                Operation::BorrowEnd {
                                    loan: shape.body_source,
                                },
                                Vec::new(),
                                Origin::Source(span),
                            )
                            .unwrap();
                    }
                    _ => unreachable!(),
                }
                let before = ambient(lowerer);
                unsupported(
                    lowerer.restore_outer_borrowed_state(relation, &state, span),
                    span,
                );
                assert_eq!(
                    ambient(lowerer),
                    before,
                    "rejection cannot partially revive source/element"
                );
            });
        }
        let mut function = fixture.entry().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            shape.seed(&fixture, lowerer);
            let (break_span, proof) = shape.before_break(lowerer);
            let before = ambient(lowerer);
            let count = lowerer.function.instructions.len();
            assert_eq!(
                lowerer.retire_outer_borrowed_break(break_span).unwrap(),
                true
            );
            assert_eq!(ambient(lowerer), before);
            assert!(!lowerer.non_null_bindings.contains_key(&relation.root));
            assert_eq!(lowerer.function.instructions.len(), count + 1);
            assert_eq!(
                lowerer.function.instructions.last().unwrap().operation,
                Operation::BorrowEnd { loan: proof.loan }
            );
        });
        for mutation in 0..4 {
            let mut function = fixture.entry().clone();
            fixture.with_lowerer(&mut function, |lowerer| {
                shape.seed(&fixture, lowerer);
                let (break_span, proof) = shape.before_break(lowerer);
                let guard = provider::guard(
                    lowerer,
                    relation,
                    &provider::shape(lowerer, relation).unwrap(),
                )
                .unwrap();
                match mutation {
                    0 => {
                        lowerer
                            .non_null_bindings
                            .get_mut(&relation.root)
                            .unwrap()
                            .origin = NonNullProof::introduced(
                            guard.view,
                            proof.owner,
                            proof.inner,
                            ProofScope::While(relation.outer),
                        )
                        .origin
                    }
                    1 => {
                        lowerer
                            .non_null_bindings
                            .get_mut(&relation.root)
                            .unwrap()
                            .origin = NonNullProof::introduced(
                            proof.loan,
                            proof.owner,
                            proof.inner,
                            ProofScope::While(relation.inner),
                        )
                        .origin
                    }
                    2 => {
                        let birth_owner = provider::value(guard.non_null.entities[0]).unwrap();
                        assert_ne!(proof.owner, birth_owner);
                        lowerer
                            .bindings
                            .insert(relation.root, LoweredValue::Value(birth_owner));
                        lowerer
                            .non_null_bindings
                            .get_mut(&relation.root)
                            .unwrap()
                            .owner = birth_owner;
                    }
                    3 => {}
                    _ => unreachable!(),
                }
                let before = ambient(lowerer);
                let count = lowerer.function.instructions.len();
                let attempted = if mutation == 3 { span } else { break_span };
                unsupported(lowerer.retire_outer_borrowed_break(attempted), attempted);
                assert_eq!(ambient(lowerer), before);
                assert_eq!(lowerer.function.instructions.len(), count);
            });
        }
        let mut function = fixture.entry().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            shape.seed(&fixture, lowerer);
            shape.exhaustion(lowerer);
            let exit = fixture
                .owned
                .iteration(relation.outer)
                .unwrap()
                .exits()
                .iter()
                .find(|exit| exit.kind() == IterationExitKind::Exhaustion)
                .unwrap();
            lowerer
                .preflight_outer_borrowed_iteration(exit, span)
                .unwrap();
            let source = lowerer.loops[0]
                .for_loop
                .as_ref()
                .unwrap()
                .active_source_loan
                .unwrap();
            lowerer
                .append(Operation::BorrowEnd { loan: source }, Vec::new(), span)
                .unwrap();
            lowerer.loops[0]
                .for_loop
                .as_mut()
                .unwrap()
                .active_source_loan = None;
            for fact in relation.drops {
                let phis = fixture
                    .owned
                    .iteration(relation.outer)
                    .unwrap()
                    .closure_phis();
                let header = phis
                    .iter()
                    .find(|phi| {
                        phi.boundary() == IterationPhiBoundary::Header
                            && Some(phi.owner()) == fact.owner()
                    })
                    .unwrap();
                let exit = phis
                    .iter()
                    .find(|phi| {
                        phi.boundary() == IterationPhiBoundary::Exit
                            && phi.symbol() == header.symbol()
                    })
                    .unwrap();
                assert_ne!(header.owner(), exit.owner());
                assert_eq!(fact.condition(), Some(header.availability_condition()));
                assert_ne!(fact.condition(), Some(exit.availability_condition()));
                assert_eq!(lowerer.emit_outer_borrowed_drop(fact).unwrap(), true);
            }
            assert!(!lowerer.bindings.contains_key(&relation.root));
            assert!(!lowerer.bindings.contains_key(&relation.items));
        });
        for mutation in 0..5 {
            let mut function = fixture.entry().clone();
            fixture.with_lowerer(&mut function, |lowerer| {
                shape.seed(&fixture, lowerer);
                shape.exhaustion(lowerer);
                match mutation {
                    0 => {
                        lowerer
                            .bindings
                            .insert(relation.root, LoweredValue::Value(shape.initial));
                    }
                    1 => {
                        lowerer.loops[0]
                            .for_loop
                            .as_mut()
                            .unwrap()
                            .active_source_loan = Some(shape.source);
                    }
                    2 => {
                        lowerer.borrow_bindings.insert(relation.item, shape.element);
                    }
                    3 | 4 => {
                        let back = incoming(lowerer.function, shape.header)
                            .into_iter()
                            .find(|(block, _)| *block != shape.preheader)
                            .unwrap()
                            .0;
                        if mutation == 3 {
                            let end = lowerer.function.blocks[back.index()]
                                .instructions
                                .iter()
                                .copied()
                                .find(|id| {
                                    matches!(
                                        lowerer.function.instruction(*id).unwrap().operation,
                                        Operation::BorrowEnd { .. }
                                    )
                                })
                                .unwrap();
                            lowerer.function.instructions[end.index()].operation =
                                Operation::BorrowEnd {
                                    loan: shape.body_source,
                                };
                        } else {
                            let next = lowerer.function.blocks[back.index()]
                                .instructions
                                .iter()
                                .copied()
                                .find(|id| {
                                    matches!(
                                        lowerer.function.instruction(*id).unwrap().operation,
                                        Operation::Binary {
                                            operator: BinaryOperator::Add,
                                            ..
                                        }
                                    )
                                })
                                .unwrap();
                            let Operation::Binary { left, .. } =
                                &mut lowerer.function.instructions[next.index()].operation
                            else {
                                unreachable!()
                            };
                            *left = shape.cursor;
                        }
                        let provider = provider::shape(lowerer, relation).unwrap();
                        assert_eq!(
                            provider::completed_backedge(lowerer, relation, &provider),
                            None
                        );
                    }
                    _ => unreachable!(),
                }
                let exit = fixture
                    .owned
                    .iteration(relation.outer)
                    .unwrap()
                    .exits()
                    .iter()
                    .find(|exit| exit.kind() == IterationExitKind::Exhaustion)
                    .unwrap();
                unsupported(lowerer.preflight_outer_borrowed_iteration(exit, span), span);
            });
        }
    }
}
