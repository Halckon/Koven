use super::*;
use crate::ssa::model::{Function, InstructionId, LoanId};
use crate::ssa::unit_lower::{
    loop_control::iteration::IterationContext,
    nullable_proof::{
        NullableProofScope,
        guard_tests::support::{Fixture, assert_transport, parameter, rebound},
    },
};

pub(super) struct Shape {
    pub(super) outer: UnitStatementId,
    pub(super) scope: UnitStatementId,
    pub(super) condition: ExpressionId,
    pub(super) span: Span,
    pub(super) inner_span: Span,
    pub(super) roots: [UnitSymbolId; 3],
    pub(super) item: UnitSymbolId,
    pub(super) header: BlockId,
    pub(super) anchor: BlockId,
    pub(super) six: [EntityId; 6],
    pub(super) initial_pending: Vec<EntityId>,
    pub(super) birth: InstructionId,
    pub(super) inner_header: BlockId,
    pub(super) body: BlockId,
    pub(super) view: LoanId,
    pub(super) owner_column: usize,
    pub(super) step: ValueId,
    pub(super) int_type: SsaTypeId,
    pub(super) back_block: BlockId,
    pub(super) back_edge: Edge,
    pub(super) element_end: InstructionId,
    pub(super) cursor_before: ValueId,
}

impl Shape {
    pub(super) fn read(fixture: &Fixture, lowerer: &UnitExpressionLowerer<'_>) -> Self {
        let [descriptor] = lowerer.typed.sequential_iterations() else {
            panic!("one actual provider")
        };
        let outer = descriptor.statement();
        let span = lowerer
            .parsed
            .ast()
            .statements()
            .get(outer.statement())
            .unwrap()
            .span();
        let (scope, condition, inner_span) = lowerer
            .parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(id, node)| match node.payload() {
                Statement::While { condition, .. } => Some((
                    UnitStatementId::new(lowerer.source_unit, id),
                    *condition,
                    node.span(),
                )),
                _ => None,
            })
            .unwrap();
        let current = fixture.symbol("current");
        let items = fixture.symbol("items");
        let total = fixture.symbol("total");
        let item = fixture.symbol("item");
        let function = &*lowerer.function;
        let (header, body_edge, comparison) = function
            .blocks
            .iter()
            .find_map(|block| {
                let term = block.terminator.as_ref()?;
                let TerminatorKind::Conditional {
                    condition,
                    when_true,
                    ..
                } = &term.kind
                else {
                    return None;
                };
                (term.origin == Origin::Source(span)).then_some((
                    block.id,
                    when_true.clone(),
                    *condition,
                ))
            })
            .unwrap();
        let anchor = body_edge.target;
        let Operation::Compare {
            operator: ComparisonOperator::LessThan,
            left: header_cursor,
            right: header_length,
        } = definition(lowerer, EntityId::Value(comparison))
            .unwrap()
            .operation
        else {
            panic!("real provider guard")
        };
        let place = function
            .instructions
            .iter()
            .find(|instruction| {
                instruction.block == anchor
                    && matches!(
                        instruction.operation,
                        Operation::ContainerElementPlace { .. }
                    )
            })
            .unwrap();
        let Operation::ContainerElementPlace {
            owner: source,
            index: cursor,
        } = place.operation
        else {
            unreachable!()
        };
        let [EntityId::Place(place_id)] = place.results.as_slice() else {
            panic!()
        };
        let begin = function
            .instructions
            .iter()
            .find(|instruction| {
                instruction.block == anchor
                    && instruction.operation
                        == Operation::BorrowBegin {
                            place: *place_id,
                            kind: LoanKind::Shared,
                        }
            })
            .unwrap();
        let [element] = begin.results.as_slice() else {
            panic!()
        };
        let element = *element;
        let length = rebound(function, EntityId::Value(header_length), anchor);
        assert_eq!(
            rebound(function, EntityId::Value(header_cursor), anchor),
            EntityId::Value(cursor)
        );
        let entry = function.entry_block().unwrap();
        let TerminatorKind::Branch(pre_edge) = &function
            .block(entry)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
        else {
            panic!()
        };
        assert_eq!(pre_edge.target, header);
        let initial_pending = [source, length, EntityId::Value(cursor)]
            .map(|entity| {
                incoming(
                    lowerer,
                    incoming(lowerer, entity, &body_edge, span).unwrap(),
                    pre_edge,
                    span,
                )
                .unwrap()
            })
            .to_vec();
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = definition(lowerer, initial_pending[0]).unwrap().operation
        else {
            panic!()
        };
        let Operation::RootPlace {
            owner: original_items,
        } = definition(lowerer, EntityId::Place(place))
            .unwrap()
            .operation
        else {
            panic!()
        };
        let LoweredValue::Value(original_current) = lowerer.bindings[&fixture.symbol("initial")]
        else {
            panic!()
        };
        let six = [
            rebound(function, EntityId::Value(original_items), anchor),
            rebound(function, EntityId::Value(original_current), anchor),
            source,
            length,
            EntityId::Value(cursor),
            element,
        ];
        let int_type = function.entity(length).unwrap().ty.semantic_type();
        let step = function
            .instructions
            .iter()
            .find_map(|instruction| {
                (instruction.block == entry
                    && instruction.origin == Origin::Source(span)
                    && instruction.operation == Operation::Constant(ScalarConstant::Integer(1)))
                .then(|| match instruction.results[0] {
                    EntityId::Value(value) => value,
                    _ => panic!(),
                })
            })
            .unwrap();
        let (inner_header, owner_column, body, view) = function
            .blocks
            .iter()
            .find_map(|block| {
                let term = block.terminator.as_ref()?;
                let TerminatorKind::NullableBranch {
                    owner,
                    when_non_null,
                    view,
                    ..
                } = &term.kind
                else {
                    return None;
                };
                (term.origin == Origin::Source(inner_span)).then(|| {
                    (
                        block.id,
                        when_non_null
                            .arguments
                            .iter()
                            .position(|entity| *entity == EntityId::Value(*owner))
                            .unwrap(),
                        when_non_null.target,
                        *view,
                    )
                })
            })
            .unwrap();
        let (back_block, back_edge) = function
            .blocks
            .iter()
            .find_map(|block| {
                let TerminatorKind::Branch(edge) = &block.terminator.as_ref()?.kind else {
                    return None;
                };
                (block.id != entry && edge.target == header).then_some((block.id, edge.clone()))
            })
            .unwrap();
        let (_, cursor_column) = parameter(function, EntityId::Value(header_cursor));
        let cursor_next = back_edge.arguments[cursor_column];
        let next = definition(lowerer, cursor_next).unwrap();
        assert_eq!(next.block, back_block);
        let Operation::Binary {
            operator: BinaryOperator::Add,
            left: cursor_before,
            right,
        } = next.operation
        else {
            panic!()
        };
        assert_eq!(right, step);
        let current_element = rebound(function, element, back_block);
        let element_end = function.block(back_block).unwrap().instructions.iter().find(|id| {
            matches!(function.instruction(**id).unwrap().operation, Operation::BorrowEnd { loan }
                if EntityId::Loan(loan) == current_element)
        }).copied().unwrap();
        let shape = Self {
            outer,
            scope,
            condition,
            span,
            inner_span,
            roots: [current, total, items],
            item,
            header,
            anchor,
            six,
            initial_pending,
            birth: begin.id,
            inner_header,
            body,
            view,
            owner_column,
            step,
            int_type,
            back_block,
            back_edge,
            element_end,
            cursor_before,
        };
        // Independent whole-graph baseline: all six roles travel together along
        // every predecessor to the one actual element producer.
        let end_six = six.map(|entity| rebound(function, entity, back_block));
        assert_transport(function, back_block, end_six, anchor, six);
        shape
    }

    fn named_at(&self, lowerer: &mut UnitExpressionLowerer<'_>, block: BlockId) {
        let mut roots = self.roots;
        roots.sort();
        lowerer.bindings = roots
            .into_iter()
            .zip(&lowerer.function.block(block).unwrap().parameters)
            .map(|(root, entity)| {
                let EntityId::Value(value) = entity else {
                    panic!("actual named value prefix")
                };
                assert_eq!(
                    lowerer.function.entity(*entity).unwrap().ty,
                    EntityType::Value(lowerer.type_ids[&lowerer.typed.symbol_type(root).unwrap()])
                );
                (root, LoweredValue::Value(*value))
            })
            .collect();
    }

    fn restore_roles(
        &self,
        lowerer: &mut UnitExpressionLowerer<'_>,
        original: &Function,
        block: BlockId,
    ) {
        self.named_at(lowerer, block);
        lowerer.block = block;
        lowerer.pending_operands = self.six[2..]
            .iter()
            .map(|entity| {
                if block == self.anchor && *entity == self.six[5] {
                    *entity
                } else {
                    rebound(original, *entity, block)
                }
            })
            .collect();
        let flag = *lowerer.entry_boolean_roots.first().unwrap();
        let original_flag = original
            .block(original.entry_block().unwrap())
            .unwrap()
            .parameters[1];
        let EntityId::Loan(flag_loan) = rebound(original, original_flag, block) else {
            panic!()
        };
        let EntityId::Loan(element) = lowerer.pending_operands[3] else {
            panic!()
        };
        lowerer.borrow_bindings = BTreeMap::from([(flag, flag_loan), (self.item, element)]);
        lowerer.nullable_proofs.clear();
    }

    /// At element birth only preheader/header and the two element instructions
    /// have executed. Future instructions are removed, their ID arenas retained.
    pub(super) fn qualify(
        &self,
        lowerer: &mut UnitExpressionLowerer<'_>,
        original: &Function,
    ) -> (DirectNullableWhileEntry, BorrowedForWhile) {
        let entry = original.entry_block().unwrap();
        for block in &mut lowerer.function.blocks {
            if block.id == entry || block.id == self.header {
                continue;
            }
            if block.id == self.anchor {
                let end = block
                    .instructions
                    .iter()
                    .position(|id| *id == self.birth)
                    .unwrap();
                block.instructions.truncate(end + 1);
            } else {
                block.instructions.clear();
            }
            block.terminator = None;
        }
        self.restore_roles(lowerer, original, self.anchor);
        let mut context = lowerer.guard_source_loop_context(
            self.header,
            &self.roots,
            self.initial_pending.clone(),
            None,
            self.span,
        );
        context.iteration = Some(IterationContext {
            statement: self.outer,
            retained_start: 0,
            outer_temporaries: BTreeSet::new(),
            state: IterationState::Borrowed {
                start: 0,
                element: 3,
                step: self.step,
                int_type: self.int_type,
            },
            inherited_borrowed: None,
        });
        lowerer.loops.push(context);
        let (descriptor, operand) = lowerer
            .direct_nullable_while_condition(self.condition, self.inner_span)
            .unwrap()
            .unwrap();
        let NullableWhileEntry::Direct(entry) = lowerer
            .prepare_nullable_while_entry(
                self.scope.statement(),
                descriptor,
                operand,
                self.inner_span,
            )
            .unwrap()
        else {
            panic!("direct qualified source")
        };
        let token = entry.borrowed_for.as_ref().unwrap().as_ref().clone();
        assert_eq!(
            (token.outer, token.outer_header, token.scope),
            (self.outer, self.header, self.scope)
        );
        assert_eq!(
            (token.items, token.current, token.item),
            (self.roots[2], self.roots[0], self.item)
        );
        assert_eq!(
            (token.step, token.int_type, token.anchor_block, token.anchor),
            (self.step, self.int_type, self.anchor, self.six)
        );
        assert!(token.header.is_none() && token.proof.is_none());
        assert_eq!(lowerer.function.values.len(), original.values.len());
        assert_eq!(lowerer.function.loans.len(), original.loans.len());
        token
            .validate_exit(lowerer, &lowerer.loop_state(), self.inner_span)
            .unwrap();
        token
            .validate_birth(lowerer, self.step, self.inner_span)
            .unwrap();
        (entry, token)
    }

    pub(super) fn active(
        &self,
        lowerer: &mut UnitExpressionLowerer<'_>,
        original: &Function,
        token: &mut BorrowedForWhile,
    ) {
        let anchor_state = lowerer.loop_state();
        *lowerer.function = original.clone();
        let entry = original.entry_block().unwrap();
        for block in &mut lowerer.function.blocks {
            if [entry, self.header, self.anchor, self.inner_header].contains(&block.id) {
                continue;
            }
            block.instructions.clear();
            block.terminator = None;
        }
        self.restore_roles(lowerer, original, self.body);
        lowerer
            .introduce_nullable_proof(
                self.inner_header,
                self.owner_column,
                token.current,
                token.inner,
                NullableProofScope::While(self.scope),
                self.inner_span,
            )
            .unwrap();
        token.bind_header(self.inner_header);
        token.bind_proof(lowerer, self.inner_span).unwrap();
        assert_eq!(token.proof.unwrap().loan, self.view);
        let body_state = lowerer.loop_state();
        lowerer.restore_loop_state(&anchor_state);
        let mut carried = lowerer
            .carried_bindings(&lowerer.bindings, self.inner_span)
            .unwrap();
        let mut loans = lowerer
            .carried_loans(&lowerer.borrow_bindings, self.inner_span)
            .unwrap();
        lowerer
            .carry_pending_operands(&mut carried, &mut loans, self.inner_span)
            .unwrap();
        let edge = carried_control_edge(self.inner_header, &carried, &loans);
        assert_eq!(
            lowerer
                .function
                .block(self.anchor)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .kind,
            TerminatorKind::Branch(edge)
        );
        let context = LoopContext {
            proof_scope: Some(self.scope),
            borrowed_for_while: Some(Box::new(token.clone())),
            borrowed_while_body: None,
            iteration: None,
            header: self.inner_header,
            carried,
            loans,
            entry_temporaries: lowerer.temporaries.keys().copied().collect(),
            entry_pending_count: 4,
            entry_receiver: None,
            entry_consumed_receiver: None,
            entry_symbols: lowerer.bindings.keys().copied().collect(),
            entry_closure_bindings: lowerer.closure_bindings.clone(),
            continues: Vec::new(),
            breaks: Vec::new(),
        };
        lowerer.restore_loop_state(&body_state);
        lowerer.loops.push(context);
        token
            .validate(lowerer, &lowerer.loop_state(), true, false, self.inner_span)
            .unwrap();
    }

    pub(super) fn at_end(
        &self,
        lowerer: &mut UnitExpressionLowerer<'_>,
        original: &Function,
        token: &mut BorrowedForWhile,
        advanced: bool,
    ) {
        *lowerer.function = original.clone();
        let block = &mut lowerer.function.blocks[self.back_block.index()];
        block.terminator = None;
        if !advanced {
            let end = block
                .instructions
                .iter()
                .position(|id| *id == self.element_end)
                .unwrap();
            block.instructions.truncate(end + 1);
        }
        lowerer.block = self.back_block;
        let mut roots = self.roots;
        roots.sort();
        lowerer.bindings = roots
            .into_iter()
            .zip(&self.back_edge.arguments)
            .map(|(root, entity)| {
                let EntityId::Value(value) = entity else {
                    panic!()
                };
                (root, LoweredValue::Value(*value))
            })
            .collect();
        let source = rebound(original, self.six[2], self.back_block);
        let length = rebound(original, self.six[3], self.back_block);
        let cursor = if advanced {
            let (_, header_column) = parameter(
                original,
                rebound(original, self.initial_pending[2], self.header),
            );
            self.back_edge.arguments[header_column]
        } else {
            EntityId::Value(self.cursor_before)
        };
        lowerer.pending_operands = vec![source, length, cursor];
        if !advanced {
            lowerer
                .pending_operands
                .push(rebound(original, self.six[5], self.back_block));
        }
        let flag = *lowerer.entry_boolean_roots.first().unwrap();
        let original_flag = original
            .block(original.entry_block().unwrap())
            .unwrap()
            .parameters[1];
        let (_, flag_column) = parameter(original, rebound(original, original_flag, self.header));
        let EntityId::Loan(flag_loan) = self.back_edge.arguments[flag_column] else {
            panic!()
        };
        lowerer.borrow_bindings = BTreeMap::from([(flag, flag_loan)]);
        lowerer.nullable_proofs.clear();
        token.bind_header(self.inner_header);
        token.install_outer(lowerer, self.inner_span).unwrap();
        if advanced {
            let state = lowerer.loop_state();
            lowerer.loops[0].continues.push(LoopJump {
                state,
                span: self.span,
            });
        } else {
            token.validate_element_end(lowerer, self.span).unwrap();
        }
    }
}
