//! Complete ordinary counter caller closure from real source; no generated code execution.
use super::*;
use crate::ssa::{
    model::{CheckedArithmeticOperator, EntityId, TerminatorKind},
    refine::arm_loop_receipt,
    unit_lower, unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn counter_full_caller_preserves_checked_update_and_original_roots() {
    for signature in [
        "source:String,rounds:Int,choose:Boolean",
        "source:String,choose:Boolean,rounds:Int",
        "rounds:Int,source:String,choose:Boolean",
        "rounds:Int,choose:Boolean,source:String",
        "choose:Boolean,source:String,rounds:Int",
        "choose:Boolean,rounds:Int,source:String",
    ] {
        check(signature, true);
    }
}
#[test]
fn counter_full_caller_keeps_unproved_modes_and_extra_parameters_outside() {
    for signature in [
        "source:String,rounds:Int,choose:Boolean,extra:Boolean",
        "source:String,own rounds:Int,choose:Boolean",
        "source:String,rounds:Int,own choose:Boolean",
    ] {
        check(signature, false);
    }
}
fn check(signature: &str, certified: bool) {
    for reverse in [false, true] {
        let mut sources = SourceMap::new();
        let (p, provider) = unit_lower_test_support::parsed(
            &mut sources,
            "counter-provider.ko",
            "fun view(source:String):borrow String from source=source\nfun next(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}",
        );
        let text = format!(
            "fun run({signature}):Unit {{var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=next(source);other(b);consume(b)}};remaining=remaining-1}}}}"
        );
        let (c, caller) = unit_lower_test_support::parsed(&mut sources, "counter-caller.ko", &text);
        let mut inputs = [
            SourceUnitInput::new("root", "counter-provider.ko", p, &provider),
            SourceUnitInput::new("root", "counter-caller.ko", c, &caller),
        ];
        if reverse {
            inputs.reverse();
        }
        let (environment, types) = standard_environments();
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let facts = owned.ownership().no_carry_counter_branches();
        assert_eq!(facts.len(), 1);
        assert!(owned.ownership().no_carry_branches().is_empty());
        assert!(owned.ownership().no_carry_break_branches().is_empty());
        let fact = &facts[0];
        assert!(owned.ownership().owns_no_carry_counter_branch(fact));
        assert!(fact.same_identity(&fact.clone()));
        assert_ne!(fact.counter(), fact.rounds());
        assert_eq!(
            sources
                .slice(
                    caller
                        .ast()
                        .expressions()
                        .get(fact.initializer().expression())
                        .unwrap()
                        .span()
                )
                .unwrap(),
            "rounds"
        );
        assert_eq!(
            sources
                .slice(
                    caller
                        .ast()
                        .expressions()
                        .get(fact.condition().expression())
                        .unwrap()
                        .span()
                )
                .unwrap(),
            "remaining>0"
        );
        assert_eq!(
            sources
                .slice(
                    caller
                        .ast()
                        .expressions()
                        .get(fact.decrement().expression())
                        .unwrap()
                        .span()
                )
                .unwrap(),
            "remaining-1"
        );
        let declaration = |name| {
            names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == name)
                .unwrap()
                .id()
        };
        for _ in 0..2 {
            let ((result, audits), receipts, trace) = arm_loop_receipt::observe(|| {
                observe(|| {
                    unit_lower::lower_scalar_unit_with_entry(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        &owned,
                        declaration("run"),
                    )
                })
            });
            let (program, entry) = result.expect("original real counter producer");
            crate::ssa::verify::verify_program(&program).unwrap();
            assert_eq!(receipts.len(), 1);
            assert_eq!(trace.recorded, 1);
            assert_eq!(trace.before_verifier, 1);
            assert_eq!(trace.pending_after, 0);
            assert!(!trace.rolled_back);
            let function = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.id == entry)
                .unwrap();
            receipts[0].validate(function).unwrap();
            let calls: Vec<_> = function
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                .collect();
            assert_eq!(calls.len(), 2);
            assert_ne!(calls[0].results, calls[1].results);
            for call in calls {
                let result = call.results[0];
                assert_eq!(function.instructions.iter().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==result)).count(),1);
                for block in &function.blocks {
                    assert!(!block.parameters.contains(&result));
                    assert!(
                        !block
                            .terminator
                            .as_ref()
                            .unwrap()
                            .kind
                            .entities()
                            .contains(&result)
                    );
                }
            }
            assert_eq!(
                function
                    .instructions
                    .iter()
                    .filter(|i| matches!(
                        i.operation,
                        Operation::CheckedArithmetic {
                            operator: CheckedArithmeticOperator::Subtract,
                            ..
                        }
                    ))
                    .count(),
                1
            );
            assert_eq!(
                function
                    .blocks
                    .iter()
                    .filter(|b| matches!(
                        b.terminator.as_ref().unwrap().kind,
                        TerminatorKind::Abort
                    ))
                    .count(),
                1
            );
            assert_eq!(function.blocks.iter().filter(|b|matches!(&b.terminator.as_ref().unwrap().kind,TerminatorKind::Return{values} if values.is_empty())).count(),1);
            assert_eq!(audits.len(), 1);
            for name in ["view", "next"] {
                assert_eq!(
                    audits[0]
                        .borrow_edges
                        .contains(&(declaration("run"), declaration(name))),
                    certified
                );
            }
            if !certified {
                assert!(
                    audits[0]
                        .unsupported
                        .iter()
                        .any(|g| g == "bounded-counter arm-loop full-caller evidence gap")
                );
                continue;
            }
            assert!(
                !audits[0]
                    .unsupported
                    .iter()
                    .any(|g| g.starts_with("bounded-counter arm-loop")
                        || g.starts_with("unclassified operation: BorrowCall"))
            );
            assert_eq!(function.blocks.len(), 9);
            assert_eq!(function.instructions.len(), 13);
            let updates: Vec<_> = function
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::CheckedArithmetic { .. }))
                .collect();
            let [update] = updates.as_slice() else {
                panic!("one original checked update");
            };
            assert_eq!(
                update.origin,
                crate::ssa::model::Origin::Source(
                    caller
                        .ast()
                        .expressions()
                        .get(fact.decrement().expression())
                        .unwrap()
                        .span()
                )
            );
            let [EntityId::Value(result), EntityId::Value(failed)] = update.results.as_slice()
            else {
                panic!("int result and failed bool");
            };
            assert_ne!(result, failed);
            let block = function.block(update.block).unwrap();
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = &block.terminator.as_ref().unwrap().kind
            else {
                panic!("checked branch");
            };
            assert_eq!(condition, failed);
            let abort = function.block(when_true.target).unwrap();
            let success = function.block(when_false.target).unwrap();
            assert!(abort.instructions.is_empty());
            assert!(success.instructions.is_empty());
            assert!(matches!(
                abort.terminator.as_ref().unwrap().kind,
                TerminatorKind::Abort
            ));
            let TerminatorKind::Branch(back) = &success.terminator.as_ref().unwrap().kind else {
                panic!("actual successful update backedge");
            };
            let entry_block = function.block(function.entry_block().unwrap()).unwrap();
            let TerminatorKind::Branch(initial) = &entry_block.terminator.as_ref().unwrap().kind
            else {
                panic!("initial counter edge");
            };
            assert_eq!(back.target, initial.target);
            assert_eq!(block.instructions.last(), Some(&update.id));
            let one = function.instruction(block.instructions[0]).unwrap();
            assert_eq!(
                one.operation,
                Operation::Constant(crate::ssa::model::ScalarConstant::Integer(1))
            );
            assert!(one.results.iter().all(|r| matches!(r, EntityId::Value(_))));
        }
    }
}
