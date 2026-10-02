use lang_frontend::source::SourceMap;

use super::*;
use crate::ssa::{
    model::{FunctionId, ModuleId, Origin, Program, ScalarConstant, SsaTypeKind},
    verify::verify_program,
};

struct Fixture {
    program: Program,
    module: ModuleId,
    function: FunctionId,
    entry: BlockId,
    owner_ty: SsaTypeId,
    payload_ty: SsaTypeId,
    field_ty: SsaTypeId,
    origin: Origin,
}

impl Fixture {
    fn new(copyable: bool) -> Self {
        let mut sources = SourceMap::default();
        let source = sources.add_source("field-exchange.ko", "replace").unwrap();
        let origin = Origin::Source(sources.span(source, 0, 7).unwrap());
        let mut program = Program::default();
        let module = program.add_module("field_exchange");
        let m = program.module_mut(module).unwrap();
        let field_ty = m.intern_type(if copyable {
            SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            }
        } else {
            SsaTypeKind::StringOwner
        });
        let payload_ty = m
            .add_aggregate_type("Node.payload", vec![field_ty; 2])
            .unwrap();
        let owner_ty = m.declare_heap_owner("Node").unwrap();
        m.define_heap_owner(owner_ty, payload_ty).unwrap();
        let function = m.add_function("exchange", vec![], origin.clone()).unwrap();
        let entry = m
            .function_mut(function)
            .unwrap()
            .add_block(
                vec![
                    EntityType::Value(owner_ty),
                    EntityType::Value(owner_ty),
                    EntityType::Value(field_ty),
                ],
                origin.clone(),
            )
            .unwrap();
        Self {
            program,
            module,
            function,
            entry,
            owner_ty,
            payload_ty,
            field_ty,
            origin,
        }
    }
    fn function(&self) -> &Function {
        self.program
            .module(self.module)
            .unwrap()
            .function(self.function)
            .unwrap()
    }
    fn function_mut(&mut self) -> &mut Function {
        self.program
            .module_mut(self.module)
            .unwrap()
            .function_mut(self.function)
            .unwrap()
    }
    fn arg(&self, index: usize) -> ValueId {
        value(self.function().block(self.entry).unwrap().parameters[index])
    }
    fn append(
        &mut self,
        block: BlockId,
        operation: Operation,
        types: Vec<EntityType>,
    ) -> Vec<EntityId> {
        let origin = self.origin.clone();
        self.function_mut()
            .append_instruction(block, operation, types, origin)
            .unwrap()
            .1
    }
    fn field_place(&mut self, owner: ValueId, field: usize) -> PlaceId {
        let payload = place(
            self.append(
                self.entry,
                Operation::HeapPayloadPlace { owner },
                vec![EntityType::Place(self.payload_ty)],
            )[0],
        );
        place(
            self.append(
                self.entry,
                Operation::FieldPlace {
                    base: payload,
                    field,
                },
                vec![EntityType::Place(self.field_ty)],
            )[0],
        )
    }
    fn borrow(&mut self, place: PlaceId, kind: LoanKind) -> LoanId {
        loan(
            self.append(
                self.entry,
                Operation::BorrowBegin { place, kind },
                vec![EntityType::Loan {
                    kind,
                    target: self.field_ty,
                }],
            )[0],
        )
    }
    fn field_loan(&mut self, owner: ValueId, field: usize, kind: LoanKind) -> LoanId {
        let place = self.field_place(owner, field);
        self.borrow(place, kind)
    }
    fn exchange(
        &mut self,
        block: BlockId,
        owner: ValueId,
        field: usize,
        loan: LoanId,
        replacement: ValueId,
    ) -> ValueId {
        value(
            self.append(
                block,
                Operation::HeapFieldExchange {
                    owner,
                    field,
                    loan,
                    replacement,
                },
                vec![EntityType::Value(self.field_ty)],
            )[0],
        )
    }
    fn terminate(&mut self, block: BlockId, kind: TerminatorKind) {
        let origin = self.origin.clone();
        self.function_mut()
            .set_terminator(block, kind, origin)
            .unwrap();
    }
    fn abort(&mut self) {
        self.terminate(self.entry, TerminatorKind::Abort);
    }
    fn assert_rejected(&self) {
        assert!(
            verify_program(&self.program).is_err(),
            "malformed exchange accepted"
        );
    }
}
fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(id) = entity else {
        panic!("value")
    };
    id
}
fn place(entity: EntityId) -> PlaceId {
    let EntityId::Place(id) = entity else {
        panic!("place")
    };
    id
}
fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(id) = entity else {
        panic!("loan")
    };
    id
}

#[test]
fn field_exchange_preserves_parent_and_returns_exactly_one_old_owner() {
    for copyable in [false, true] {
        let mut f = Fixture::new(copyable);
        let [owner, other, replacement] = [f.arg(0), f.arg(1), f.arg(2)];
        let l = f.field_loan(owner, 0, LoanKind::Exclusive);
        let old = f.exchange(f.entry, owner, 0, l, replacement);
        // A new loan on the unchanged parent proves commit consumed the original loan.
        let again = f.field_loan(owner, 0, LoanKind::Shared);
        f.append(f.entry, Operation::BorrowEnd { loan: again }, vec![]);
        for value in [owner, other] {
            f.append(f.entry, Operation::Drop { owner: value }, vec![]);
        }
        if !copyable {
            f.append(f.entry, Operation::Drop { owner: old }, vec![]);
        }
        f.terminate(f.entry, TerminatorKind::Return { values: vec![] });
        assert_eq!(verify_program(&f.program), Ok(()));
    }
}

#[test]
fn field_exchange_rejects_wrong_owner_field_shared_inactive_and_reused_loans() {
    for scenario in 0..5 {
        let mut f = Fixture::new(false);
        let [owner, other, replacement] = [f.arg(0), f.arg(1), f.arg(2)];
        let l = f.field_loan(
            if scenario == 0 { other } else { owner },
            if scenario == 1 { 1 } else { 0 },
            if scenario == 2 {
                LoanKind::Shared
            } else {
                LoanKind::Exclusive
            },
        );
        if scenario == 3 {
            f.append(f.entry, Operation::BorrowEnd { loan: l }, vec![]);
        }
        let old = f.exchange(f.entry, owner, 0, l, replacement);
        if scenario == 4 {
            f.exchange(f.entry, owner, 0, l, old);
        }
        f.abort();
        f.assert_rejected();
    }
}

#[test]
fn field_exchange_rejects_unavailable_or_borrowed_replacement() {
    for borrowed in [false, true] {
        let mut f = Fixture::new(false);
        let [owner, replacement] = [f.arg(0), f.arg(2)];
        let l = f.field_loan(owner, 0, LoanKind::Exclusive);
        if borrowed {
            let p = place(
                f.append(
                    f.entry,
                    Operation::RootPlace { owner: replacement },
                    vec![EntityType::Place(f.field_ty)],
                )[0],
            );
            f.borrow(p, LoanKind::Shared);
        } else {
            f.append(f.entry, Operation::Drop { owner: replacement }, vec![]);
        }
        f.exchange(f.entry, owner, 0, l, replacement);
        f.abort();
        f.assert_rejected();
    }
}

#[test]
fn field_exchange_rejects_double_delivery_old_leaks_and_parent_leaks() {
    for scenario in 0..5 {
        let mut f = Fixture::new(false);
        let [owner, other, replacement] = [f.arg(0), f.arg(1), f.arg(2)];
        let l = f.field_loan(owner, 0, LoanKind::Exclusive);
        let old = f.exchange(f.entry, owner, 0, l, replacement);
        f.append(f.entry, Operation::Drop { owner: other }, vec![]);
        if scenario != 0 {
            f.append(f.entry, Operation::Drop { owner }, vec![]);
        }
        if scenario != 1 {
            f.append(f.entry, Operation::Drop { owner: old }, vec![]);
        }
        if scenario == 2 {
            f.append(f.entry, Operation::Drop { owner: old }, vec![]);
        }
        if scenario == 3 {
            f.append(f.entry, Operation::Drop { owner: replacement }, vec![]);
        }
        if scenario == 4 {
            f.append(f.entry, Operation::BorrowEnd { loan: l }, vec![]);
        }
        f.terminate(f.entry, TerminatorKind::Return { values: vec![] });
        f.assert_rejected();
    }
}

#[test]
fn field_exchange_rejects_wrong_types_bounds_and_result_arity() {
    for scenario in 0..5 {
        let mut f = Fixture::new(false);
        let [owner, replacement] = [f.arg(0), f.arg(2)];
        let l = f.field_loan(owner, 0, LoanKind::Exclusive);
        f.append(
            f.entry,
            Operation::HeapFieldExchange {
                owner: if scenario == 0 { replacement } else { owner },
                field: if scenario == 1 { 2 } else { 0 },
                loan: l,
                replacement: if scenario == 2 { owner } else { replacement },
            },
            if scenario == 3 {
                vec![EntityType::Value(f.owner_ty)]
            } else {
                vec![EntityType::Value(f.field_ty); if scenario == 4 { 2 } else { 1 }]
            },
        );
        f.abort();
        f.assert_rejected();
    }
}

#[test]
fn field_exchange_rejects_root_and_entry_inout_loans() {
    for entry_loan in [false, true] {
        let mut f = Fixture::new(false);
        let [owner, replacement] = [f.arg(0), f.arg(2)];
        let l = if entry_loan {
            let field_ty = f.field_ty;
            let owner_ty = f.owner_ty;
            let origin = f.origin.clone();
            f.abort();
            f.function = f
                .program
                .module_mut(f.module)
                .unwrap()
                .add_function("inout", vec![], origin.clone())
                .unwrap();
            let entry = f
                .function_mut()
                .add_block(
                    vec![
                        EntityType::Value(owner_ty),
                        EntityType::Value(field_ty),
                        EntityType::Loan {
                            kind: LoanKind::Exclusive,
                            target: field_ty,
                        },
                    ],
                    origin,
                )
                .unwrap();
            f.entry = entry;
            let args = f.function().block(entry).unwrap().parameters.clone();
            f.exchange(entry, value(args[0]), 0, loan(args[2]), value(args[1]));
            f.abort();
            f.assert_rejected();
            continue;
        } else {
            let p = place(
                f.append(
                    f.entry,
                    Operation::RootPlace { owner: replacement },
                    vec![EntityType::Place(f.field_ty)],
                )[0],
            );
            f.borrow(p, LoanKind::Exclusive)
        };
        f.exchange(f.entry, owner, 0, l, replacement);
        f.abort();
        f.assert_rejected();
    }
}

#[test]
fn field_exchange_cfg_rebinding_checks_each_pair_not_only_origin_sets() {
    for crossed in [false, true] {
        let mut f = Fixture::new(false);
        let [a, b, replacement] = [f.arg(0), f.arg(1), f.arg(2)];
        let la = f.field_loan(a, 0, LoanKind::Exclusive);
        let lb = f.field_loan(b, 0, LoanKind::Exclusive);
        let boolean = f
            .program
            .module_mut(f.module)
            .unwrap()
            .intern_type(SsaTypeKind::Boolean);
        let condition = value(
            f.append(
                f.entry,
                Operation::Constant(ScalarConstant::Boolean(true)),
                vec![EntityType::Value(boolean)],
            )[0],
        );
        let origin = f.origin.clone();
        let types = vec![
            EntityType::Value(f.owner_ty),
            EntityType::Value(f.owner_ty),
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: f.field_ty,
            },
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: f.field_ty,
            },
            EntityType::Value(f.field_ty),
        ];
        let left = f
            .function_mut()
            .add_block(types.clone(), origin.clone())
            .unwrap();
        let right = f
            .function_mut()
            .add_block(types.clone(), origin.clone())
            .unwrap();
        let mut join_types = types;
        join_types.remove(3);
        let join = f.function_mut().add_block(join_types, origin).unwrap();
        f.terminate(
            f.entry,
            TerminatorKind::Conditional {
                condition,
                when_true: Edge {
                    target: left,
                    arguments: vec![
                        EntityId::Value(a),
                        EntityId::Value(b),
                        EntityId::Loan(la),
                        EntityId::Loan(lb),
                        EntityId::Value(replacement),
                    ],
                },
                when_false: Edge {
                    target: right,
                    arguments: vec![
                        EntityId::Value(b),
                        EntityId::Value(a),
                        EntityId::Loan(lb),
                        EntityId::Loan(la),
                        EntityId::Value(replacement),
                    ],
                },
            },
        );
        for block in [left, right] {
            let mut args = f.function().block(block).unwrap().parameters.clone();
            f.append(
                block,
                Operation::BorrowEnd {
                    loan: loan(args.remove(3)),
                },
                vec![],
            );
            if crossed {
                args.swap(0, 1);
            }
            f.terminate(
                block,
                TerminatorKind::Branch(Edge {
                    target: join,
                    arguments: args,
                }),
            );
        }
        let args = f.function().block(join).unwrap().parameters.clone();
        f.exchange(join, value(args[0]), 0, loan(args[2]), value(args[3]));
        f.terminate(join, TerminatorKind::Abort);
        if crossed {
            f.assert_rejected();
        } else {
            assert_eq!(verify_program(&f.program), Ok(()));
        }
    }
}

#[test]
fn field_exchange_rejects_same_typed_parent_delivery_and_whole_root_loan() {
    for wrong_loan in [false, true] {
        let mut f = Fixture::new(false);
        let m = f.program.module_mut(f.module).unwrap();
        let node = m.declare_heap_owner("Recursive").unwrap();
        let payload = m
            .add_aggregate_type("Recursive.payload", vec![node, node])
            .unwrap();
        m.define_heap_owner(node, payload).unwrap();
        f.owner_ty = node;
        f.payload_ty = payload;
        f.field_ty = node;
        for entity in &mut f.function_mut().values {
            entity.ty = EntityType::Value(node);
        }
        let [owner, replacement] = [f.arg(0), f.arg(2)];
        let l = if wrong_loan {
            let p = place(
                f.append(
                    f.entry,
                    Operation::RootPlace { owner },
                    vec![EntityType::Place(node)],
                )[0],
            );
            f.borrow(p, LoanKind::Exclusive)
        } else {
            f.field_loan(owner, 0, LoanKind::Exclusive)
        };
        f.exchange(
            f.entry,
            owner,
            0,
            l,
            if wrong_loan { replacement } else { owner },
        );
        f.abort();
        f.assert_rejected();
    }
}

#[test]
fn field_exchange_rejects_nested_field_with_same_type_and_overlap_root() {
    let mut f = Fixture::new(false);
    let ty = f.field_ty;
    let m = f.program.module_mut(f.module).unwrap();
    let inner = m.add_aggregate_type("Inner", vec![ty]).unwrap();
    let payload = m
        .add_aggregate_type("Nested.payload", vec![ty, inner])
        .unwrap();
    let owner_ty = m.declare_heap_owner("Nested").unwrap();
    m.define_heap_owner(owner_ty, payload).unwrap();
    f.owner_ty = owner_ty;
    f.payload_ty = payload;
    for entity in &mut f.function_mut().values[..2] {
        entity.ty = EntityType::Value(owner_ty);
    }
    let [owner, replacement] = [f.arg(0), f.arg(2)];
    let base = place(
        f.append(
            f.entry,
            Operation::HeapPayloadPlace { owner },
            vec![EntityType::Place(payload)],
        )[0],
    );
    let middle = place(
        f.append(
            f.entry,
            Operation::FieldPlace { base, field: 1 },
            vec![EntityType::Place(inner)],
        )[0],
    );
    let nested = place(
        f.append(
            f.entry,
            Operation::FieldPlace {
                base: middle,
                field: 0,
            },
            vec![EntityType::Place(ty)],
        )[0],
    );
    let l = f.borrow(nested, LoanKind::Exclusive);
    f.exchange(f.entry, owner, 0, l, replacement);
    f.abort();
    f.assert_rejected();
}

#[test]
fn field_exchange_keeps_ordinary_move_only_field_reads_forbidden() {
    let mut f = Fixture::new(false);
    let [owner, replacement] = [f.arg(0), f.arg(2)];
    let p = f.field_place(owner, 0);
    let l = f.borrow(p, LoanKind::Exclusive);
    f.append(
        f.entry,
        Operation::Read {
            source: PlaceAccess::Loan(l),
        },
        vec![EntityType::Value(f.field_ty)],
    );
    f.exchange(f.entry, owner, 0, l, replacement);
    f.abort();
    let errors = verify_program(&f.program).unwrap_err().errors;
    assert!(
        errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::MoveOnlyPlaceRead { .. }))
    );
}

#[test]
fn field_exchange_accepts_payload_projection_and_loan_across_cfg_loop() {
    let mut f = Fixture::new(false);
    let [owner, other, replacement] = [f.arg(0), f.arg(1), f.arg(2)];
    f.append(f.entry, Operation::Drop { owner: other }, vec![]);
    let payload = f.payload_ty;
    let field_ty = f.field_ty;
    let parent_ty = f.owner_ty;
    let p = place(
        f.append(
            f.entry,
            Operation::HeapPayloadPlace { owner },
            vec![EntityType::Place(payload)],
        )[0],
    );
    let origin = f.origin.clone();
    let projected = f
        .function_mut()
        .add_block(
            vec![
                EntityType::Value(parent_ty),
                EntityType::Place(payload),
                EntityType::Value(field_ty),
            ],
            origin.clone(),
        )
        .unwrap();
    let transported = vec![
        EntityType::Value(parent_ty),
        EntityType::Loan {
            kind: LoanKind::Exclusive,
            target: field_ty,
        },
        EntityType::Value(field_ty),
    ];
    let header = f
        .function_mut()
        .add_block(transported.clone(), origin.clone())
        .unwrap();
    let exit = f.function_mut().add_block(transported, origin).unwrap();
    f.terminate(
        f.entry,
        TerminatorKind::Branch(Edge {
            target: projected,
            arguments: vec![
                EntityId::Value(owner),
                EntityId::Place(p),
                EntityId::Value(replacement),
            ],
        }),
    );
    let args = f.function().block(projected).unwrap().parameters.clone();
    let fp = place(
        f.append(
            projected,
            Operation::FieldPlace {
                base: place(args[1]),
                field: 0,
            },
            vec![EntityType::Place(field_ty)],
        )[0],
    );
    let l = loan(
        f.append(
            projected,
            Operation::BorrowBegin {
                place: fp,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: field_ty,
            }],
        )[0],
    );
    f.terminate(
        projected,
        TerminatorKind::Branch(Edge {
            target: header,
            arguments: vec![args[0], EntityId::Loan(l), args[2]],
        }),
    );
    let boolean = f
        .program
        .module_mut(f.module)
        .unwrap()
        .intern_type(SsaTypeKind::Boolean);
    let condition = value(
        f.append(
            header,
            Operation::Constant(ScalarConstant::Boolean(false)),
            vec![EntityType::Value(boolean)],
        )[0],
    );
    let args = f.function().block(header).unwrap().parameters.clone();
    f.terminate(
        header,
        TerminatorKind::Conditional {
            condition,
            when_true: Edge {
                target: header,
                arguments: args.clone(),
            },
            when_false: Edge {
                target: exit,
                arguments: args,
            },
        },
    );
    let args = f.function().block(exit).unwrap().parameters.clone();
    let old = f.exchange(exit, value(args[0]), 0, loan(args[1]), value(args[2]));
    f.append(
        exit,
        Operation::Drop {
            owner: value(args[0]),
        },
        vec![],
    );
    f.append(exit, Operation::Drop { owner: old }, vec![]);
    f.terminate(exit, TerminatorKind::Return { values: vec![] });
    assert_eq!(verify_program(&f.program), Ok(()));
}

#[test]
fn field_exchange_retains_conservative_sibling_loan_backend_boundary() {
    let mut f = Fixture::new(false);
    let [owner, replacement] = [f.arg(0), f.arg(2)];
    let l = f.field_loan(owner, 0, LoanKind::Exclusive);
    // Language-level disjoint fields are legal, but this backend's existing AliasRoots
    // intentionally remains conservative. This test does not make the source illegal.
    f.field_loan(owner, 1, LoanKind::Shared);
    f.exchange(f.entry, owner, 0, l, replacement);
    f.abort();
    let errors = verify_program(&f.program).unwrap_err().errors;
    assert!(
        errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::BorrowConflict { .. }))
    );
}

#[test]
fn field_exchange_cfg_rejects_same_typed_other_field_on_one_incoming_edge() {
    for wrong_field in [false, true] {
        let mut f = Fixture::new(false);
        let [owner, other, replacement] = [f.arg(0), f.arg(1), f.arg(2)];
        f.append(f.entry, Operation::Drop { owner: other }, vec![]);
        let boolean = f
            .program
            .module_mut(f.module)
            .unwrap()
            .intern_type(SsaTypeKind::Boolean);
        let condition = value(
            f.append(
                f.entry,
                Operation::Constant(ScalarConstant::Boolean(true)),
                vec![EntityType::Value(boolean)],
            )[0],
        );
        let parent_ty = f.owner_ty;
        let field_ty = f.field_ty;
        let payload_ty = f.payload_ty;
        let origin = f.origin.clone();
        let branch_types = vec![EntityType::Value(parent_ty), EntityType::Value(field_ty)];
        let left = f
            .function_mut()
            .add_block(branch_types.clone(), origin.clone())
            .unwrap();
        let right = f
            .function_mut()
            .add_block(branch_types, origin.clone())
            .unwrap();
        let join = f
            .function_mut()
            .add_block(
                vec![
                    EntityType::Value(parent_ty),
                    EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target: field_ty,
                    },
                    EntityType::Value(field_ty),
                ],
                origin,
            )
            .unwrap();
        let arguments = vec![EntityId::Value(owner), EntityId::Value(replacement)];
        f.terminate(
            f.entry,
            TerminatorKind::Conditional {
                condition,
                when_true: Edge {
                    target: left,
                    arguments: arguments.clone(),
                },
                when_false: Edge {
                    target: right,
                    arguments,
                },
            },
        );
        for block in [left, right] {
            let args = f.function().block(block).unwrap().parameters.clone();
            let payload = place(
                f.append(
                    block,
                    Operation::HeapPayloadPlace {
                        owner: value(args[0]),
                    },
                    vec![EntityType::Place(payload_ty)],
                )[0],
            );
            let p = place(
                f.append(
                    block,
                    Operation::FieldPlace {
                        base: payload,
                        field: usize::from(wrong_field && block == right),
                    },
                    vec![EntityType::Place(field_ty)],
                )[0],
            );
            let l = loan(
                f.append(
                    block,
                    Operation::BorrowBegin {
                        place: p,
                        kind: LoanKind::Exclusive,
                    },
                    vec![EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target: field_ty,
                    }],
                )[0],
            );
            f.terminate(
                block,
                TerminatorKind::Branch(Edge {
                    target: join,
                    arguments: vec![args[0], EntityId::Loan(l), args[1]],
                }),
            );
        }
        let args = f.function().block(join).unwrap().parameters.clone();
        f.exchange(join, value(args[0]), 0, loan(args[1]), value(args[2]));
        f.terminate(join, TerminatorKind::Abort);
        if wrong_field {
            f.assert_rejected();
        } else {
            assert_eq!(verify_program(&f.program), Ok(()));
        }
    }
}
