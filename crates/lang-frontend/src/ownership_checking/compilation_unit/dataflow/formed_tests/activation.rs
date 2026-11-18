//! Ordinary source exercises the actual checkpoint, without injected failures.
use super::*;

#[test]
fn rejected_assignment_rolls_back_only_new_activation_witnesses() {
    let text = "class Holder(var target:Int) { inout fun set(own next:Int):Int {target=next;return next} }\nfun run(inout cell:Holder):Unit {cell.set(0);val fixed=0;fixed=cell.set(1)}";
    let fixture = Fixture::new(&[("activation.ko", text)]);
    let shared = fixture.shared();
    let parts = fixture.parts(&shared);
    with_checker_phase(&fixture, &parts, false, false, |checker| {
        assert_eq!(checker.diagnostics.len(), 1);
        assert_eq!(checker.diagnostics[0].code().to_string(), "L0134");
        assert_eq!(
            checker.receiver_activations.len(),
            1,
            "RHS activation rolled back, prior real transition retained"
        );
        let activation = &checker.receiver_activations[0];
        assert!(checker.receiver_facts.contains(activation));
        let span = checker
            .parsed
            .ast()
            .expressions()
            .get(activation.call().expression())
            .unwrap()
            .span();
        assert_eq!(fixture.sources.slice(span).unwrap(), "cell.set(0)");
    });
    with_checker_phase(&fixture, &parts, false, true, |checker| {
        assert_eq!(checker.diagnostics.len(), 1);
        assert!(
            checker.receiver_activations.is_empty(),
            "run does not retain raw witnesses after failure"
        );
    });
}

#[test]
fn conflicting_activation_has_no_private_success_witness() {
    let text = "class Holder(var target:Int) { inout fun update(other:Holder):Unit {} }\nfun run(inout cell:Holder):Unit {cell.update(cell)}";
    let fixture = Fixture::new(&[("conflict.ko", text)]);
    let shared = fixture.shared();
    with_checker_phase(&fixture, &fixture.parts(&shared), false, false, |checker| {
        assert!(
            checker
                .diagnostics
                .iter()
                .any(|d| d.code().to_string() == "L0135")
        );
        assert_eq!(
            checker.receiver_facts.len(),
            1,
            "actual reservation exists before failure"
        );
        assert!(checker.receiver_facts[0].activation_point().is_none());
        assert!(
            checker.receiver_activations.is_empty(),
            "failed transition never creates a witness"
        );
    });
}

#[test]
fn separate_source_failure_or_deferred_revokes_raw_activation_catalog() {
    let good = "class Holder(var target:Int) { inout fun set(own next:Int):Unit {target=next} }\nfun run(inout cell:Holder):Unit {cell.set(1)}";
    for (bad, deferred) in [
        ("fun bad():Unit {val fixed=0;fixed=1}", false),
        (
            "fun bad():Unit {var callback:()->Int={1};val old=replace(&callback,{2})}",
            true,
        ),
    ] {
        let fixture = Fixture::new(&[("good.ko", good), ("bad.ko", bad)]);
        let shared = fixture.shared();
        let raw = fixture.dataflow(&fixture.parts(&shared));
        if deferred {
            assert!(!raw.deferred.is_empty());
        } else {
            assert!(!raw.diagnostics.is_empty());
        }
        assert!(
            raw.loan_action_observations.is_empty(),
            "all raw source catalogs revoke atomically"
        );
    }
}

#[test]
fn receiver_activation_index_preserves_exact_unique_witness_gate() {
    let text = "class Holder(var target:Int) { inout fun set(own next:Int):Unit {target=next} }\nfun ordinary():Unit {}\nfun run(inout cell:Holder):Unit {cell.set(1);ordinary();cell.set(2)}";
    let fixture = Fixture::new(&[("activation_index.ko", text)]);
    let shared = fixture.shared();
    let parts = fixture.parts(&shared);
    for mode in 0..6 {
        with_checker_phase(&fixture, &parts, false, false, |checker| {
            assert!(checker.diagnostics.is_empty());
            assert_eq!(checker.receiver_activations.len(), 2);
            let first = checker.receiver_activations[0].clone();
            match mode {
                0 => {} // Both unique witnesses remain eligible.
                1 => {
                    checker.receiver_activations.remove(0);
                }
                2 => checker.receiver_activations.push(first.clone()),
                3 => {
                    checker.receiver_activations.push(first.clone());
                    checker.receiver_activations.push(first.clone());
                }
                4 | 5 => {
                    // Same call key, but not the actual activated receiver token.
                    let mismatched = UnitReceiverOwnershipFact::new(
                        first.call(),
                        first.source(),
                        first.target().clone(),
                        first.kind(),
                        first.receiver_type(),
                        first.begin_span(),
                        first.end_span(),
                        first.declaration_span(),
                    );
                    if mode == 4 {
                        checker.receiver_activations[0] = mismatched;
                    } else {
                        checker.receiver_activations.push(mismatched);
                    }
                }
                _ => unreachable!(),
            }
            let result = drop_planner::plan(checker).unwrap();
            let activations: Vec<_> = result
                .loan_action_observations
                .iter()
                .flat_map(|observation| observation.actions())
                .filter_map(|action| match action {
                    crate::ownership_checking::UnitLoanAction::ReceiverActivate(fact) => {
                        Some(fact.call())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                activations.len(),
                if mode == 0 { 2 } else { 1 },
                "mode {mode}"
            );
            assert_eq!(
                activations.contains(&first.call()),
                mode == 0,
                "mode {mode}"
            );
        });
    }
}
