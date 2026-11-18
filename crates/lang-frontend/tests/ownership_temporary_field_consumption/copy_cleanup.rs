//! A copied field releases its completed temporary constructor parent once.
use super::*;
use lang_frontend::ownership_checking::{UnitDropPoint, UnitDropTarget};

#[test]
fn copy_field_releases_the_constructor_after_read() {
    for receiver in ["Holder(7)", "(Holder(7))", "((Holder(7)))"] {
        let text = format!("class Holder(val value: Int)\nfun use(): Int = {receiver}.value");
        for constants in [false, true] {
            for rooted in [false, true] {
                let (sources, source, owned, parsed) = observe(&text, constants, rooted);
                assert!(owned.diagnostics().is_empty());
                let [drop] = owned.drops() else {
                    panic!("one actual parent cleanup: {:?}", owned.drops());
                };
                let UnitDropPoint::AfterExpression(point) = drop.point() else {
                    panic!("read completion point");
                };
                let UnitDropTarget::Temporary(target) = drop.target() else {
                    panic!("temporary parent");
                };
                assert_eq!(point.source_unit(), target.source_unit());
                assert_eq!(
                    sources.slice(
                        parsed
                            .ast()
                            .expressions()
                            .get(point.expression())
                            .unwrap()
                            .span()
                    ),
                    Ok(receiver)
                );
                assert_eq!(
                    sources.slice(
                        parsed
                            .ast()
                            .expressions()
                            .get(target.expression())
                            .unwrap()
                            .span()
                    ),
                    Ok("Holder(7)")
                );
                assert_eq!(drop.value_origin().source_id(), source);
                assert_eq!(sources.slice(drop.value_origin()), Ok("Holder(7)"));
                assert!(owned.loans().is_empty());
            }
        }
    }
}

#[test]
fn borrow_and_unfinished_constructor_do_not_gain_early_cleanup() {
    for (body, borrowed) in [
        (
            "fun read(value: Int): Unit {}\nfun use(): Unit { read((Holder(7)).value) }",
            true,
        ),
        (
            "fun read(value: Int?): Unit {}\nfun use(): Unit { read((Holder(7)).value) }",
            true,
        ),
        (
            "fun stop(): Nothing = stop()\nfun use(): Int = Holder(stop()).value",
            false,
        ),
        ("fun use(): Int { return Holder(return 9).value }", false),
    ] {
        let text = format!("class Holder(val value: Int)\n{body}");
        for constants in [false, true] {
            for rooted in [false, true] {
                let (_, _, owned, _) = observe(&text, constants, rooted);
                assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
                if borrowed {
                    assert_eq!(owned.drops().len(), 1);
                    assert!(matches!(
                        owned.drops()[0].point(),
                        UnitDropPoint::CallReturn(_)
                    ));
                } else {
                    assert!(owned.drops().is_empty(), "{:?}", owned.drops());
                }
            }
        }
    }
}
