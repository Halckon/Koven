//! Group preserves syntax identity while the actual range call owns the source loan.
use super::*;
use lang_frontend::ownership_checking::{
    RangeUseSite, UnitDropTarget, UnitIterationCleanupAction as Action,
    UnitIterationExitKind as Exit,
};

#[test]
fn grouped_iteration_preserves_actual_call_root_and_exit_cleanup() {
    for depth in [0, 1, 3] {
        for temporary in [false, true] {
            for body in ["println(item)", "break", "continue", "return"] {
                let backing = if temporary {
                    "listOf(\"temporary\")"
                } else {
                    "source"
                };
                let expression = format!(
                    "{}rangeView({backing},0,1){}",
                    "(".repeat(depth),
                    ")".repeat(depth)
                );
                let text = format!(
                    "fun run():Unit{{val source=listOf(\"kept\");for(item in {expression}){{{body}}};consume(source)}}"
                );
                let (sources, document, rooted) = checked(&text);
                for owned in [&document, &rooted] {
                    assert!(
                        owned.diagnostics().is_empty(),
                        "{text}: {:?}",
                        owned.diagnostics()
                    );
                    assert!(owned.deferred().is_empty());
                    assert!(
                        owned.clone().validate().is_ok(),
                        "sealed iteration identity: {text}"
                    );
                    let [fact] = owned.borrow_results().range_uses() else {
                        panic!("one actual range use: {text}")
                    };
                    assert_eq!(fact.site(), RangeUseSite::Iteration);
                    let [plan] = owned.iterations() else {
                        panic!("one provider")
                    };
                    assert_eq!(plan.source(), Some(fact.origin()));
                    assert_eq!(plan.descriptor().source() == fact.expression(), depth == 0);
                    let loan = owned
                        .loans()
                        .iter()
                        .find(|loan| {
                            loan.call() == fact.source_loan().call()
                                && loan.argument() == fact.source_loan().argument()
                        })
                        .unwrap();
                    assert_eq!(fact.expression(), loan.call());
                    assert_eq!(loan.target(), fact.origin());
                    assert_eq!(sources.slice(loan.begin_span()), Ok(backing));
                    for exit in plan.exits() {
                        let terminal = matches!(
                            exit.kind(),
                            Exit::Exhaustion | Exit::Break(_) | Exit::Return(_)
                        );
                        assert_eq!(
                            exit.actions()
                                .iter()
                                .filter(|a| matches!(a, Action::FinishProvider(_)))
                                .count(),
                            usize::from(terminal)
                        );
                        assert_eq!(
                            exit.actions()
                                .iter()
                                .filter(|a| matches!(a, Action::EndSource(_)))
                                .count(),
                            usize::from(terminal)
                        );
                        if let UnitLoanTarget::Temporary(root) = fact.origin() {
                            assert_eq!(exit.actions().iter().filter(|a| matches!(a, Action::Drop(drop) if drop.target() == UnitDropTarget::Temporary(*root))).count(), usize::from(terminal));
                        }
                    }
                    assert!(
                        !owned
                            .drops()
                            .iter()
                            .any(|drop| drop.target()
                                == UnitDropTarget::Temporary(fact.expression()))
                    );
                }
            }
        }
    }
}

#[test]
fn grouped_iteration_keeps_live_root_rejection_and_prefix_exit_boundary() {
    for depth in [0, 1, 3] {
        let group = |text: &str| format!("{}{text}{}", "(".repeat(depth), ")".repeat(depth));
        let expression = group("rangeView(source,0,1)");
        let (sources, document, rooted) = checked(&format!(
            "fun run(own source:List<String>):Unit{{for(item in {expression}){{consume(source)}}}}"
        ));
        for owned in [&document, &rooted] {
            let errors = owned
                .diagnostics()
                .iter()
                .filter(|d| d.code().to_string() == "L0135")
                .collect::<Vec<_>>();
            assert_eq!(errors.len(), 1, "{:?}", owned.diagnostics());
            assert_eq!(sources.slice(errors[0].primary_span()), Ok("source"));
        }
        for bound in ["return 7", "error(\"stop\")"] {
            let expression = group(&format!("rangeView(source,{bound},1)"));
            let (_, document, rooted) = checked(&format!(
                "fun run(own source:List<String>):Int{{for(item in {expression}){{println(item)}};return 0}}"
            ));
            for owned in [&document, &rooted] {
                assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
                assert!(owned.deferred().is_empty());
                assert!(
                    owned.iterations().is_empty(),
                    "prefix exit cannot acquire a provider"
                );
                assert!(owned.borrow_results().range_uses().is_empty());
                if bound == "return 7" {
                    // The helper's separate consume(own source) also has a FunctionEntry drop.
                    // Select the actual return edge, then forbid any second cleanup of its owner.
                    let transfers = owned
                        .drops()
                        .iter()
                        .filter(|drop| {
                            matches!(
                                drop.point(),
                                lang_frontend::ownership_checking::UnitDropPoint::ControlTransfer(
                                    _
                                )
                            )
                        })
                        .collect::<Vec<_>>();
                    let [drop] = transfers.as_slice() else {
                        panic!("one return-prefix cleanup")
                    };
                    assert!(matches!(drop.target(), UnitDropTarget::Named(_)));
                    assert_eq!(
                        owned
                            .drops()
                            .iter()
                            .filter(|other| other.target() == drop.target())
                            .count(),
                        1
                    );
                }
            }
        }
        for bound in [
            "break",
            "continue",
            "if(flag){break}else{0}",
            "if(flag){continue}else{0}",
        ] {
            let expression = group(&format!("rangeView(source,{bound},1)"));
            let (_, document, rooted) = checked(&format!(
                "fun run(own source:List<String>,flag:Boolean):Unit{{loop{{for(item in {expression}){{println(item)}};break}};consume(source)}}"
            ));
            for owned in [&document, &rooted] {
                assert!(
                    owned.diagnostics().is_empty(),
                    "{bound}: {:?}",
                    owned.diagnostics()
                );
                assert!(owned.deferred().is_empty());
                if !bound.starts_with("if") {
                    assert!(owned.iterations().is_empty());
                    assert!(owned.borrow_results().range_uses().is_empty());
                } else {
                    assert_eq!(owned.iterations().len(), 1);
                    assert_eq!(owned.borrow_results().range_uses().len(), 1);
                }
            }
        }
    }
}
