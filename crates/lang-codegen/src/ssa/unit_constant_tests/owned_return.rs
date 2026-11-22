use super::{
    Operation, ScalarConstant, SourceMap, SourceUnitInput,
    check_compilation_unit_constant_ownership, check_compilation_unit_types,
    index_compilation_unit, lower_constant_unit_with_entry, parsed, resolve_compilation_unit_names,
    standard_environments,
};
use crate::ssa::model::{EntityId, EntityType, LoanKind, PlaceAccess, SsaTypeKind, TerminatorKind};

#[test]
fn original_early_owned_argument_constant_and_literal_cleanup_follow_real_edges() {
    for (text, token) in [
        (
            include_str!(
                "../../../../lang-frontend/tests/ownership_constants/fixtures/owned-constant-return.ko"
            ),
            "TEXT",
        ),
        (
            include_str!(
                "../../../../lang-frontend/tests/ownership_constants/fixtures/owned-literal-return.ko"
            ),
            "\"hi\"",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "constants.ko", text);
        let (ne, te) = standard_environments();
        for rooted in [false, true] {
            let inputs = [if rooted {
                SourceUnitInput::new("root", "constants.ko", source, &file)
            } else {
                SourceUnitInput::memory_document(source, &file)
            }];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &te)
                .unwrap()
                .validate_constants()
                .unwrap();
            let owned =
                check_compilation_unit_constant_ownership(&sources, &inputs, &names, &te, &typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            let id = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "use")
                .unwrap()
                .id();
            let (program, entry) =
                lower_constant_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, id)
                    .expect("the two original inputs reach real constant Unit SSA");
            crate::ssa::verify::verify_program(&program).unwrap();
            let module = program.module(entry.module()).unwrap();
            let caller = module.function(entry).unwrap();
            let [head, early, normal] = caller.blocks.as_slice() else {
                panic!("one real branch and two real returns")
            };
            let [EntityId::Loan(flag)] = head.parameters.as_slice() else {
                panic!("borrowed flag ABI")
            };
            let instructions = |block: &crate::ssa::model::Block| {
                block
                    .instructions
                    .iter()
                    .map(|id| caller.instruction(*id).unwrap())
                    .collect::<Vec<_>>()
            };
            let head_steps = instructions(head);
            let [string, read] = head_steps.as_slice() else {
                panic!("materialize first, then evaluate flag")
            };
            assert!(
                matches!(&string.operation, Operation::StringLiteral { bytes, .. } if bytes == b"hi")
            );
            assert_eq!(sources.slice(string.origin.span()), Ok(token));
            let [EntityId::Value(owner)] = string.results.as_slice() else {
                panic!("one actual String owner")
            };
            let EntityType::Value(owner_ty) = caller.entity(EntityId::Value(*owner)).unwrap().ty
            else {
                panic!("owner type")
            };
            assert_eq!(module.type_kind(owner_ty), Some(&SsaTypeKind::StringOwner));
            assert_eq!(
                read.operation,
                Operation::Read {
                    source: PlaceAccess::Loan(*flag)
                }
            );
            let [EntityId::Value(condition)] = read.results.as_slice() else {
                panic!("actual read of flag")
            };
            let TerminatorKind::Conditional {
                condition: checked,
                when_true,
                when_false,
            } = &head.terminator.as_ref().unwrap().kind
            else {
                panic!("branch on the real runtime flag")
            };
            assert_eq!(checked, condition);
            assert_eq!(when_true.target, early.id);
            assert_eq!(when_false.target, normal.id);
            for (edge, target) in [(when_true, early), (when_false, normal)] {
                // The literal lane may explicitly carry the already-borrowed flag;
                // neither lane may carry any different owned value or extra loan.
                match (edge.arguments.as_slice(), target.parameters.as_slice()) {
                    ([EntityId::Value(value)], [EntityId::Value(_)]) => assert_eq!(value, owner),
                    (
                        [EntityId::Value(value), EntityId::Loan(carried)],
                        [EntityId::Value(_), EntityId::Loan(param)],
                    ) => {
                        assert_eq!(value, owner);
                        assert_eq!(carried, flag);
                        assert_eq!(
                            caller.entity(EntityId::Loan(*param)).unwrap().ty,
                            caller.entity(EntityId::Loan(*flag)).unwrap().ty
                        );
                    }
                    rest => panic!("unexpected original owner/flag edge: {rest:?}"),
                }
            }
            let Some(EntityId::Value(early_owner)) = early.parameters.first() else {
                panic!("early edge owner")
            };
            let early_steps = instructions(early);
            let [early_drop] = early_steps.as_slice() else {
                panic!("no call or second operand on early edge")
            };
            assert_eq!(
                early_drop.operation,
                Operation::Drop {
                    owner: *early_owner
                }
            );
            let Some(EntityId::Value(passed)) = normal.parameters.first() else {
                panic!("normal edge owner")
            };
            assert_ne!(early_owner, passed);
            for edge in [early, normal] {
                assert!(
                    matches!(&edge.terminator.as_ref().unwrap().kind, TerminatorKind::Return { values } if values.is_empty())
                );
            }
            let normal_steps = instructions(normal);
            let [zero, place, begin, call, end] = normal_steps.as_slice() else {
                panic!("only normal path evaluates and delivers both operands")
            };
            assert_eq!(
                zero.operation,
                Operation::Constant(ScalarConstant::Integer(0))
            );
            let [EntityId::Value(number)] = zero.results.as_slice() else {
                panic!("actual scalar")
            };
            assert_eq!(place.operation, Operation::RootPlace { owner: *number });
            let [EntityId::Place(number_place)] = place.results.as_slice() else {
                panic!("scalar temporary slot")
            };
            assert_eq!(
                begin.operation,
                Operation::BorrowBegin {
                    place: *number_place,
                    kind: LoanKind::Shared
                }
            );
            let [EntityId::Loan(number_loan)] = begin.results.as_slice() else {
                panic!("scalar borrow ABI")
            };
            let Operation::DirectCall {
                callee: take,
                arguments,
                receiver: None,
            } = &call.operation
            else {
                panic!("real owning callee")
            };
            assert_eq!(
                arguments.as_slice(),
                [EntityId::Value(*passed), EntityId::Loan(*number_loan)]
            );
            assert_eq!(end.operation, Operation::BorrowEnd { loan: *number_loan });
            let take = module.function(*take).unwrap();
            let [take_entry] = take.blocks.as_slice() else {
                panic!("empty owning callee")
            };
            let [EntityId::Value(received), EntityId::Loan(_)] = take_entry.parameters.as_slice()
            else {
                panic!("actual owning parameter")
            };
            let [drop] = take.instructions.as_slice() else {
                panic!("callee retires one precise owner")
            };
            assert_eq!(drop.operation, Operation::Drop { owner: *received });
            let llvm = crate::llvm::render_verified_program(&program).unwrap();
            let entry = format!("f{}.{}", caller.id.index(), caller.name);
            let target = format!("f{}.{}", take.id.index(), take.name);
            run_both_flags(&llvm, &entry, &target, owner_ty.index());
        }
    }
}

fn run_both_flags(llvm: &str, entry: &str, take: &str, owner_ty: usize) {
    let marker_take = format!("call void @{take}(");
    let marker_drop = format!("call void @koven.drop.t{owner_ty}(");
    let mut observed = String::new();
    let (mut takes, mut drops) = (0, 0);
    for line in llvm.lines() {
        if line.trim().starts_with(&marker_take) {
            takes += 1;
            observed.push_str("  call void @koven_test_owned_event(i32 2)\n");
        }
        observed.push_str(line);
        observed.push('\n');
        if line.trim().starts_with(&marker_drop) {
            drops += 1;
            observed.push_str("  call void @koven_test_owned_event(i32 3)\n");
        }
    }
    assert_eq!((takes, drops), (1, 2));
    assert!(llvm.contains(&format!("define internal void @{entry}(ptr %l0)")));
    // This separate ABI driver selects the two original CFG branches without changing
    // either source. Observe the actual calls, not a replacement owner or heap allocation.
    observed.push_str(&format!(
        r#"
declare void @koven_test_owned_event(i32)
define i32 @main() {{
  %flag = alloca i1
  call void @koven_test_owned_event(i32 0)
  store i1 true, ptr %flag
  call void @{entry}(ptr %flag)
  call void @koven_test_owned_event(i32 1)
  store i1 false, ptr %flag
  call void @{entry}(ptr %flag)
  ret i32 0
}}
"#
    ));
    crate::ssa::unit_field_replace_tests::link_and_run(
        &observed,
        r#"
#include <assert.h>
#include <stdio.h>
void koven_test_owned_event(int event) {
    static const char *events[] = {"early", "normal", "take", "drop"};
    assert(event >= 0 && event < 4);
    puts(events[event]);
}
"#,
        b"early\ndrop\nnormal\ntake\ndrop\n",
    );
}
