//! Prefixes are selected by source facts and actual branch roles, never type/ID order.
use super::*;
use crate::ssa::model::{Definition, InstructionId, LoanId};
use lang_frontend::ownership_checking::{UnitDropFact, UnitDropTarget};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa::unit_lower) enum Stage {
    Producer,
    BeforeUse,
    Cleanup,
    Exits,
}

pub(in crate::ssa::unit_lower) struct Checkpoint {
    pub root: UnitSymbolId,
    pub alias: UnitSymbolId,
    pub storage: UnitTypeId,
    pub narrowed: UnitTypeId,
    pub nullable: SsaTypeId,
    pub inner: SsaTypeId,
    pub control: UnitExpressionId,
    pub condition: ExpressionId,
    pub control_span: Span,
    pub use_id: UnitExpressionId,
    pub use_span: Span,
    pub header: BlockId,
    pub body: BlockId,
    pub null: BlockId,
    pub header_owner: ValueId,
    pub owner: ValueId,
    pub null_owner: ValueId,
    pub loan: LoanId,
    pub use_instruction: InstructionId,
    pub before_use: usize,
    pub before_end: Option<usize>,
    pub cleanup: Option<UnitDropFact>,
    pub non_null_first: bool,
}

impl Checkpoint {
    pub fn read(fixture: &Fixture) -> Self {
        let typed = fixture.typed.types();
        let parsed = fixture.parsed();
        let root = fixture.symbol("source");
        assert!(
            fixture
                .owned
                .ownership()
                .formed_owned_locals()
                .contains(&root)
        );
        let storage = typed.symbol_type(root).unwrap();
        let UnitTypeKind::Nullable(narrowed) = typed.types().get(storage).unwrap() else {
            panic!("nullable root")
        };
        let narrowed = *narrowed;
        let nullable = fixture.type_ids[&storage];
        let inner = fixture.type_ids[&narrowed];
        assert_eq!(
            fixture.program.modules[0].nullable_maps.get(&nullable),
            Some(&inner)
        );
        let comparisons = typed
            .null_comparisons()
            .iter()
            .filter(|fact| fact.symbol() == root)
            .collect::<Vec<_>>();
        let [comparison] = comparisons.as_slice() else {
            panic!("one root comparison")
        };
        let controls = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                let Expression::If { condition, .. } = node.payload() else {
                    return None;
                };
                let mut inner = *condition;
                while let Expression::Group { expression } =
                    parsed.ast().expressions().get(inner).unwrap().payload()
                {
                    inner = *expression;
                }
                (UnitExpressionId::new(fixture.source, inner) == comparison.expression()).then_some(
                    (
                        UnitExpressionId::new(fixture.source, id),
                        *condition,
                        node.span(),
                    ),
                )
            })
            .collect::<Vec<_>>();
        let [(control, condition, control_span)] = controls.as_slice() else {
            panic!("source comparison's unique If")
        };
        let (control, condition, control_span) = (*control, *condition, *control_span);
        let function = fixture.function();
        let births = function
            .blocks
            .iter()
            .filter_map(|block| {
                let term = block.terminator.as_ref()?;
                let TerminatorKind::NullableBranch {
                    owner,
                    when_null,
                    when_non_null,
                    view,
                } = &term.kind
                else {
                    return None;
                };
                (term.origin == Origin::Source(control_span)).then_some((
                    block.id,
                    *owner,
                    when_null,
                    when_non_null,
                    *view,
                ))
            })
            .collect::<Vec<_>>();
        let [(header, header_owner, null_edge, body_edge, loan)] = births.as_slice() else {
            panic!("one actual branch birth at control span")
        };
        let (header, header_owner, loan) = (*header, *header_owner, *loan);
        assert_eq!(
            Some(header),
            function.entry_block(),
            "these source prefixes have no earlier CFG"
        );
        assert_eq!(null_edge.arguments, [EntityId::Value(header_owner)]);
        assert_eq!(body_edge.arguments, null_edge.arguments);
        let body = body_edge.target;
        let null = null_edge.target;
        let [EntityId::Value(owner), EntityId::Loan(actual_view)] =
            function.block(body).unwrap().parameters.as_slice()
        else {
            panic!("real owner/view columns")
        };
        let owner = *owner;
        assert_eq!(*actual_view, loan);
        let [EntityId::Value(null_owner)] = function.block(null).unwrap().parameters.as_slice()
        else {
            panic!("real null owner column")
        };
        let null_owner = *null_owner;
        for (value, block, column) in [(owner, body, 0), (null_owner, null, 0)] {
            let data = function.entity(EntityId::Value(value)).unwrap();
            assert_eq!(data.ty, EntityType::Value(nullable));
            assert_eq!(
                data.definition,
                Definition::BlockParameter {
                    block,
                    index: column
                }
            );
        }
        assert_eq!(
            function.entity(EntityId::Loan(loan)).unwrap().definition,
            Definition::BlockParameter {
                block: body,
                index: 1
            }
        );
        assert_eq!(
            function.entity(EntityId::Loan(loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: inner
            }
        );
        prove_root_birth(fixture, root, header_owner, inner);
        let is_read = fixture.id.starts_with('B');
        let use_id = if is_read {
            let descriptors = typed
                .map_sizes()
                .iter()
                .filter(|descriptor| {
                    typed
                        .non_null_use(descriptor.receiver())
                        .is_some_and(|fact| fact.symbol() == root)
                })
                .collect::<Vec<_>>();
            let [descriptor] = descriptors.as_slice() else {
                panic!("one root read")
            };
            descriptor.expression()
        } else {
            let descriptors = typed
                .map_consumes()
                .iter()
                .filter(|descriptor| descriptor.source() == Some((root, storage)))
                .collect::<Vec<_>>();
            let [descriptor] = descriptors.as_slice() else {
                panic!("one root consume")
            };
            let fact = fixture
                .owned
                .ownership()
                .receiver_fact(descriptor.expression())
                .unwrap();
            assert_eq!(
                fact.kind(),
                lang_frontend::ownership_checking::UnitReceiverOwnershipKind::Move
            );
            assert_eq!(
                fact.source(),
                lang_frontend::type_checking::UnitCallReceiverOrigin::Expression(
                    descriptor.receiver()
                )
            );
            descriptor.expression()
        };
        assert_eq!(use_id.source_unit(), fixture.source);
        let use_span = parsed
            .ast()
            .expressions()
            .get(use_id.expression())
            .unwrap()
            .span();
        let receiver_span = if is_read {
            use_span
        } else {
            let descriptor = typed.map_consume(use_id).unwrap();
            parsed
                .ast()
                .expressions()
                .get(descriptor.receiver().expression())
                .unwrap()
                .span()
        };
        let candidates = function
            .block(body)
            .unwrap()
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let instruction = function.instruction(*id).unwrap();
                let expected = if is_read {
                    Operation::MapSize {
                        owner: EntityId::Loan(loan),
                    }
                } else {
                    Operation::NullableTake { owner, proof: loan }
                };
                (instruction.operation == expected
                    && instruction.origin == Origin::Source(receiver_span))
                .then_some((index, *id))
            })
            .collect::<Vec<_>>();
        let [(before_use, use_instruction)] = candidates.as_slice() else {
            panic!("actual source read/take before checkpoint")
        };
        assert_eq!(
            *before_use, 0,
            "first source read/consume has no prior body bindings"
        );
        let null_instructions = function
            .block(null)
            .unwrap()
            .instructions
            .iter()
            .map(|id| function.instruction(*id).unwrap())
            .collect::<Vec<_>>();
        if fixture.id == "B3" {
            assert!(null_instructions.is_empty());
            let body_edge = match &function
                .block(body)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .kind
            {
                TerminatorKind::Branch(edge) => edge,
                _ => panic!("B3 actual non-null exit"),
            };
            let null_edge = match &function
                .block(null)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .kind
            {
                TerminatorKind::Branch(edge) => edge,
                _ => panic!("B3 actual null exit"),
            };
            assert_eq!(body_edge.target, null_edge.target);
            assert_eq!(body_edge.arguments, [EntityId::Value(owner)]);
            assert_eq!(null_edge.arguments, [EntityId::Value(null_owner)]);
        } else {
            let null_point = UnitDropPoint::BranchExit {
                control,
                branch: usize::from(comparison.non_null_when_true()),
            };
            let facts = fixture
                .owned
                .ownership()
                .drops()
                .iter()
                .filter(|fact| {
                    fact.target() == UnitDropTarget::Named(root) && fact.point() == null_point
                })
                .collect::<Vec<_>>();
            let [fact] = facts.as_slice() else {
                panic!("actual null-arm named cleanup")
            };
            let [instruction] = null_instructions.as_slice() else {
                panic!("only actual null-arm owner drop")
            };
            assert_eq!(instruction.operation, Operation::Drop { owner: null_owner });
            assert_eq!(instruction.origin, Origin::Source(fact.value_origin()));
        }
        let before_end = function
            .block(body)
            .unwrap()
            .instructions
            .iter()
            .position(|id| {
                function.instruction(*id).unwrap().operation == Operation::BorrowEnd { loan }
            });
        let cleanup = fixture
            .owned
            .ownership()
            .drops()
            .iter()
            .find(|fact| {
                fact.target() == UnitDropTarget::Named(root)
                    && fact.point() == UnitDropPoint::CallReturn(use_id)
            })
            .copied();
        if is_read {
            assert_eq!(before_end, Some(before_use + 1));
            if fixture.id == "B3" {
                assert!(cleanup.is_none());
            } else {
                assert!(cleanup.is_some());
            }
        } else {
            assert!(before_end.is_none());
        }
        if fixture.id == "B3" {
            let end = function
                .instruction(function.block(body).unwrap().instructions[before_end.unwrap()])
                .unwrap();
            assert_eq!(end.origin, Origin::Source(control_span));
            assert_eq!(
                function.block(body).unwrap().instructions.len(),
                before_end.unwrap() + 1
            );
        }
        let alias = fixture.symbol(match fixture.id {
            "B3" => "kept",
            "B1" | "B2" => "count",
            "C2" => "seed",
            "C3" => "input",
            _ => "result",
        });
        Self {
            root,
            alias,
            storage,
            narrowed,
            nullable,
            inner,
            control,
            condition,
            control_span,
            use_id,
            use_span,
            header,
            body,
            null,
            header_owner,
            owner,
            null_owner,
            loan,
            use_instruction: *use_instruction,
            before_use: *before_use,
            before_end,
            cleanup,
            non_null_first: comparison.non_null_when_true(),
        }
    }

    pub fn install(
        &self,
        lowerer: &mut UnitExpressionLowerer<'_>,
        stage: Stage,
        prelude_bindings: BTreeMap<UnitSymbolId, LoweredValue>,
    ) {
        assert!(lowerer.borrow_bindings.is_empty() && lowerer.temporaries.is_empty());
        assert!(lowerer.pending_operands.is_empty() && lowerer.pending_call_frames.is_empty());
        // Recovered from the actual top-level declaration/put/Move transitions,
        // with every header owner and loan independently accounted for.
        assert_eq!(
            prelude_bindings,
            BTreeMap::from([(self.root, LoweredValue::Value(self.header_owner))])
        );
        lowerer.bindings = prelude_bindings;
        let body_keep = match stage {
            Stage::Producer => 0,
            Stage::BeforeUse => self.before_use,
            Stage::Cleanup | Stage::Exits => self.before_end.expect("read cleanup checkpoint"),
        };
        for block in &mut lowerer.function.blocks {
            if block.id == self.header {
                if stage == Stage::Producer {
                    block.terminator = None;
                }
                continue;
            }
            if block.id == self.body {
                block.instructions.truncate(body_keep);
            } else if block.id != self.null
                || stage == Stage::Producer
                || (stage != Stage::Exits && self.non_null_first)
            {
                block.instructions.clear();
            }
            // Null-arm instructions already ran only if the null arm was then_branch.
            // Join terminators are still future at every checkpoint used here.
            block.terminator = None;
        }
        lowerer.block = if stage == Stage::Producer {
            self.header
        } else {
            self.body
        };
        if stage != Stage::Producer {
            lowerer
                .bindings
                .insert(self.root, LoweredValue::Value(self.owner));
            lowerer
                .introduce_nullable_proof(
                    self.header,
                    0,
                    self.root,
                    self.inner,
                    nullable_proof::NullableProofScope::If(self.control),
                    self.control_span,
                )
                .unwrap();
            let guard = lowerer
                .map_borrow_condition(self.condition, self.control_span)
                .unwrap()
                .unwrap();
            lowerer.map_borrow_guards.insert(self.root, guard);
        }
        assert!(
            !lowerer
                .function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|id| matches!(
                    lowerer.function.instruction(*id).unwrap().operation,
                    Operation::NullableTake { .. } | Operation::MapConsume { .. }
                ))
        );
        if stage == Stage::Exits {
            // conditional.rs resets baseline_guards before retire_if_exit_proofs.
            lowerer.map_borrow_guards.clear();
        }
    }

    pub fn exits(&self, lowerer: &mut UnitExpressionLowerer<'_>) -> Vec<cfg::BranchExit> {
        assert!(
            self.non_null_first,
            "B3 actual then/non-null then else/null order"
        );
        let mut non_null = exit(lowerer);
        assert_eq!(non_null.block, self.body);
        assert_eq!(
            non_null.bindings,
            BTreeMap::from([(self.root, LoweredValue::Value(self.owner))])
        );
        // lower_if_branch discards Copyable count before capturing BranchExit.
        non_null.result = LoweredValue::Unit;
        let mut null = exit(lowerer);
        null.block = self.null;
        null.bindings
            .insert(self.root, LoweredValue::Value(self.null_owner));
        null.nullable_proofs.clear();
        let exits = vec![non_null, null];
        // B3 lowers then/non-null before else/null. At retirement entry the
        // active lowerer is already the latter; exits retain both complete states.
        lowerer.restore_loop_state(&exits[1]);
        exits
    }
}

fn prove_root_birth(fixture: &Fixture, root: UnitSymbolId, owner: ValueId, inner: SsaTypeId) {
    let parsed = fixture.parsed();
    let resolution = fixture
        .names
        .names()
        .source_units()
        .iter()
        .find(|unit| unit.source_unit() == root.source_unit())
        .unwrap()
        .resolution();
    let root_span = resolution.symbols()[root.symbol().index()].span();
    let declarations = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, node)| {
            let Statement::LocalVariable { declaration } = node.payload() else {
                return None;
            };
            let (_, item, _) = unwrap_modified(parsed, *declaration).unwrap();
            match item {
                Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if name == root_span => Some((node.span(), initializer)),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    let [(declaration_span, initializer)] = declarations.as_slice() else {
        panic!("actual root declaration")
    };
    let function = fixture.function();
    let initializer_id = UnitExpressionId::new(fixture.source, *initializer);
    assert!(fixture.typed.types().non_null_use(initializer_id).is_none());
    if fixture.id == "C2" {
        let seed = fixture.symbol("seed");
        let span = parsed.ast().expressions().get(*initializer).unwrap().span();
        assert_eq!(
            symbol_references(&fixture.names, fixture.source, Namespace::Value)
                .get(&span_key(span)),
            Some(&seed)
        );
        assert_eq!(
            fixture.type_ids[&fixture.typed.types().symbol_type(seed).unwrap()],
            inner
        );
    } else if fixture.id != "C3" {
        assert!(
            fixture
                .typed
                .types()
                .map_construction(initializer_id)
                .is_some()
        );
    }
    if fixture.id == "C3" {
        let input = fixture.symbol("input");
        let span = parsed.ast().expressions().get(*initializer).unwrap().span();
        assert_eq!(
            symbol_references(&fixture.names, fixture.source, Namespace::Value)
                .get(&span_key(span)),
            Some(&input)
        );
        assert_eq!(
            function
                .block(function.entry_block().unwrap())
                .unwrap()
                .parameters,
            [EntityId::Value(owner)]
        );
        assert_eq!(
            fixture.typed.types().symbol_type(input),
            fixture.typed.types().symbol_type(root)
        );
        return;
    }
    let Definition::InstructionResult {
        instruction,
        index: 0,
    } = function.entity(EntityId::Value(owner)).unwrap().definition
    else {
        panic!("source wrapper birth")
    };
    let wrap = function.instruction(instruction).unwrap();
    assert_eq!(wrap.origin, Origin::Source(*declaration_span));
    let Operation::NullableWrap {
        owner: mut current, ..
    } = wrap.operation
    else {
        panic!("actual root wrap")
    };
    let mut puts = 0;
    loop {
        let Definition::InstructionResult {
            instruction,
            index: 0,
        } = function
            .entity(EntityId::Value(current))
            .unwrap()
            .definition
        else {
            panic!("actual Map owner chain")
        };
        let instruction = function.instruction(instruction).unwrap();
        match instruction.operation {
            Operation::MapConstruct { map_type } => {
                assert_eq!(map_type, inner);
                assert!(
                    fixture
                        .typed
                        .types()
                        .map_constructions()
                        .iter()
                        .any(
                            |descriptor| descriptor.expression().source_unit() == fixture.source
                                && parsed
                                    .ast()
                                    .expressions()
                                    .get(descriptor.expression().expression())
                                    .unwrap()
                                    .span()
                                    == instruction.origin.span()
                        )
                );
                break;
            }
            Operation::MapPut { owner, .. } => {
                assert_eq!(fixture.id, "C2");
                let descriptors = fixture
                    .typed
                    .types()
                    .map_puts()
                    .iter()
                    .filter(|descriptor| {
                        descriptor.expression().source_unit() == fixture.source
                            && parsed
                                .ast()
                                .expressions()
                                .get(descriptor.expression().expression())
                                .unwrap()
                                .span()
                                == instruction.origin.span()
                    })
                    .collect::<Vec<_>>();
                let [descriptor] = descriptors.as_slice() else {
                    panic!("actual seed put descriptor")
                };
                let receiver_span = parsed
                    .ast()
                    .expressions()
                    .get(descriptor.receiver().expression())
                    .unwrap()
                    .span();
                assert_eq!(
                    symbol_references(&fixture.names, fixture.source, Namespace::Value)
                        .get(&span_key(receiver_span)),
                    Some(&fixture.symbol("seed"))
                );
                current = owner;
                puts += 1;
            }
            _ => panic!("only the actual seed put chain may precede wrap"),
        }
    }
    assert_eq!(puts, usize::from(fixture.id == "C2") * 2);
}

fn exit(lowerer: &UnitExpressionLowerer<'_>) -> cfg::BranchExit {
    cfg::BranchExit {
        block: lowerer.block,
        result: LoweredValue::Unit,
        receiver: lowerer.current_receiver,
        consumed_receiver: lowerer.consumed_receiver,
        bindings: lowerer.bindings.clone(),
        borrow_bindings: lowerer.borrow_bindings.clone(),
        nullable_proofs: lowerer.nullable_proofs.clone(),
        pending_operands: lowerer.pending_operands.clone(),
        pending_call_frames: lowerer.pending_call_frames.clone(),
        temporaries: lowerer.temporaries.clone(),
        closure_bindings: lowerer.closure_bindings.clone(),
        capture_loans: lowerer.capture_loans.clone(),
        result_source_loans: lowerer.result_source_loans.clone(),
    }
}
pub(in crate::ssa::unit_lower) fn exit_text(exit: &cfg::BranchExit) -> String {
    format!(
        "{:?}|{:?}|{:?}",
        (
            exit.block,
            exit.result,
            exit.receiver.map(|receiver| (
                receiver.owner,
                receiver.mode,
                receiver.template_ty,
                receiver.ty,
                receiver.entity,
                receiver.origin,
            )),
            exit.consumed_receiver
        ),
        (
            &exit.bindings,
            &exit.borrow_bindings,
            &exit.nullable_proofs,
            &exit.pending_operands,
            &exit.pending_call_frames
        ),
        (
            &exit.temporaries,
            &exit.closure_bindings,
            &exit.capture_loans,
            &exit.result_source_loans
        )
    )
}
pub(in crate::ssa::unit_lower) fn state_text(lowerer: &UnitExpressionLowerer<'_>) -> String {
    format!(
        "{}|{:?}",
        exit_text(&exit(lowerer)),
        (
            &lowerer.map_borrow_guards,
            &lowerer.temporary_range_slots,
            &lowerer.short_range_ends,
            &lowerer.entry_boolean_roots,
            &lowerer.entry_string_roots,
            lowerer.closure_binding_context,
            lowerer.thunk_expression,
            lowerer.source_token,
            lowerer.closure_scope,
            lowerer.loops.len()
        )
    )
}
pub(in crate::ssa::unit_lower) fn unchanged(
    lowerer: &UnitExpressionLowerer<'_>,
    before: &(Function, String),
) {
    assert_eq!(
        *lowerer.function, before.0,
        "failure must emit no Take/Consume/End/Drop or other IR"
    );
    assert_eq!(
        state_text(lowerer),
        before.1,
        "failure must not consume checkpoint state"
    );
}
