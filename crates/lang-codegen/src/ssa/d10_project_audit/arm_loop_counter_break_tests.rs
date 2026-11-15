//! Complete bounded counter/conditional-break model evidence from ordinary source only.
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
fn counter_break_complete_caller_preserves_distinct_stop_and_checked_paths() {
    let parameters = [
        "source:String",
        "rounds:Int",
        "choose:Boolean",
        "stop:Boolean",
    ];
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    if [a, b, c, d].iter().copied().collect::<BTreeSet<_>>().len() != 4 {
                        continue;
                    }
                    check(
                        &[parameters[a], parameters[b], parameters[c], parameters[d]].join(","),
                        true,
                    );
                }
            }
        }
    }
}
#[test]
fn counter_break_complete_caller_keeps_unproved_abi_modes_outside() {
    for signature in [
        "source:String,rounds:Int,choose:Boolean,own stop:Boolean",
        "source:String,rounds:Int,choose:Boolean,stop:Boolean,extra:Boolean",
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
            "fun run({signature}):Unit {{var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=next(source);other(b);consume(b)}};if(stop){{break}};remaining=remaining-1}}}}"
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
        let facts = owned.ownership().no_carry_counter_break_branches();
        assert_eq!(facts.len(), 1);
        let wrapper = &facts[0];
        assert!(
            owned
                .ownership()
                .owns_no_carry_counter_break_branch(wrapper)
        );
        assert!(wrapper.same_identity(&wrapper.clone()));
        assert!(owned.ownership().no_carry_counter_branches().is_empty());
        assert!(owned.ownership().no_carry_break_branches().is_empty());
        let fact = wrapper.counter();
        assert_eq!(
            wrapper.target_loop(),
            fact.pair().arms()[0].loop_statement()
        );
        assert_eq!(
            sources
                .slice(
                    caller
                        .ast()
                        .expressions()
                        .get(wrapper.break_condition().expression())
                        .unwrap()
                        .span()
                )
                .unwrap(),
            "stop"
        );
        assert_eq!(
            sources
                .slice(
                    caller
                        .ast()
                        .expressions()
                        .get(wrapper.break_expression().expression())
                        .unwrap()
                        .span()
                )
                .unwrap(),
            "break"
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
                        .any(|s| s == "counter-break arm-loop full-caller evidence gap")
                );
                continue;
            }
            assert!(
                !audits[0]
                    .unsupported
                    .iter()
                    .any(|s| s.starts_with("counter-break arm-loop")
                        || s.starts_with("unclassified operation: BorrowCall"))
            );
            assert_eq!(function.blocks.len(), 12);
            assert_eq!(function.instructions.len(), 14);
            let entry_block = function.block(function.entry_block().unwrap()).unwrap();
            let mut roles = entry_block
                .parameters
                .iter()
                .enumerate()
                .map(|(i, e)| (*e, i))
                .collect::<BTreeMap<_, _>>();
            loop {
                let old = roles.clone();
                for block in &function.blocks {
                    let edges = match &block.terminator.as_ref().unwrap().kind {
                        TerminatorKind::Branch(edge) => vec![edge],
                        TerminatorKind::Conditional {
                            when_true,
                            when_false,
                            ..
                        } => vec![when_true, when_false],
                        TerminatorKind::Return { .. } | TerminatorKind::Abort => vec![],
                        _ => panic!("bounded counter terminator"),
                    };
                    for edge in edges {
                        let target = function.block(edge.target).unwrap();
                        for (arg, param) in edge.arguments.iter().zip(&target.parameters) {
                            if let Some(role) = old.get(arg)
                                && let Some(previous) = roles.insert(*param, *role)
                            {
                                assert_eq!(previous, *role);
                            }
                        }
                    }
                }
                if roles == old {
                    break;
                }
            }
            let stop_role = signature
                .split(',')
                .position(|p| p == "stop:Boolean")
                .unwrap();
            let stop_span = caller
                .ast()
                .expressions()
                .get(wrapper.break_condition().expression())
                .unwrap()
                .span();
            let reads = function
                .instructions
                .iter()
                .filter(|i| i.origin == crate::ssa::model::Origin::Source(stop_span))
                .collect::<Vec<_>>();
            let [read] = reads.as_slice() else {
                panic!("one original stop Read")
            };
            let Operation::Read {
                source: crate::ssa::model::PlaceAccess::Loan(loan),
            } = read.operation
            else {
                panic!("borrow stop")
            };
            assert_eq!(roles.get(&EntityId::Loan(loan)), Some(&stop_role));
            let gate = function.block(read.block).unwrap();
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = &gate.terminator.as_ref().unwrap().kind
            else {
                panic!("stop gate")
            };
            assert_eq!(read.results, [EntityId::Value(*condition)]);
            let yes = function.block(when_true.target).unwrap();
            assert!(yes.instructions.is_empty());
            let TerminatorKind::Branch(exit_edge) = &yes.terminator.as_ref().unwrap().kind else {
                panic!("break exit")
            };
            assert_eq!(
                yes.terminator.as_ref().unwrap().origin,
                crate::ssa::model::Origin::Source(
                    caller
                        .ast()
                        .expressions()
                        .get(wrapper.break_expression().expression())
                        .unwrap()
                        .span()
                )
            );
            let exit = function.block(exit_edge.target).unwrap();
            assert!(exit.parameters.is_empty());
            assert!(
                matches!(&exit.terminator.as_ref().unwrap().kind,TerminatorKind::Return{values} if values.is_empty())
            );
            let continued = function.block(when_false.target).unwrap();
            assert_eq!(continued.instructions.len(), 2);
            assert!(matches!(
                function
                    .instruction(continued.instructions[1])
                    .unwrap()
                    .operation,
                Operation::CheckedArithmetic {
                    operator: CheckedArithmeticOperator::Subtract,
                    ..
                }
            ));
        }
    }
}
