//! One ordinary analysis containing P1/P2/P3/P4 and B/Call/Return, without native execution.
use super::*;
#[test]
fn temporary_named_receiver_and_owner_siblings_keep_receipts_separate() {
    let text = r#"class Node { fun touch(): Int = 1 }
fun receiver(own node: Node): Unit { node.touch() }
fun view(source: String): borrow String from source = source
fun consume(item: String): Unit {}
fun loan(): Unit { val owner = "kept"; consume(view(owner)) }
fun sink(own item: String): Unit {}
fun owner_call(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; sink(held) }
fun owner_drop(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; val retired = held }
fun inspect(value: String): Int = 1
fun named(own source: String): Unit { inspect(source) }
fun owner_return(flag: Boolean): String { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; return held }
fun temporary(): Unit { inspect("temporary") }
fun entry(): Unit { temporary(); receiver(Node()); named("named"); loan(); owner_call(false); owner_drop(false); val result = owner_return(false) }"#;
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "receipt-siblings.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "receipt-siblings.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "entry")
            .unwrap()
            .id();
        let (((((((lowered, calls), drops), consumer), receiver), named), returns), temporary) =
            super::super::refine::temporary_receipt::observe(|| {
                crate::ssa::observe_owner_returns(|| {
                    super::super::refine::named_receipt::observe(|| {
                        super::super::refine::receiver_receipt::observe(|| {
                            super::super::refine::loan_receipt::observe(|| {
                                crate::ssa::observe_owner_drops(|| {
                                    crate::ssa::observe_owner_calls(|| {
                                        super::super::unit_lower::lower_scalar_unit_with_entry(
                                            &sources, &inputs, &names, &types, &typed, &owned,
                                            entry,
                                        )
                                    })
                                })
                            })
                        })
                    })
                })
            });
        let (program, _) = lowered.unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
        let [p1] = temporary.as_slice() else {
            panic!("one P1 receipt");
        };
        let [p3] = named.as_slice() else {
            panic!("one P3 receipt");
        };
        assert_eq!(returns.len(), 1);
        assert!(returns[0].1.is_some());
        assert_eq!(returns[0].1, returns[0].2);
        let [p2] = receiver.as_slice() else {
            panic!("one P2 receipt");
        };
        let [p4] = consumer.as_slice() else {
            panic!("one P4 receipt");
        };
        for (caller, ids, entities) in [
            (p2.caller, &p2.ids, &p2.entities),
            (p3.caller, &p3.ids, &p3.entities),
            (p4.caller, &p4.ids, &p4.entities),
        ] {
            assert_ne!(p1.caller, caller);
            assert!(p1.ids.iter().all(|id| !ids.contains(id)));
            assert!(p1.entities.iter().all(|entity| !entities.contains(entity)));
        }
        for (caller, ids, entities) in [
            (p2.caller, &p2.ids, &p2.entities),
            (p4.caller, &p4.ids, &p4.entities),
        ] {
            assert_ne!(p3.caller, caller);
            assert!(p3.ids.iter().all(|id| !ids.contains(id)));
            assert!(p3.entities.iter().all(|entity| !entities.contains(entity)));
        }
        assert_ne!(p2.caller, p4.caller);
        assert_ne!(p2.call, p4.call);
        assert_ne!(p2.callable, p4.callable);
        assert!(p2.ids.iter().all(|id| !p4.ids.contains(id)));
        assert!(
            p2.entities
                .iter()
                .all(|entity| !p4.entities.contains(entity))
        );
        assert_eq!(
            drops.iter().filter(|drop| drop.3.is_some()).count(),
            1,
            "only B owner Drop authorization"
        );
        assert_eq!(
            calls.iter().filter(|call| call.3.is_some()).count(),
            1,
            "only own String Call authorization"
        );
        for (protocol, ids, call) in [
            (1, &p1.ids, p1.call),
            (2, &p2.ids, p2.call),
            (3, &p3.ids, p3.call),
            (4, &p4.ids, p4.call),
        ] {
            let matches = owned
                .ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|o| o.regions())
                .filter(|r| r.protocol_version() == protocol && r.call() == call)
                .collect::<Vec<_>>();
            let [region] = matches.as_slice() else {
                panic!("one same-analysis region");
            };
            assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
            assert_eq!(*ids, region.action_ids().collect::<Vec<_>>());
        }
    }
}
