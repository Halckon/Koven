use lang_frontend::source::SourceMap;

use super::*;
use crate::ssa::{
    model::{FunctionId, ModuleId, Origin, Program, SsaTypeKind},
    verify::verify_program,
};

struct Fixture {
    program: Program,
    module: ModuleId,
    function: FunctionId,
    entry: BlockId,
    ty: SsaTypeId,
    origin: Origin,
}

impl Fixture {
    fn new(copyable: bool) -> Self {
        let mut sources = SourceMap::default();
        let source = sources
            .add_source("root-exchange.ko", "replace swap")
            .unwrap();
        let origin = Origin::Source(sources.span(source, 0, 7).unwrap());
        let mut program = Program::default();
        let module = program.add_module("root_exchange");
        let m = program.module_mut(module).unwrap();
        let ty = m.intern_type(if copyable {
            SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            }
        } else {
            SsaTypeKind::StringOwner
        });
        let function = m.add_function("exchange", vec![], origin.clone()).unwrap();
        let entry = m
            .function_mut(function)
            .unwrap()
            .add_block(vec![EntityType::Value(ty); 3], origin.clone())
            .unwrap();
        Self {
            program,
            module,
            function,
            entry,
            ty,
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

    fn owner(&self, index: usize) -> ValueId {
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

    fn root(&mut self, owner: ValueId, kind: LoanKind) -> (PlaceId, LoanId) {
        let place = place(
            self.append(
                self.entry,
                Operation::RootPlace { owner },
                vec![EntityType::Place(self.ty)],
            )[0],
        );
        let loan = loan(
            self.append(
                self.entry,
                Operation::BorrowBegin { place, kind },
                vec![EntityType::Loan {
                    kind,
                    target: self.ty,
                }],
            )[0],
        );
        (place, loan)
    }

    fn replace(
        &mut self,
        block: BlockId,
        owner: ValueId,
        loan: LoanId,
        replacement: ValueId,
    ) -> Vec<EntityId> {
        self.append(
            block,
            Operation::RootReplace {
                owner,
                loan,
                replacement,
            },
            vec![EntityType::Value(self.ty); 2],
        )
    }

    fn finish(&mut self, block: BlockId) {
        let origin = self.origin.clone();
        self.function_mut()
            .set_terminator(block, TerminatorKind::Abort, origin)
            .unwrap();
    }

    fn branch(&mut self, block: BlockId, edge: Edge) {
        let origin = self.origin.clone();
        self.function_mut()
            .set_terminator(block, TerminatorKind::Branch(edge), origin)
            .unwrap();
    }

    fn errors(&self) -> Vec<VerifyError> {
        verify_program(&self.program)
            .expect_err("must reject invalid root exchange")
            .errors
    }
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value")
    };
    value
}
fn place(entity: EntityId) -> PlaceId {
    let EntityId::Place(place) = entity else {
        panic!("expected place")
    };
    place
}
fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(loan) = entity else {
        panic!("expected loan")
    };
    loan
}

#[test]
fn root_replace_and_swap_accept_owned_scalar_and_string_roots() {
    for copyable in [false, true] {
        let mut f = Fixture::new(copyable);
        let [a, b, c] = [f.owner(0), f.owner(1), f.owner(2)];
        let (_, l) = f.root(a, LoanKind::Exclusive);
        let results = f.replace(f.entry, a, l, b);
        let new_a = value(results[0]);
        let (_, la) = f.root(new_a, LoanKind::Exclusive);
        let (_, lc) = f.root(c, LoanKind::Exclusive);
        let swapped = f.append(
            f.entry,
            Operation::RootSwap {
                owners: [new_a, c],
                loans: [la, lc],
            },
            vec![EntityType::Value(f.ty); 2],
        );
        if !copyable {
            for owner in [value(results[1]), value(swapped[0]), value(swapped[1])] {
                f.append(f.entry, Operation::Drop { owner }, vec![]);
            }
        }
        let origin = f.origin.clone();
        let entry = f.entry;
        f.function_mut()
            .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
            .unwrap();
        assert_eq!(verify_program(&f.program), Ok(()));
    }
}

#[test]
fn root_replace_rejects_shared_inactive_wrong_and_duplicate_loans() {
    for scenario in 0..4 {
        let mut f = Fixture::new(false);
        let [a, b, c] = [f.owner(0), f.owner(1), f.owner(2)];
        let (_, l) = f.root(
            if scenario == 2 { c } else { a },
            if scenario == 0 {
                LoanKind::Shared
            } else {
                LoanKind::Exclusive
            },
        );
        if scenario == 1 {
            f.append(f.entry, Operation::BorrowEnd { loan: l }, vec![]);
        }
        f.replace(f.entry, a, l, b);
        if scenario == 3 {
            f.replace(f.entry, a, l, c);
        }
        f.finish(f.entry);
        assert!(!f.errors().is_empty(), "scenario {scenario}");
    }
}

#[test]
fn root_exchange_rejects_aliased_replacement_and_overlapping_swap() {
    for copyable in [false, true] {
        let mut f = Fixture::new(copyable);
        let a = f.owner(0);
        let (_, l) = f.root(a, LoanKind::Exclusive);
        f.replace(f.entry, a, l, a);
        f.finish(f.entry);
        assert!(!f.errors().is_empty());

        let mut f = Fixture::new(copyable);
        let a = f.owner(0);
        let (_, l) = f.root(a, LoanKind::Exclusive);
        f.append(
            f.entry,
            Operation::RootSwap {
                owners: [a, a],
                loans: [l, l],
            },
            vec![EntityType::Value(f.ty); 2],
        );
        f.finish(f.entry);
        assert!(!f.errors().is_empty());
    }
}

#[test]
fn root_exchange_invalidates_old_places_even_for_copyable_values() {
    for copyable in [false, true] {
        let mut f = Fixture::new(copyable);
        let [a, b] = [f.owner(0), f.owner(1)];
        let (p, l) = f.root(a, LoanKind::Exclusive);
        f.replace(f.entry, a, l, b);
        f.append(
            f.entry,
            Operation::BorrowBegin {
                place: p,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: f.ty,
            }],
        );
        f.finish(f.entry);
        assert!(
            f.errors().iter().any(
                |e| matches!(e.kind, VerifyErrorKind::PlaceUnavailable { place } if place == p)
            )
        );
    }
}

#[test]
fn root_replace_rejects_wrong_replacement_and_result_types() {
    for bad_result in [false, true] {
        let mut f = Fixture::new(false);
        let a = f.owner(0);
        let b = f.owner(1);
        let (_, l) = f.root(a, LoanKind::Exclusive);
        let int = f
            .program
            .module_mut(f.module)
            .unwrap()
            .intern_type(SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            });
        let wrong = value(
            f.append(
                f.entry,
                Operation::Constant(crate::ssa::model::ScalarConstant::Integer(3)),
                vec![EntityType::Value(int)],
            )[0],
        );
        f.append(
            f.entry,
            Operation::RootReplace {
                owner: a,
                loan: l,
                replacement: if bad_result { b } else { wrong },
            },
            vec![EntityType::Value(if bad_result { int } else { f.ty }); 2],
        );
        f.finish(f.entry);
        assert!(
            f.errors()
                .iter()
                .any(|e| matches!(e.kind, VerifyErrorKind::OperationContract { .. }))
        );
    }
}

#[test]
fn root_replace_accepts_paired_owner_loan_cfg_rebinding() {
    let mut f = Fixture::new(false);
    let [a, b] = [f.owner(0), f.owner(1)];
    let (_, l) = f.root(a, LoanKind::Exclusive);
    let origin = f.origin.clone();
    let ty = f.ty;
    let next = f
        .function_mut()
        .add_block(
            vec![
                EntityType::Value(ty),
                EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: ty,
                },
                EntityType::Value(ty),
            ],
            origin,
        )
        .unwrap();
    let c = f.owner(2);
    f.append(f.entry, Operation::Drop { owner: c }, vec![]);
    f.branch(
        f.entry,
        Edge {
            target: next,
            arguments: vec![EntityId::Value(a), EntityId::Loan(l), EntityId::Value(b)],
        },
    );
    let args = f.function().block(next).unwrap().parameters.clone();
    f.replace(next, value(args[0]), loan(args[1]), value(args[2]));
    f.finish(next);
    assert_eq!(verify_program(&f.program), Ok(()));
}

#[test]
fn root_replace_rejects_same_typed_field_loan_with_identical_overlap_roots() {
    let mut f = Fixture::new(false);
    let module = f.program.module_mut(f.module).unwrap();
    let node = module.declare_heap_owner("Node").unwrap();
    let payload = module
        .add_aggregate_type("Node.payload", vec![node])
        .unwrap();
    module.define_heap_owner(node, payload).unwrap();
    f.ty = node;
    for parameter in &mut f.function_mut().values {
        parameter.ty = EntityType::Value(node);
    }
    let [a, b] = [f.owner(0), f.owner(1)];
    let payload_place = place(
        f.append(
            f.entry,
            Operation::HeapPayloadPlace { owner: a },
            vec![EntityType::Place(payload)],
        )[0],
    );
    let field = place(
        f.append(
            f.entry,
            Operation::FieldPlace {
                base: payload_place,
                field: 0,
            },
            vec![EntityType::Place(node)],
        )[0],
    );
    let field_loan = loan(
        f.append(
            f.entry,
            Operation::BorrowBegin {
                place: field,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: node,
            }],
        )[0],
    );
    f.replace(f.entry, a, field_loan, b);
    f.finish(f.entry);
    assert!(f.errors().iter().any(|e| matches!(e.kind, VerifyErrorKind::OperationContract { reason } if reason.contains("exact owned root"))));
}

#[test]
fn root_replace_cfg_join_preserves_pairing_instead_of_only_origin_sets() {
    for crossed in [false, true] {
        let mut f = Fixture::new(true);
        let [a, b, c] = [f.owner(0), f.owner(1), f.owner(2)];
        let (_, la) = f.root(a, LoanKind::Exclusive);
        let (_, lb) = f.root(b, LoanKind::Exclusive);
        let ty = f.ty;
        let boolean = f
            .program
            .module_mut(f.module)
            .unwrap()
            .intern_type(SsaTypeKind::Boolean);
        let condition = value(
            f.append(
                f.entry,
                Operation::Constant(crate::ssa::model::ScalarConstant::Boolean(true)),
                vec![EntityType::Value(boolean)],
            )[0],
        );
        let branch_types = vec![
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: ty
            };
            2
        ];
        let origin = f.origin.clone();
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
                    EntityType::Value(ty),
                    EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target: ty,
                    },
                ],
                origin.clone(),
            )
            .unwrap();
        let entry = f.entry;
        f.function_mut()
            .set_terminator(
                entry,
                TerminatorKind::Conditional {
                    condition,
                    when_true: Edge {
                        target: left,
                        arguments: vec![EntityId::Loan(la), EntityId::Loan(lb)],
                    },
                    when_false: Edge {
                        target: right,
                        arguments: vec![EntityId::Loan(lb), EntityId::Loan(la)],
                    },
                },
                origin,
            )
            .unwrap();
        for (block, selected, other) in [(left, a, b), (right, b, a)] {
            let args = f.function().block(block).unwrap().parameters.clone();
            f.append(
                block,
                Operation::BorrowEnd {
                    loan: loan(args[1]),
                },
                vec![],
            );
            f.branch(
                block,
                Edge {
                    target: join,
                    arguments: vec![
                        EntityId::Value(if crossed { other } else { selected }),
                        args[0],
                    ],
                },
            );
        }
        let args = f.function().block(join).unwrap().parameters.clone();
        f.replace(join, value(args[0]), loan(args[1]), c);
        f.finish(join);
        if crossed {
            assert!(f.errors().iter().any(|e| matches!(e.kind, VerifyErrorKind::OperationContract { reason } if reason.contains("exact owned root"))));
        } else {
            assert_eq!(verify_program(&f.program), Ok(()));
        }
    }
}

#[test]
fn root_replace_rejects_reusing_either_consumed_move_only_input() {
    for use_replacement in [false, true] {
        let mut f = Fixture::new(false);
        let [a, b] = [f.owner(0), f.owner(1)];
        let (_, l) = f.root(a, LoanKind::Exclusive);
        f.replace(f.entry, a, l, b);
        let consumed = if use_replacement { b } else { a };
        f.append(f.entry, Operation::Drop { owner: consumed }, vec![]);
        f.finish(f.entry);
        assert!(f.errors().iter().any(
            |e| matches!(e.kind, VerifyErrorKind::ValueUnavailable { value } if value == consumed)
        ));
    }
}

#[test]
fn root_exchange_rejects_using_ended_loans_after_commit() {
    for swap in [false, true] {
        let mut f = Fixture::new(false);
        let [a, b] = [f.owner(0), f.owner(1)];
        let (_, la) = f.root(a, LoanKind::Exclusive);
        let loans = if swap {
            let (_, lb) = f.root(b, LoanKind::Exclusive);
            f.append(
                f.entry,
                Operation::RootSwap {
                    owners: [a, b],
                    loans: [la, lb],
                },
                vec![EntityType::Value(f.ty); 2],
            );
            vec![la, lb]
        } else {
            f.replace(f.entry, a, la, b);
            vec![la]
        };
        for loan in &loans {
            f.append(f.entry, Operation::BorrowEnd { loan: *loan }, vec![]);
        }
        f.finish(f.entry);
        let errors = f.errors();
        for expected in loans {
            assert!(errors.iter().any(
                |e| matches!(e.kind, VerifyErrorKind::LoanInactive { loan } if loan == expected)
            ));
        }
    }
}

#[test]
fn root_replace_rejects_exclusive_inout_abi_loan_as_owned_root_proof() {
    let mut f = Fixture::new(false);
    f.finish(f.entry);
    let origin = f.origin.clone();
    let ty = f.ty;
    let module = f.program.module_mut(f.module).unwrap();
    let function = module
        .add_function("inout_callee", vec![], origin.clone())
        .unwrap();
    let entry = module
        .function_mut(function)
        .unwrap()
        .add_block(
            vec![
                EntityType::Value(ty),
                EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: ty,
                },
                EntityType::Value(ty),
            ],
            origin,
        )
        .unwrap();
    f.function = function;
    f.entry = entry;
    let args = f.function().block(entry).unwrap().parameters.clone();
    f.replace(entry, value(args[0]), loan(args[1]), value(args[2]));
    f.finish(entry);
    assert!(f.errors().iter().any(|e| matches!(e.kind, VerifyErrorKind::OperationContract { reason } if reason.contains("exact owned root"))));
}

#[test]
fn root_swap_rejects_wrong_result_arity_and_loan_target() {
    for wrong_target in [false, true] {
        let mut f = Fixture::new(false);
        let [a, b] = [f.owner(0), f.owner(1)];
        let (_, la) = f.root(a, LoanKind::Exclusive);
        let (_, lb) = f.root(b, LoanKind::Exclusive);
        let boolean = f
            .program
            .module_mut(f.module)
            .unwrap()
            .intern_type(SsaTypeKind::Boolean);
        if wrong_target {
            f.function_mut().loans[lb.index()].ty = EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: boolean,
            };
        }
        f.append(
            f.entry,
            Operation::RootSwap {
                owners: [a, b],
                loans: [la, lb],
            },
            vec![EntityType::Value(f.ty); if wrong_target { 2 } else { 1 }],
        );
        f.finish(f.entry);
        assert!(
            f.errors()
                .iter()
                .any(|e| matches!(e.kind, VerifyErrorKind::OperationContract { .. }))
        );
    }
}

#[test]
fn root_exchange_accepts_local_unit_values_without_return_payload() {
    let mut f = Fixture::new(true);
    let unit = f
        .program
        .module_mut(f.module)
        .unwrap()
        .intern_type(SsaTypeKind::Unit);
    f.ty = unit;
    let mut units = Vec::new();
    for _ in 0..3 {
        units.push(value(
            f.append(
                f.entry,
                Operation::Constant(crate::ssa::model::ScalarConstant::Unit),
                vec![EntityType::Value(unit)],
            )[0],
        ));
    }
    let (_, replace_loan) = f.root(units[0], LoanKind::Exclusive);
    let replaced = f.replace(f.entry, units[0], replace_loan, units[1]);
    let (_, swap_a) = f.root(value(replaced[0]), LoanKind::Exclusive);
    let (_, swap_b) = f.root(units[2], LoanKind::Exclusive);
    f.append(
        f.entry,
        Operation::RootSwap {
            owners: [value(replaced[0]), units[2]],
            loans: [swap_a, swap_b],
        },
        vec![EntityType::Value(unit); 2],
    );
    let origin = f.origin.clone();
    let entry = f.entry;
    f.function_mut()
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
        .unwrap();
    assert_eq!(verify_program(&f.program), Ok(()));
}
