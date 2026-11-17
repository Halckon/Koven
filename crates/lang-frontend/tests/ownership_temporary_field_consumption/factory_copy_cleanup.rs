//! Owned factory results are distinct from borrowed result storage.
use super::*;
use lang_frontend::ownership_checking::{UnitDropPoint, UnitDropTarget};

#[test]
fn owned_factory_field_read_has_one_result_cleanup() {
    for receiver in ["make(7)", "(make(7))", "((make(7)))"] {
        let text = format!(
            "class Holder(val value: Int)\nfun make(own n: Int): Holder = Holder(n)\nfun use(): Int = {receiver}.value"
        );
        for constants in [false, true] {
            for rooted in [false, true] {
                let (sources, _, owned, parsed) = observe(&text, constants, rooted);
                assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
                let [drop] = owned.drops() else {
                    panic!("one result cleanup: {:?}", owned.drops());
                };
                let UnitDropPoint::AfterExpression(point) = drop.point() else {
                    panic!("read completion");
                };
                let UnitDropTarget::Temporary(target) = drop.target() else {
                    panic!("factory result");
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
                    Ok("make(7)")
                );
                assert_eq!(sources.slice(drop.value_origin()), Ok("make(7)"));
            }
        }
    }
}

#[test]
fn borrowed_factory_result_does_not_become_an_owned_temporary() {
    let text = "class Holder(val value: Int)\nfun view(source: Holder): borrow Holder from source = source\nfun use(source: Holder): Int = view(source).value";
    for constants in [false, true] {
        for rooted in [false, true] {
            let (sources, source, owned, _) = observe(text, constants, rooted);
            let catalog = DiagnosticCodeCatalog::try_new(&["L0164"]).unwrap();
            let call = text.rfind("view(source)").unwrap();
            let declaration = text.find("borrow Holder").unwrap();
            let mut expected = Diagnostic::new(
                &sources,
                Severity::Error,
                catalog.resolve("L0164").unwrap(),
                "caller borrow result continuation is not yet proven",
                sources
                    .span(source, call, call + "view(source)".len())
                    .unwrap(),
            )
            .unwrap();
            expected
                .add_label(
                    &sources,
                    sources.span(source, declaration, declaration + 6).unwrap(),
                    "borrow result declared here",
                )
                .unwrap();
            assert_eq!(owned.diagnostics(), &[expected]);
            assert!(owned.drops().is_empty(), "{:?}", owned.drops());
        }
    }
}

#[test]
fn factory_argument_cleanup_and_result_cleanup_have_separate_boundaries() {
    for (argument, outcome) in [("7", 2), ("return 9", 1), ("stop()", 0)] {
        let call = format!("make(\"prefix\", {argument})");
        let text = format!(
            "class Holder(val value: Int)\nfun make(prefix: String, own n: Int): Holder = Holder(n)\nfun stop(): Nothing = stop()\nfun use(): Int {{ return {call}.value }}"
        );
        for constants in [false, true] {
            for rooted in [false, true] {
                let (sources, _, owned, _) = observe(&text, constants, rooted);
                assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
                assert_eq!(owned.drops().len(), outcome, "{:?}", owned.drops());
                if outcome > 0 {
                    let prefix = &owned.drops()[0];
                    assert_eq!(sources.slice(prefix.value_origin()), Ok("\"prefix\""));
                    assert!(matches!(prefix.target(), UnitDropTarget::Temporary(_)));
                    if outcome == 1 {
                        assert!(matches!(prefix.point(), UnitDropPoint::ControlTransfer(_)));
                    } else {
                        assert!(matches!(prefix.point(), UnitDropPoint::CallReturn(_)));
                    }
                }
                if outcome == 2 {
                    let result = &owned.drops()[1];
                    assert_eq!(sources.slice(result.value_origin()), Ok(call.as_str()));
                    assert!(matches!(result.point(), UnitDropPoint::AfterExpression(_)));
                    assert!(matches!(result.target(), UnitDropTarget::Temporary(_)));
                    assert_ne!(result.target(), owned.drops()[0].target());
                }
            }
        }
    }
}
