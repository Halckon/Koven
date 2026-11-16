//! Compare the cached gate to the unchanged predicate with actual typed Nothing facts.
use super::*;

#[test]
fn typed_borrow_call_gate_matches_original_for_every_call() {
    let fixture = Fixture::new(&[(
        "index.ko",
        r#"
class Holder(var target:Int)
fun view(source:String):borrow String from source=source
fun count(value:String):Int=1
fun run(source:String):Unit { count(view(source)); count((view(source))) }
"#,
    )]);
    let shared = fixture.shared();
    let parts = fixture.parts(&shared);
    with_checker_phase(&fixture, &parts, false, false, |checker| {
        assert!(checker.diagnostics.is_empty());
        assert_eq!(checker.borrow_results.call_uses().len(), 2);
        let index = checker
            .borrow_call_admission
            .get()
            .expect("real consumers initialize the cache");
        for (id, node) in checker.parsed.ast().expressions().iter() {
            if !matches!(node.payload(), Expression::Call { .. }) {
                continue;
            }
            let expected = crate::ownership_checking::borrow_result::call_use_is_straight_line(
                checker.parsed,
                id,
            ) && !checker
                .parsed
                .ast()
                .expressions()
                .iter()
                .any(|(other, inner)| {
                    node.span().start() <= inner.span().start()
                        && inner.span().end() <= node.span().end()
                        && checker.is_nothing_expression(checker.unit_expression(other))
                });
            assert_eq!(index.allows(checker.parsed, id), expected);
        }
        assert!(std::ptr::eq(
            index,
            checker.borrow_call_admission.get().unwrap()
        ));
    });
}

#[test]
fn actual_typed_nothing_inside_call_keeps_gate_closed() {
    let fixture = Fixture::new(&[(
        "nothing.ko",
        r#"
class Holder(var target:Int)
fun stop():Nothing=stop()
fun view(source:String):borrow String from source=source
fun count(value:String, gate:Int):Int=1
fun run(source:String):Unit { count(view(source), stop()) }
"#,
    )]);
    let shared = fixture.shared();
    let parts = fixture.parts(&shared);
    with_checker_phase(&fixture, &parts, false, false, |checker| {
        let index = borrow_call_admission::AdmissionIndex::new(checker.parsed, |id| {
            checker.is_nothing_expression(checker.unit_expression(id))
        });
        let mut outer_seen = false;
        for (id, node) in checker.parsed.ast().expressions().iter() {
            if !matches!(node.payload(), Expression::Call { .. }) {
                continue;
            }
            let straight = crate::ownership_checking::borrow_result::call_use_is_straight_line(
                checker.parsed,
                id,
            );
            let nothing = checker
                .parsed
                .ast()
                .expressions()
                .iter()
                .any(|(other, inner)| {
                    node.span().start() <= inner.span().start()
                        && inner.span().end() <= node.span().end()
                        && checker.is_nothing_expression(checker.unit_expression(other))
                });
            assert_eq!(index.allows(checker.parsed, id), straight && !nothing);
            if fixture.sources.slice(node.span()).unwrap() == "count(view(source), stop())" {
                assert!(straight, "the original syntax gate permits the outer call");
                assert!(nothing, "the outer call contains an actual typed Nothing");
                assert!(!index.allows(checker.parsed, id));
                outer_seen = true;
            }
        }
        assert!(outer_seen, "the exact outer consumer was inspected");
    });
}
