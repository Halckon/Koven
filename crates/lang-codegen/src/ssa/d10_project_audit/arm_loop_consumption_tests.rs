//! Original ordinary arm-loop facts and final publication, never synthetic receipt mutation.
use super::*;
use crate::ssa::{model::EntityId, refine::arm_loop_receipt, unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn ordinary_arm_loop_certifies_both_edges_with_complete_caller() {
    for signature in [
        "source:String,flag:Boolean,choose:Boolean",
        "source:String,choose:Boolean,flag:Boolean",
        "flag:Boolean,source:String,choose:Boolean",
        "flag:Boolean,choose:Boolean,source:String",
        "choose:Boolean,source:String,flag:Boolean",
        "choose:Boolean,flag:Boolean,source:String",
    ] {
        for swapped in [false, true] {
            for reverse in [false, true] {
                let mut sources = SourceMap::new();
                let (p, provider) = unit_lower_test_support::parsed(
                    &mut sources,
                    "arm-audit-provider.ko",
                    "fun view(source:String):borrow String from source=source\nfun next(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}",
                );
                let (outer, inner) = if swapped {
                    ("choose", "flag")
                } else {
                    ("flag", "choose")
                };
                let text = format!(
                    "fun run({signature}):Unit {{while({outer}){{if({inner}){{borrow val a=view(source);consume(a)}}else{{borrow val b=next(source);other(b);consume(b)}}}}}}"
                );
                let (c, caller) =
                    unit_lower_test_support::parsed(&mut sources, "arm-audit-caller.ko", &text);
                let mut inputs = [
                    SourceUnitInput::new("root", "arm-audit-provider.ko", p, &provider),
                    SourceUnitInput::new("root", "arm-audit-caller.ko", c, &caller),
                ];
                if reverse {
                    inputs.reverse();
                }
                let (environment, types) = standard_environments();
                let (names, typed, owned) =
                    unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
                let facts = owned.ownership().no_carry_branches();
                assert_eq!(facts.len(), 1);
                let pair = &facts[0];
                assert!(owned.ownership().owns_no_carry_branch(pair));
                assert_eq!(pair.arms()[0].consumers().len(), 1);
                assert_eq!(pair.arms()[1].consumers().len(), 2);
                assert_ne!(pair.arms()[0].binding(), pair.arms()[1].binding());
                assert_eq!(pair.arms()[0].source(), pair.arms()[1].source());
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
                    let (program, entry) = result.expect("original ordinary arm route");
                    crate::ssa::verify::verify_program(&program).unwrap();
                    assert_eq!(receipts.len(), 1);
                    assert_eq!(trace.recorded, 1);
                    assert_eq!(trace.before_verifier, 1);
                    assert!(!trace.rolled_back);
                    assert_eq!(trace.pending_after, 0);
                    let function = program
                        .modules
                        .iter()
                        .flat_map(|m| &m.functions)
                        .find(|f| f.id == entry)
                        .unwrap();
                    assert_eq!(receipts[0].caller, entry);
                    receipts[0].validate(function).unwrap();
                    let calls: Vec<_> = function
                        .instructions
                        .iter()
                        .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                        .collect();
                    assert_eq!(calls.len(), 2);
                    let results: Vec<_> = calls
                        .iter()
                        .map(|i| {
                            assert_eq!(i.results.len(), 1);
                            i.results[0]
                        })
                        .collect();
                    assert_ne!(results[0], results[1]);
                    for result in results {
                        assert_eq!(function.instructions.iter().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==result)).count(),1);
                    }
                    assert_eq!(audits.len(), 1);
                    for name in ["view", "next"] {
                        assert!(
                            audits[0]
                                .borrow_edges
                                .contains(&(declaration("run"), declaration(name)))
                        );
                    }
                    assert!(
                        !audits[0]
                            .unsupported
                            .iter()
                            .any(|g| g.starts_with("ordinary arm-loop")
                                || g.starts_with("unclassified operation: BorrowCall"))
                    );
                }
            }
        }
    }
}

#[test]
fn ordinary_arm_loop_domain_boundaries_keep_both_edges_uncertified() {
    let arm = "if(choose){borrow val a=view(source);consume(a)}else{borrow val b=next(source);other(b);consume(b)}";
    let cases = [
        format!(
            "fun run(source:String,flag:Boolean,choose:Boolean):Unit {{consume(source);other(source);while(flag){{{arm}}}}}"
        ),
        format!(
            "fun run(source:String,flag:Boolean,choose:Boolean):Unit {{while(flag){{{arm}}};consume(source)}}"
        ),
        format!(
            "fun run(source:String,flag:Boolean,choose:Boolean,unused:Boolean):Unit {{while(flag){{{arm}}}}}"
        ),
        format!(
            "fun run(source:String,flag:Boolean,choose:Boolean):Unit {{while(flag){{{}}}}}",
            arm.replace("if(choose)", "if(flag)")
        ),
        format!(
            "fun run(source:String,flag:Boolean,choose:Boolean):Unit {{while(flag){{{}}}}}",
            arm.replace("consume(a)", "consume(a);other(a);consume(a)")
        ),
    ];
    for (case, caller) in cases.iter().enumerate() {
        let mut sources = SourceMap::new();
        let (p, provider) = unit_lower_test_support::parsed(
            &mut sources,
            "arm-boundary-provider.ko",
            "fun view(source:String):borrow String from source=source\nfun next(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}",
        );
        let (c, caller) =
            unit_lower_test_support::parsed(&mut sources, "arm-boundary-caller.ko", caller);
        let inputs = [
            SourceUnitInput::new("root", "arm-boundary-provider.ko", p, &provider),
            SourceUnitInput::new("root", "arm-boundary-caller.ko", c, &caller),
        ];
        let (environment, types) = standard_environments();
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
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
            let ((result, audits), receipts, _) = arm_loop_receipt::observe(|| {
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
            if case == 1 {
                let error = result
                    .err()
                    .expect("original suffix arm route rejects before publication");
                assert_eq!(
                    error.kind,
                    crate::ssa::lowering_support::LoweringErrorKind::InvalidModel
                );
                let span = error.span.expect("caller span");
                assert_eq!(
                    (span.source_id(), span.start(), span.end()),
                    (c, 0, cases[case].len())
                );
                assert!(audits.is_empty());
                assert!(receipts.is_empty());
                continue;
            }
            let (program, _) = result.unwrap_or_else(|error| panic!("boundary {case}: {error:?}"));
            crate::ssa::verify::verify_program(&program).unwrap();
            assert_eq!(receipts.len(), 1, "boundary {case}");
            assert_eq!(audits.len(), 1);
            for name in ["view", "next"] {
                assert!(
                    !audits[0]
                        .borrow_edges
                        .contains(&(declaration("run"), declaration(name))),
                    "boundary {case}"
                );
            }
            assert!(
                audits[0]
                    .unsupported
                    .iter()
                    .any(|g| g == "ordinary arm-loop full-caller evidence gap"),
                "boundary {case}: {:?}",
                audits[0].unsupported
            );
        }
    }
}
