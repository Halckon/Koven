use super::{
    EntityId, EntityType, LoanKind, Operation, SourceMap, SourceUnitInput, SsaTypeKind,
    TerminatorKind, analyze, complete_compilation_unit_types, lower_scalar_unit_with_entry, parsed,
    standard_environments,
};
use crate::ssa::model::{Instruction, ScalarConstant};

#[test]
fn exact_nullable_copy_releases_aux_then_reads_source_then_releases_source() {
    let text = include_str!(
        "../../../../lang-frontend/tests/ownership_nested_borrow_arguments/fixtures/nullable-source-last-use.ko"
    );
    let path = "nested_borrow.ko";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, path, text);
    let (ne, te) = standard_environments();
    for rooted in [false, true] {
        let inputs = [if rooted {
            SourceUnitInput::new("root", path, source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        complete_compilation_unit_types(&sources, &inputs, &names, &te, typed.types()).unwrap();
        let id = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let (program, entry) =
            lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, id)
                .expect("exact last-use fixture reaches actual Unit SSA");
        crate::ssa::verify::verify_program(&program).unwrap();
        let module = program.module(entry.module()).unwrap();
        let caller = module.function(entry).unwrap();
        let [block] = caller.blocks.as_slice() else {
            panic!("linear original SSA")
        };
        let [
            seven,
            boxed_payload,
            boxed_alloc,
            nine,
            aux_payload,
            aux_alloc,
            boxed_place,
            boxed_begin,
            aux_place,
            aux_begin,
            call,
            end_aux,
            drop_aux,
            read,
            end_result,
            end_boxed,
            drop_boxed,
            wrap,
            wrapped_place,
            begin_wrapped,
            observe,
            end_wrapped,
        ] = caller.instructions.as_slice()
        else {
            panic!("exact two constructor, loan, snapshot and release chains")
        };
        let value = |inst: &Instruction| {
            let [EntityId::Value(value)] = inst.results.as_slice() else {
                panic!("actual value")
            };
            *value
        };
        let allocated =
            |constant: &Instruction, payload: &Instruction, allocation: &Instruction, n| {
                assert_eq!(
                    constant.operation,
                    Operation::Constant(ScalarConstant::Integer(n))
                );
                let Operation::AggregateConstruct { fields, .. } = &payload.operation else {
                    panic!("real Boxed payload")
                };
                assert_eq!(fields.as_slice(), [value(constant)]);
                let Operation::HeapAllocate {
                    owner,
                    payload: input,
                } = allocation.operation
                else {
                    panic!("real Boxed allocation")
                };
                assert_eq!(input, value(payload));
                (value(allocation), owner)
            };
        let (boxed, record_ty) = allocated(seven, boxed_payload, boxed_alloc, 7);
        let (aux, same_record_ty) = allocated(nine, aux_payload, aux_alloc, 9);
        assert_ne!(boxed, aux);
        assert_eq!(record_ty, same_record_ty);
        let loan = |inst: &Instruction| {
            let [EntityId::Loan(loan)] = inst.results.as_slice() else {
                panic!("actual loan")
            };
            *loan
        };
        let root_loan = |root: &Instruction, begin: &Instruction, owner| {
            assert_eq!(root.operation, Operation::RootPlace { owner });
            let [EntityId::Place(place)] = root.results.as_slice() else {
                panic!("actual owner place")
            };
            assert_eq!(
                begin.operation,
                Operation::BorrowBegin {
                    place: *place,
                    kind: LoanKind::Shared
                }
            );
            loan(begin)
        };
        let boxed_loan = root_loan(boxed_place, boxed_begin, boxed);
        let aux_loan = root_loan(aux_place, aux_begin, aux);
        let Operation::BorrowCall {
            callee,
            arguments,
            source: witness,
        } = &call.operation
        else {
            panic!("original borrow call")
        };
        assert_eq!(*witness, boxed_loan);
        assert_eq!(
            arguments.as_slice(),
            [EntityId::Loan(boxed_loan), EntityId::Loan(aux_loan)]
        );
        assert_eq!(sources.slice(call.origin.span()), Ok("view(boxed, aux)"));
        let callee = module.function(*callee).unwrap();
        let [callee_entry] = callee.blocks.as_slice() else {
            panic!("linear projection")
        };
        let [EntityId::Loan(lhs), EntityId::Loan(rhs)] = callee_entry.parameters.as_slice() else {
            panic!("two typed ABI parameters")
        };
        assert_ne!(lhs, rhs);
        for input in [*lhs, *rhs] {
            assert_eq!(
                callee.entity(EntityId::Loan(input)).unwrap().ty,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: record_ty
                }
            );
        }
        let [projected] = callee.instructions.as_slice() else {
            panic!("one real projection")
        };
        assert_eq!(
            projected.operation,
            Operation::SharedHeapFieldLoan {
                base: *lhs,
                field: 0
            }
        );
        let field = loan(projected);
        assert!(matches!(callee_entry.terminator.as_ref().unwrap().kind,
            TerminatorKind::BorrowReturn { loan } if loan == field));

        assert_eq!(end_aux.operation, Operation::BorrowEnd { loan: aux_loan });
        assert_eq!(drop_aux.operation, Operation::Drop { owner: aux });
        let result = loan(call);
        assert_eq!(
            read.operation,
            Operation::Read {
                source: crate::ssa::model::PlaceAccess::Loan(result)
            }
        );
        let integer = value(read);
        let EntityType::Value(integer_ty) = caller.entity(EntityId::Value(integer)).unwrap().ty
        else {
            panic!("copied scalar")
        };
        assert_eq!(
            module.type_kind(integer_ty),
            Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true
            })
        );
        assert_eq!(end_result.operation, Operation::BorrowEnd { loan: result });
        assert_eq!(
            end_boxed.operation,
            Operation::BorrowEnd { loan: boxed_loan }
        );
        assert_eq!(drop_boxed.operation, Operation::Drop { owner: boxed });
        let Operation::NullableWrap { nullable, owner } = wrap.operation else {
            panic!("the copied scalar is wrapped, not the original owner")
        };
        assert_eq!(owner, integer);
        let wrap_loan = root_loan(wrapped_place, begin_wrapped, value(wrap));
        let Operation::DirectCall {
            callee: observer,
            arguments,
            receiver: None,
        } = &observe.operation
        else {
            panic!("real outer observer")
        };
        assert_eq!(
            sources.slice(observe.origin.span()),
            Ok("observe(view(boxed, aux))")
        );
        assert_eq!(arguments.as_slice(), [EntityId::Loan(wrap_loan)]);
        assert_eq!(
            end_wrapped.operation,
            Operation::BorrowEnd { loan: wrap_loan }
        );
        assert!(matches!(&block.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return { values } if values.is_empty()));
        let observer = module.function(*observer).unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        super::super::unit_field_replace_tests::link_and_run(&llvm, "", b"");
        let symbol = format!("f{}.{}", observer.id.index(), observer.name);
        observe_original_nullable_and_releases(&llvm, &symbol, nullable.index());
    }
}

fn observe_original_nullable_and_releases(llvm: &str, symbol: &str, nullable: usize) {
    let prefix = format!("define internal void @{symbol}(ptr %l0) {{\nbb0:\n");
    assert_eq!(llvm.matches(&prefix).count(), 1);
    let ty = format!("%koven.nullable.t{nullable}");
    let watched = format!(
        "{prefix}  %test.slot = load {ty}, ptr %l0\n  %test.present = extractvalue {ty} %test.slot, 0\n  %test.tag = zext i1 %test.present to i32\n  %test.value = extractvalue {ty} %test.slot, 1\n  call void @koven_test_observe_nullable(i32 %test.tag, i32 %test.value)\n"
    );
    let observed = llvm
        .replace(&prefix, &watched)
        .replace("@malloc(", "@koven_test_box_alloc(")
        .replace("@free(", "@koven_test_box_free(")
        + "\ndeclare void @koven_test_observe_nullable(i32, i32)\n";
    super::super::unit_field_replace_tests::link_and_run(
        &observed,
        r#"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
static void *addresses[2];
static int allocated, released;
void *koven_test_box_alloc(size_t n) {
    assert(n == sizeof(int32_t) && allocated < 2);
    void *p = malloc(n);
    assert(p && (allocated == 0 || p != addresses[0]));
    addresses[allocated++] = p;
    return p;
}
void koven_test_box_free(void *p) {
    assert(allocated == 2 && released < 2);
    int expected = 1 - released;
    assert(p == addresses[expected]);
    assert(*(int32_t *)p == (expected == 1 ? 9 : 7));
    addresses[expected] = NULL;
    ++released;
    free(p);
}
void koven_test_observe_nullable(int32_t present, int32_t value) {
    assert(allocated == 2 && released == 2 && present == 1 && value == 7);
    puts("aux=9 source=7 copied=7");
}
"#,
        b"aux=9 source=7 copied=7\n",
    );
}
