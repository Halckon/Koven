//! Exact call sites: before MapSize/NullableTake, before CallReturn cleanup, or If retirement.
use super::prelude::{one_instruction, result};
use super::*;

#[derive(Clone, Copy)]
pub(in crate::ssa::lower_frontend) struct Point {
    pub(in crate::ssa::lower_frontend) expression: ExpressionId,
    pub(in crate::ssa::lower_frontend) span: Span,
    pub(in crate::ssa::lower_frontend) root: SymbolId,
    pub(in crate::ssa::lower_frontend) storage: TypeId,
    pub(in crate::ssa::lower_frontend) narrowed: TypeId,
    pub(in crate::ssa::lower_frontend) nullable: SsaTypeId,
    pub(in crate::ssa::lower_frontend) control_span: Span,
    pub(in crate::ssa::lower_frontend) header: BlockId,
    pub(in crate::ssa::lower_frontend) header_owner: ValueId,
    pub(in crate::ssa::lower_frontend) null_block: BlockId,
    pub(in crate::ssa::lower_frontend) body: BlockId,
    pub(in crate::ssa::lower_frontend) proof: NonNullProof,
    pub(in crate::ssa::lower_frontend) instruction: InstructionId,
}

#[derive(Clone)]
pub(in crate::ssa::lower_frontend) struct Checkpoint {
    pub(in crate::ssa::lower_frontend) function: Function,
    pub(in crate::ssa::lower_frontend) point: Point,
    pub(in crate::ssa::lower_frontend) bindings: BTreeMap<SymbolId, LoweredValue>,
    pub(in crate::ssa::lower_frontend) type_ids: BTreeMap<TypeId, SsaTypeId>,
    pub(in crate::ssa::lower_frontend) nullable_maps: BTreeMap<SsaTypeId, SsaTypeId>,
    pub(in crate::ssa::lower_frontend) function_ids: BTreeMap<FunctionInstanceKey, FunctionId>,
    pub(in crate::ssa::lower_frontend) source: Option<SourceToken>,
}

impl Fixture {
    pub(in crate::ssa::lower_frontend) fn read_checkpoint(&self, cleanup: bool) -> Checkpoint {
        let [descriptor] = self.typed.map_sizes() else {
            panic!("one frozen B read")
        };
        self.checkpoint(
            descriptor.expression(),
            descriptor.receiver(),
            false,
            cleanup,
        )
    }

    pub(in crate::ssa::lower_frontend) fn consume_checkpoint(&self) -> Checkpoint {
        let [descriptor] = self.typed.map_consumes() else {
            panic!("one frozen C consume")
        };
        self.checkpoint(descriptor.expression(), descriptor.receiver(), true, false)
    }

    fn checkpoint(
        &self,
        expression: ExpressionId,
        receiver: ExpressionId,
        consume: bool,
        cleanup: bool,
    ) -> Checkpoint {
        let span = self.span(expression);
        let use_fact = self.typed.non_null_use(receiver).unwrap();
        let root = use_fact.symbol();
        assert_eq!(root, self.reference(receiver));
        assert!(self.owned.formed_owned_locals().contains(&root));
        let storage = use_fact.declared_type();
        let narrowed = use_fact.narrowed_type();
        assert_eq!(self.typed.symbol_type(root), Some(storage));
        let nullable = self.type_ids[&storage];
        let inner = self.type_ids[&narrowed];
        let controls = self
            .parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                let Expression::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                let comparison = self.typed.null_comparison(self.ungroup(*condition))?;
                if comparison.symbol() != root {
                    return None;
                }
                let body = if comparison.non_null_when_true() {
                    Some(*then_branch)
                } else {
                    *else_branch
                }?;
                let body_span = self.parsed.ast().statements().get(body).unwrap().span();
                contains(body_span, span).then_some((id, node.span(), body, comparison))
            })
            .collect::<Vec<_>>();
        let [(control, control_span, statement, comparison)] = controls.as_slice() else {
            panic!("one exact source If introducing this read/consume")
        };
        assert_eq!(comparison.nullable_type(), storage);
        let functions = self.product.program.modules[0]
            .functions
            .iter()
            .filter(|function| contains(function.origin.span(), span))
            .collect::<Vec<_>>();
        let [baseline] = functions.as_slice() else {
            panic!("one actual source callable")
        };
        let baseline = *baseline;
        let births = baseline
            .blocks
            .iter()
            .filter(|block| {
                block.terminator.as_ref().is_some_and(|term| {
                    term.origin == Origin::Source(*control_span)
                        && matches!(term.kind, TerminatorKind::NullableBranch { .. })
                })
            })
            .collect::<Vec<_>>();
        let [header] = births.as_slice() else {
            panic!("one exact If branch birth")
        };
        assert_eq!(
            header.id,
            baseline.entry_block().unwrap(),
            "no preceding CFG in bounded callable"
        );
        let TerminatorKind::NullableBranch {
            owner,
            view,
            when_null,
            when_non_null,
        } = &header.terminator.as_ref().unwrap().kind
        else {
            unreachable!()
        };
        let before_if = self.prelude_bindings(baseline, *control);
        assert_eq!(
            before_if,
            BTreeMap::from([(root, LoweredValue::Value(*owner))])
        );
        assert_eq!(when_null.arguments, vec![EntityId::Value(*owner)]);
        assert_eq!(when_non_null.arguments, when_null.arguments);
        let body = baseline.block(when_non_null.target).unwrap();
        let [EntityId::Value(body_owner), EntityId::Loan(body_view)] = body.parameters.as_slice()
        else {
            panic!("exact owner column and branch-born view")
        };
        assert_eq!(*body_view, *view);
        assert_eq!(
            baseline
                .entity(EntityId::Value(*body_owner))
                .unwrap()
                .definition,
            Definition::BlockParameter {
                block: body.id,
                index: 0
            }
        );
        assert_eq!(
            baseline.entity(EntityId::Loan(*view)).unwrap().definition,
            Definition::BlockParameter {
                block: body.id,
                index: 1
            }
        );
        assert_eq!(
            baseline.entity(EntityId::Loan(*view)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: inner
            }
        );
        let null = baseline.block(when_null.target).unwrap();
        assert_eq!(null.parameters.len(), 1);
        assert_eq!(
            baseline.entity(null.parameters[0]).unwrap().ty,
            EntityType::Value(nullable)
        );
        assert_eq!(
            self.product.program.modules[0].nullable_maps.get(&nullable),
            Some(&inner)
        );
        let instruction = if consume {
            one_instruction(baseline, self.span(receiver), |operation| {
                matches!(operation, Operation::NullableTake { owner, proof }
                    if *owner == *body_owner && *proof == *view)
            })
        } else {
            one_instruction(
                baseline,
                span,
                |operation| matches!(operation, Operation::MapSize { owner } if *owner == EntityId::Loan(*view)),
            )
        };
        assert_eq!(instruction.block, body.id);
        let target_index = body
            .instructions
            .iter()
            .position(|&id| id == instruction.id)
            .unwrap();
        assert_eq!(
            target_index, 0,
            "first body use has no pending evaluation prefix"
        );
        let mut bindings = BTreeMap::from([(root, LoweredValue::Value(*body_owner))]);
        let stop = if cleanup {
            let end_id = body.instructions[target_index + 1];
            let end = baseline.instruction(end_id).unwrap();
            assert_eq!(end.operation, Operation::BorrowEnd { loan: *view });
            if self.id == "B3" {
                assert_eq!(end.origin, Origin::Source(*control_span));
                // Single lowering keeps the actual Copyable local until merge_exits.
                let Statement::ControlBody { elements } = self
                    .parsed
                    .ast()
                    .statements()
                    .get(*statement)
                    .unwrap()
                    .payload()
                else {
                    panic!("B body")
                };
                let [local] = elements.as_slice() else {
                    panic!("one real count declaration")
                };
                let Statement::LocalVariable { declaration } = self
                    .parsed
                    .ast()
                    .statements()
                    .get(*local)
                    .unwrap()
                    .payload()
                else {
                    panic!("real count local")
                };
                let (count, initializer) = self.declaration(*declaration);
                assert_eq!(initializer, expression);
                assert_eq!(
                    self.typed
                        .copyability(self.typed.symbol_type(count).unwrap()),
                    Some(Copyability::Copyable)
                );
                bindings.insert(count, LoweredValue::Value(result(instruction)));
            } else {
                let facts = self
                    .owned
                    .drops()
                    .iter()
                    .filter(|fact| {
                        fact.point() == DropPoint::CallReturn(expression)
                            && fact.target() == DropTarget::Named(root)
                    })
                    .collect::<Vec<_>>();
                let [fact] = facts.as_slice() else {
                    panic!("one real read CallReturn NamedDrop")
                };
                assert_eq!(end.origin, Origin::Source(fact.value_origin()));
                let drop = baseline
                    .instruction(body.instructions[target_index + 2])
                    .unwrap();
                assert_eq!(drop.operation, Operation::Drop { owner: *body_owner });
                assert_eq!(drop.origin, Origin::Source(fact.value_origin()));
            }
            target_index + 1
        } else {
            target_index
        };
        let point = Point {
            expression,
            span,
            root,
            storage,
            narrowed,
            nullable,
            control_span: *control_span,
            header: header.id,
            header_owner: *owner,
            null_block: null.id,
            body: body.id,
            proof: NonNullProof::introduced(
                *view,
                *body_owner,
                inner,
                non_null_proof::ProofScope::If(*control),
            ),
            instruction: instruction.id,
        };
        let mut function = baseline.clone();
        // Follow lower_nullable_if's actual then/else order. Arena ordinals are not a clock.
        // The header is already emitted; a false-polarity null arm has already lowered.
        for block in &mut function.blocks {
            if block.id == header.id {
                continue;
            }
            if block.id == body.id {
                block.instructions.truncate(stop);
            } else if block.id != null.id || comparison.non_null_when_true() {
                block.instructions.clear();
            }
            // All merge/return terminators were installed after the selected checkpoint.
            block.terminator = None;
        }
        assert_prefix_state(&function, point);
        Checkpoint {
            function,
            point,
            bindings,
            type_ids: self.type_ids.clone(),
            nullable_maps: self.product.program.modules[0].nullable_maps.clone(),
            function_ids: self.product.function_ids.clone(),
            source: Some(self.plans[&baseline.id].source),
        }
    }
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn assert_prefix_state(function: &Function, point: Point) {
    // prelude_bindings independently closed every header owner/loan in emission
    // order. These are the only later instructions already emitted at this point.
    for block in &function.blocks {
        if block.id == point.header {
            continue;
        }
        assert!(block.terminator.is_none());
        if block.id == point.body {
            assert!(block.instructions.len() <= 1);
            for &id in &block.instructions {
                assert_eq!(
                    function.instruction(id).unwrap().operation,
                    Operation::MapSize {
                        owner: EntityId::Loan(point.proof.loan)
                    }
                );
            }
        } else if block.id == point.null_block {
            // A false-polarity If already lowered this sibling's sole source drop.
            // It cannot create any temporary or consume the non-null arm's view.
            let [EntityId::Value(owner)] = block.parameters.as_slice() else {
                panic!("one actual null-arm owner")
            };
            assert!(block.instructions.len() <= 1);
            for &id in &block.instructions {
                assert_eq!(
                    function.instruction(id).unwrap().operation,
                    Operation::Drop { owner: *owner }
                );
            }
        } else {
            assert!(
                block.instructions.is_empty(),
                "all future block instruction references removed"
            );
        }
    }
}
