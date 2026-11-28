//! Migrated range-prefix sources: actual planner alternatives, not a public complete table.
use super::super::origin_probe::{Event, attribute_observed_if};
use super::*;
use crate::name_resolution::SourceUnitKey;
use crate::ownership_checking::{RangeUseSite, UnitDropPoint};
use crate::type_checking::UnitExpressionId;

#[test]
fn range_prefix_same_operand_has_actual_transfer_and_normal_removal() {
    for body in [
        "val root=listOf(\"kept\");read(rangeView(root,0,1),if(flag){return}else{0});consume(root)",
        "read(rangeView(listOf(\"kept\"),0,1),if(flag){return}else{0})",
        "val root=listOf(\"kept\");block {borrow val parent=rangeView(root,0,1);read(rangeView(parent,0,1),if(flag){return}else{0})};consume(root)",
        "read(rangeView(rangeView(listOf(\"kept\"),0,1),0,1),if(flag){return}else{0})",
    ] {
        // Preserve the full sources from the former ownership_range_construction case.
        let text = format!(
            "fun observe(source: View<String>): Unit {{}}\nfun alias(source: View<String>): borrow View<String> from source = source\nfun consume(own source: List<String>): Unit {{}}\nfun read(view:View<String>,number:Int):Unit{{}}\nfun run(flag:Boolean):Unit{{{body}}}"
        );
        for rooted in [false, true] {
            let fixture = Fixture::range_prefix_source("range-construct.ko", &text, rooted);
            let [entry] = fixture.names.names().index().source_units() else {
                panic!("one original source");
            };
            assert_eq!(
                fixture.sources.source_text(entry.source_id()),
                Ok(text.as_str())
            );
            match (rooted, entry.key()) {
                (false, SourceUnitKey::MemoryDocument) => {}
                (true, SourceUnitKey::Rooted { root, logical_path }) => {
                    assert_eq!(root.as_str(), "std");
                    assert_eq!(logical_path.as_str(), "range-construct.ko");
                }
                _ => panic!("original input layout"),
            }
            assert_prefix(&fixture, body);
        }
    }
}

fn assert_prefix(fixture: &Fixture, body: &str) {
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(owned.clone().validate().is_ok());
    let read_start = body.find("read(").unwrap();
    let read_end =
        body.find("if(flag){return}else{0})").unwrap() + "if(flag){return}else{0})".len();
    let call = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, &body[read_start..read_end]),
    );
    let transfer = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "return"));
    let ranges = owned
        .borrow_results()
        .range_uses()
        .iter()
        .filter(|fact| fact.site() == RangeUseSite::Call(call))
        .collect::<Vec<_>>();
    assert_eq!(ranges.len(), 1);
    let range = ranges[0];
    let loans = owned
        .loans()
        .iter()
        .filter(|loan| loan.call() == call && loan.argument() == range.expression())
        .collect::<Vec<_>>();
    assert_eq!(loans.len(), 1);
    let loan = loans[0];
    assert_eq!(loan.target(), range.origin());
    compare_and_inspect(fixture, &fixture.parts(&owned), |checker| {
        let make = || {
            super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            )
        };
        let baseline = make().run().unwrap();
        let mut planner = make();
        planner.origin_probe = Some(Vec::new());
        let observed = planner.run().unwrap();
        assert_eq!(baseline.facts, observed.facts);
        assert_eq!(
            baseline.conditional_receiver_facts,
            observed.conditional_receiver_facts
        );
        let events = observed.origin_probe.as_ref().unwrap();
        let begun = |event: &&Event| **event == Event::BorrowRegistered(loan.clone());
        let unwound = |event: &&Event| matches!(event, Event::UnwindBorrowRemoved { loan: actual, point } if actual == loan && *point == UnitDropPoint::ControlTransfer(transfer));
        let removed = |event: &&Event| matches!(event, Event::PendingBorrowRemoved { loan: actual, call: actual_call } if actual == loan && *actual_call == call);
        assert_eq!(events.iter().filter(begun).count(), 1);
        let removals = events
            .iter()
            .filter(|event| {
                matches!(event,
                Event::UnwindBorrowRemoved { loan: actual, .. }
                | Event::PendingBorrowRemoved { loan: actual, .. } if actual == loan)
            })
            .collect::<Vec<_>>();
        assert_eq!(removals.len(), 2, "no other removal of this exact loan");
        assert!(removals.iter().any(unwound));
        assert!(removals.iter().any(removed));
        let paths = attribute_observed_if(events).expect("one actual conditional operand");
        assert_eq!(paths.continuing_arms, [1]);
        assert_eq!(paths.prefix.iter().filter(begun).count(), 1);
        assert_eq!(paths.arms[0].iter().filter(unwound).count(), 1);
        assert!(!paths.arms[0].iter().any(|e| removed(&e)));
        assert!(!paths.arms[1].iter().any(|e| unwound(&e) || removed(&e)));
        assert_eq!(paths.suffix.iter().filter(removed).count(), 1);
        assert!(!paths.suffix.iter().any(|e| unwound(&e)));
        // The borrowed range is deliberately outside direct-call public projection.
        assert!(
            !events.iter().any(
                |e| matches!(e, Event::DirectBorrowEnded { loan: actual, .. } if actual == loan)
            )
        );
        // These are alternative traversal paths, never two dynamic ends on one path.
    });
}
