//! CI-only private-fact/fault-injection whole; never an ordinary-source success claim.
use super::*;
use crate::ssa::model::FunctionId;
use crate::ssa::unit_lower::{
    call_lifetimes::{PendingCallFrame, nullable_child::PendingNullableChild},
    nullable_proof::{
        NullableProofScope,
        guard_tests::{
            Birth, install_birth,
            support::{Fixture, rejected},
        },
    },
};

const SOURCE: &str = r#"class Holder(val value: Int)
fun pass(own marker: Int, own value: Int): Int {
    return if (true) { marker + value } else { marker + value }
}
fun entry(own initial: Holder?): Int {
    var current: Holder? = initial
    while (current != null) { return pass(1, current.value) }
    return 0
}"#;

#[derive(Clone, Copy, Debug)]
enum Fault {
    Loan,
    Place,
    MoveOnly,
    UnknownValue,
    ForeignFunction,
    BooleanValue,
    CrossBlock,
    UnlistedInstruction,
    WrongResultSlot,
    WrongInstructionBlock,
    WrongParameterSlot,
    CrossBlockParameter,
    NoFrame,
    NestedFrame,
    PendingStart,
    ReceiverFrame,
    LoanArgument,
    CreatedLoan,
    AbiSlot,
    FieldOwner,
    SharedFieldRoot,
    ExclusiveOwner,
    NullableChild,
    NonCall,
    ForeignCall,
    NonWhile,
    MissingBinding,
    AliasedProofLoan,
    ExclusiveProof,
    BorrowReturn,
    CarrierReturn,
    CaptureLoan,
    ResultLoan,
    RangeSlot,
    ShortRangeEnd,
    BorrowBinding,
    CurrentReceiver,
}

#[test]
fn unit_nullable_retirement_int_prefix_private_guards() {
    let fixture = Fixture::new(SOURCE);
    crate::llvm::render_verified_program(&fixture.program)
        .expect("unmodified private-test source reaches LLVM before fault injection");
    let current = fixture.symbol("current");
    let initial = fixture.symbol("initial");
    let foreign = foreign_source();
    assert_ne!(foreign, fixture.source);
    for fault in [
        Fault::Loan,
        Fault::Place,
        Fault::MoveOnly,
        Fault::UnknownValue,
        Fault::ForeignFunction,
        Fault::BooleanValue,
        Fault::CrossBlock,
        Fault::UnlistedInstruction,
        Fault::WrongResultSlot,
        Fault::WrongInstructionBlock,
        Fault::WrongParameterSlot,
        Fault::CrossBlockParameter,
        Fault::NoFrame,
        Fault::NestedFrame,
        Fault::PendingStart,
        Fault::ReceiverFrame,
        Fault::LoanArgument,
        Fault::CreatedLoan,
        Fault::AbiSlot,
        Fault::FieldOwner,
        Fault::SharedFieldRoot,
        Fault::ExclusiveOwner,
        Fault::NullableChild,
        Fault::NonCall,
        Fault::ForeignCall,
        Fault::NonWhile,
        Fault::MissingBinding,
        Fault::AliasedProofLoan,
        Fault::ExclusiveProof,
        Fault::BorrowReturn,
        Fault::CarrierReturn,
        Fault::CaptureLoan,
        Fault::ResultLoan,
        Fault::RangeSlot,
        Fault::ShortRangeEnd,
        Fault::BorrowBinding,
        Fault::CurrentReceiver,
    ] {
        let mut function = fixture.function().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            let birth = checkpoint(lowerer, current, &[current]);
            let proof = lowerer.nullable_proofs[&current];
            let marker = lowerer.pending_operands[0];
            let EntityId::Value(marker_value) = marker else {
                unreachable!()
            };
            let Definition::InstructionResult {
                instruction: marker_instruction,
                ..
            } = lowerer.function.entity(marker).unwrap().definition
            else {
                unreachable!()
            };
            let call = lowerer.pending_call_frames[0].call;
            let expected = match fault {
                Fault::MissingBinding | Fault::ExclusiveProof => LoweringErrorKind::MissingFact,
                _ => LoweringErrorKind::UnsupportedNode,
            };
            match fault {
                Fault::Loan => lowerer.pending_operands[0] = EntityId::Loan(proof.loan),
                Fault::Place => {
                    let ty = lowerer.function.entity(marker).unwrap().ty.semantic_type();
                    let (_, entities) = lowerer
                        .function
                        .append_instruction(
                            birth.body,
                            Operation::RootPlace {
                                owner: marker_value,
                            },
                            vec![EntityType::Place(ty)],
                            Origin::Source(birth.span),
                        )
                        .unwrap();
                    assert!(matches!(entities[0], EntityId::Place(_)));
                    lowerer.pending_operands[0] = entities[0];
                }
                Fault::MoveOnly => lowerer.pending_operands[0] = EntityId::Value(proof.owner),
                Fault::UnknownValue => {
                    lowerer.pending_operands[0] = EntityId::Value(ValueId {
                        function: lowerer.function.id,
                        index: lowerer.function.values.len(),
                    })
                }
                Fault::ForeignFunction => {
                    lowerer.pending_operands[0] = EntityId::Value(ValueId {
                        function: FunctionId {
                            module: lowerer.function.id.module,
                            index: usize::MAX,
                        },
                        index: marker_value.index(),
                    })
                }
                Fault::BooleanValue => {
                    let boolean = lowerer.type_ids
                        [&lowerer.typed.types().builtin(BuiltinType::Boolean).unwrap()];
                    lowerer.function.values[marker_value.index()].ty = EntityType::Value(boolean);
                }
                Fault::CrossBlock => {
                    lowerer.pending_operands[0] = lowerer
                        .function
                        .instructions
                        .iter()
                        .find(|instruction| {
                            instruction.block != birth.body
                                && matches!(
                                    instruction.operation,
                                    Operation::Constant(ScalarConstant::Integer(0))
                                )
                        })
                        .expect("real null-side Int has a different block definition")
                        .results[0];
                }
                Fault::UnlistedInstruction => {
                    lowerer.function.blocks[birth.body.index()]
                        .instructions
                        .retain(|id| *id != marker_instruction);
                }
                Fault::WrongResultSlot => {
                    lowerer.function.values[marker_value.index()].definition =
                        Definition::InstructionResult {
                            instruction: marker_instruction,
                            index: 1,
                        };
                }
                Fault::WrongInstructionBlock => {
                    lowerer.function.instructions[marker_instruction.index()].block = birth.header;
                }
                Fault::WrongParameterSlot => {
                    lowerer.function.values[marker_value.index()].definition =
                        Definition::BlockParameter {
                            block: birth.body,
                            index: birth.column,
                        };
                }
                Fault::CrossBlockParameter => {
                    lowerer.function.values[marker_value.index()].definition =
                        Definition::BlockParameter {
                            block: birth.header,
                            index: birth.column,
                        };
                }
                Fault::NoFrame => lowerer.pending_call_frames.clear(),
                Fault::NestedFrame => lowerer
                    .pending_call_frames
                    .push(lowerer.pending_call_frames[0].clone()),
                Fault::PendingStart => lowerer.pending_call_frames[0].pending_start = 1,
                Fault::ReceiverFrame => lowerer.pending_call_frames[0].receiver = true,
                Fault::LoanArgument => lowerer.pending_call_frames[0]
                    .loan_arguments
                    .push((call, vec![0])),
                Fault::CreatedLoan => lowerer.pending_call_frames[0].created_loans.push(0),
                Fault::AbiSlot => lowerer.pending_call_frames[0].abi_slots.push((0, 0)),
                Fault::FieldOwner => lowerer.pending_call_frames[0].field_replace_owner = Some(0),
                Fault::SharedFieldRoot => lowerer.pending_call_frames[0]
                    .shared_field_roots
                    .push(current),
                Fault::ExclusiveOwner => {
                    lowerer.pending_call_frames[0].exclusive_root_owners.push(0)
                }
                Fault::NullableChild => {
                    let target = lowerer.function.entity(marker).unwrap().ty.semantic_type();
                    lowerer.pending_call_frames[0]
                        .nullable_children
                        .push(PendingNullableChild {
                            call,
                            argument: call,
                            root: current,
                            parent_origin: proof.origin,
                            parent_inner: proof.inner,
                            field: 0,
                            target,
                            slot: 0,
                        });
                }
                Fault::NonCall => {
                    let expression = lowerer
                        .parsed
                        .ast()
                        .expressions()
                        .iter()
                        .find(|(_, node)| matches!(node.payload(), Expression::Name))
                        .unwrap()
                        .0;
                    lowerer.pending_call_frames[0].call =
                        UnitExpressionId::new(lowerer.source_unit, expression);
                }
                Fault::ForeignCall => {
                    lowerer.pending_call_frames[0].call =
                        UnitExpressionId::new(foreign, call.expression());
                }
                Fault::NonWhile => {
                    let expression = lowerer
                        .parsed
                        .ast()
                        .expressions()
                        .iter()
                        .find(|(_, node)| matches!(node.payload(), Expression::If { .. }))
                        .unwrap()
                        .0;
                    lowerer.nullable_proofs.clear();
                    lowerer
                        .introduce_nullable_proof(
                            birth.header,
                            birth.column,
                            current,
                            birth.inner,
                            NullableProofScope::If(UnitExpressionId::new(
                                lowerer.source_unit,
                                expression,
                            )),
                            birth.span,
                        )
                        .unwrap();
                    // The shared proof identity check accepts this injected If scope;
                    // only while retirement may use the new nonempty-prefix exception.
                    assert!(lowerer.nullable_proofs[&current].while_scope().is_none());
                    lowerer
                        .validate_nullable_proofs(
                            &lowerer.bindings,
                            &lowerer.nullable_proofs,
                            birth.span,
                        )
                        .unwrap();
                }
                Fault::MissingBinding => {
                    lowerer.bindings.remove(&current);
                }
                Fault::AliasedProofLoan => {
                    lowerer
                        .bindings
                        .insert(initial, LoweredValue::Value(proof.owner));
                    lowerer.nullable_proofs.insert(initial, proof);
                    lowerer
                        .validate_nullable_proofs(
                            &lowerer.bindings,
                            &lowerer.nullable_proofs,
                            birth.span,
                        )
                        .unwrap();
                }
                Fault::ExclusiveProof => {
                    lowerer.function.loans[proof.loan.index()].ty = EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target: proof.inner,
                    };
                }
                Fault::BorrowReturn => lowerer.function.borrow_return = Some(0),
                Fault::CarrierReturn => lowerer.function.carrier_return = Some(0),
                Fault::CaptureLoan => {
                    lowerer.capture_loans.insert((call, 0), proof.loan);
                }
                Fault::ResultLoan => {
                    lowerer
                        .result_source_loans
                        .insert(current, vec![proof.loan]);
                }
                Fault::RangeSlot => {
                    lowerer.temporary_range_slots.insert(call, vec![0]);
                }
                Fault::ShortRangeEnd => {
                    lowerer.short_range_ends.insert(proof.loan, call);
                }
                Fault::BorrowBinding => {
                    lowerer.borrow_bindings.insert(current, proof.loan);
                }
                Fault::CurrentReceiver => {
                    let UnitTypeKind::Nullable(inner) = lowerer
                        .typed
                        .types()
                        .get(lowerer.typed.symbol_type(current).unwrap())
                        .unwrap()
                    else {
                        unreachable!()
                    };
                    let UnitTypeKind::Nominal { declaration, .. } =
                        lowerer.typed.types().get(*inner).unwrap()
                    else {
                        unreachable!()
                    };
                    lowerer.current_receiver = Some(ReceiverBinding {
                        owner: *declaration,
                        mode: ParameterMode::Borrow,
                        template_ty: *inner,
                        ty: *inner,
                        entity: EntityId::Loan(proof.loan),
                        origin: birth.span,
                    });
                }
            }
            let before = lowerer.function.clone();
            let proofs = lowerer.nullable_proofs.clone();
            let bindings = lowerer.bindings.clone();
            let pending = lowerer.pending_operands.clone();
            let frames = lowerer.pending_call_frames.clone();
            rejected(
                lowerer.end_nullable_proof(current, birth.span),
                expected,
                birth.span,
            );
            assert_eq!(
                *lowerer.function, before,
                "{fault:?}: reject before BorrowEnd/Drop"
            );
            assert_eq!(lowerer.nullable_proofs, proofs, "{fault:?}: retain proof");
            assert_eq!(
                lowerer.bindings, bindings,
                "{fault:?}: retain owner binding"
            );
            assert_eq!(
                lowerer.pending_operands, pending,
                "{fault:?}: retain prefix"
            );
            assert_eq!(
                lowerer.pending_call_frames, frames,
                "{fault:?}: retain frame"
            );
        });
    }

    // Changing a legitimate same-block Int's source is not a lifetime violation.
    // These synthetic checkpoint variants do not replace the whole-source call-order oracle.
    for read_prefix in [false, true] {
        let mut function = fixture.function().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            let birth = checkpoint(lowerer, current, &[current]);
            let proof = lowerer.nullable_proofs[&current];
            if read_prefix {
                let read = lowerer
                    .function
                    .instructions
                    .iter()
                    .find(|instruction| {
                        matches!(instruction.operation, Operation::HeapFieldRead { .. })
                    })
                    .unwrap()
                    .results[0];
                lowerer.pending_operands.push(read);
            }
            lowerer
                .validate_nullable_proof_retirement(current, birth.span)
                .unwrap();
            rejected(
                lowerer.ensure_no_while_proof_carriers(birth.span),
                LoweringErrorKind::UnsupportedNode,
                birth.span,
            );
            let pending = lowerer.pending_operands.clone();
            let frames = lowerer.pending_call_frames.clone();
            let bindings = lowerer.bindings.clone();
            let instructions = lowerer.function.instructions.len();
            lowerer.end_nullable_proof(current, birth.span).unwrap();
            assert!(!lowerer.nullable_proofs.contains_key(&current));
            assert_eq!(lowerer.pending_operands, pending);
            assert_eq!(lowerer.pending_call_frames, frames);
            assert_eq!(lowerer.bindings, bindings);
            assert_eq!(lowerer.function.instructions.len(), instructions + 1);
            assert!(
                matches!(lowerer.function.instructions.last().unwrap().operation,
                Operation::BorrowEnd { loan } if loan == proof.loan)
            );
            lowerer.end_nullable_proof(current, birth.span).unwrap();
            assert_eq!(lowerer.function.instructions.len(), instructions + 1);
        });
    }

    // An actual current-block Int parameter is equally independent. Its source
    // reaches the same ordinary pipeline, and is not a fabricated instruction ID.
    let parameter_source = SOURCE
        .replace(
            "own initial: Holder?",
            "own initial: Holder?, own seed: Int",
        )
        .replace("pass(1, current.value)", "pass(seed, current.value)");
    let parameter_fixture = Fixture::new(&parameter_source);
    let current = parameter_fixture.symbol("current");
    let seed = parameter_fixture.symbol("seed");
    let mut function = parameter_fixture.function().clone();
    parameter_fixture.with_lowerer(&mut function, |lowerer| {
        let birth = checkpoint(lowerer, current, &[current, seed]);
        let prefix = lowerer.pending_operands[0];
        assert!(
            matches!(lowerer.function.entity(prefix).unwrap().definition,
            Definition::BlockParameter { block, .. } if block == birth.body)
        );
        lowerer
            .validate_nullable_proof_retirement(current, birth.span)
            .unwrap();
        lowerer.pending_operands.clear();
        lowerer.ensure_no_while_proof_carriers(birth.span).unwrap();
        lowerer.pending_call_frames.clear();
        lowerer.ensure_no_while_proof_carriers(birth.span).unwrap();
    });
}

fn checkpoint(
    lowerer: &mut UnitExpressionLowerer<'_>,
    current: UnitSymbolId,
    live: &[UnitSymbolId],
) -> Birth {
    let birth = install_birth(lowerer, current, live);
    let proof = lowerer.nullable_proofs[&current];
    let calls = lowerer
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::Call { .. })
                .then_some(UnitExpressionId::new(lowerer.source_unit, id))
        })
        .collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        panic!("one actual source call")
    };
    let descriptor = lowerer.typed.call(*call).unwrap();
    assert_eq!(descriptor.expression(), *call);
    assert_eq!(descriptor.arguments().len(), 2);
    let marker = lowerer
        .function
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::DirectCall { arguments, .. } if instruction.block == birth.body => {
                assert_eq!(arguments.len(), 2);
                Some(arguments[0])
            }
            _ => None,
        })
        .expect("the actual first call argument supplies the pending Int");
    let ended = lowerer.function.instructions.iter().find(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == proof.loan)
    }).unwrap().id;
    let block = &mut lowerer.function.blocks[birth.body.index()];
    let position = block
        .instructions
        .iter()
        .position(|id| *id == ended)
        .unwrap();
    block.instructions.truncate(position);
    block.terminator = None;
    // Restore this documented point from real SSA. Completed-function suffix entities
    // remain in the arena, but no longer count as definitions in the active block.
    lowerer.pending_operands = vec![marker];
    lowerer.pending_call_frames = vec![PendingCallFrame {
        call: *call,
        receiver: false,
        pending_start: 0,
        loop_depth: lowerer.loops.len(),
        nullable_children: Vec::new(),
        loan_arguments: Vec::new(),
        created_loans: Vec::new(),
        abi_slots: Vec::new(),
        field_replace_owner: None,
        shared_field_roots: Vec::new(),
        exclusive_root_owners: Vec::new(),
    }];
    let before = lowerer.function.clone();
    lowerer
        .validate_nullable_proof_retirement(current, birth.span)
        .unwrap();
    assert_eq!(
        *lowerer.function, before,
        "retirement qualification is read-only"
    );
    assert_eq!(lowerer.nullable_proofs[&current], proof);
    rejected(
        lowerer.ensure_no_while_proof_carriers(birth.span),
        LoweringErrorKind::UnsupportedNode,
        birth.span,
    );
    birth
}

fn foreign_source() -> SourceUnitId {
    use crate::ssa::unit_lower_test_support::parsed;
    use lang_frontend::name_resolution::index_compilation_unit;
    let mut sources = SourceMap::new();
    let (a, a_file) = parsed(&mut sources, "a.ko", "package a\nfun a(): Int { return 1 }");
    let (b, b_file) = parsed(&mut sources, "b.ko", "package b\nfun b(): Int { return 2 }");
    let inputs = [
        SourceUnitInput::new("root", "a.ko", a, &a_file),
        SourceUnitInput::new("root", "b.ko", b, &b_file),
    ];
    index_compilation_unit(&sources, &inputs)
        .unwrap()
        .source_units()[1]
        .id()
}
