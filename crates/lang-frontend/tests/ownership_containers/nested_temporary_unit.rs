//! Unit Index/Group views retain the original temporary container, not an intermediate element.
//! Expected identities come from the input AST, independently of ownership products.

use lang_frontend::{
    diagnostic::DiagnosticDetail,
    name_resolution::SourceUnitId,
    ownership_checking::{
        CompilationUnitOwnership, LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
        UnitValueDeliveryKind, UnitValueDeliverySource,
    },
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::UnitExpressionId,
};

use super::mutable_unit;

fn expression(
    sources: &SourceMap,
    parsed: &ParsedFile,
    unit: SourceUnitId,
    text: &str,
    kind: impl Fn(&Expression) -> bool,
) -> UnitExpressionId {
    let found = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            (sources.slice(node.span()).unwrap() == text && kind(node.payload())).then_some(id)
        })
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "one independent AST site for {text}");
    UnitExpressionId::new(unit, found[0])
}

fn assert_backing_drops(
    parsed: &ParsedFile,
    owned: &CompilationUnitOwnership,
    expected: &[(UnitDropPoint, UnitExpressionId)],
) {
    // The complete table excludes extra cleanup of inner constructors, Index or Group views.
    assert_eq!(owned.drops().len(), expected.len(), "{:?}", owned.drops());
    for &(point, owner) in expected {
        let origin = parsed
            .ast()
            .expressions()
            .get(owner.expression())
            .unwrap()
            .span();
        assert_eq!(
            owned
                .drops()
                .iter()
                .filter(|drop| {
                    drop.point() == point
                        && drop.target() == UnitDropTarget::Temporary(owner)
                        && drop.value_origin() == origin
                })
                .count(),
            1,
            "exactly one backing cleanup at {point:?}"
        );
    }
}

#[test]
fn copy_and_value_delivery_release_only_the_original_backing_at_the_index() {
    let text = "fun take(own item: Int) {}\n\
                fun run() {\n\
                    val copied = (listOf(listOf(1))[0])[0]\n\
                    take(((listOf(listOf(2))[0])[0]))\n\
                }";
    mutable_unit::check_each(text, &[], |sources, parsed, unit, owned| {
        let copied = expression(sources, parsed, unit, "(listOf(listOf(1))[0])[0]", |e| {
            matches!(e, Expression::Index { .. })
        });
        let delivered = expression(sources, parsed, unit, "(listOf(listOf(2))[0])[0]", |e| {
            matches!(e, Expression::Index { .. })
        });
        let argument = expression(sources, parsed, unit, "((listOf(listOf(2))[0])[0])", |e| {
            matches!(e, Expression::Group { .. })
        });
        let call = expression(
            sources,
            parsed,
            unit,
            "take(((listOf(listOf(2))[0])[0]))",
            |e| matches!(e, Expression::Call { .. }),
        );
        let first = expression(sources, parsed, unit, "listOf(listOf(1))", |e| {
            matches!(e, Expression::Call { .. })
        });
        let second = expression(sources, parsed, unit, "listOf(listOf(2))", |e| {
            matches!(e, Expression::Call { .. })
        });
        let deliveries = owned
            .value_deliveries()
            .iter()
            .filter(|delivery| delivery.call() == call)
            .collect::<Vec<_>>();
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].argument(), argument);
        assert_eq!(deliveries[0].kind(), UnitValueDeliveryKind::Copy);
        assert_eq!(
            deliveries[0].source(),
            &UnitValueDeliverySource::BorrowedProjection { owner: second }
        );
        assert!(owned.loans().is_empty());
        assert_backing_drops(
            parsed,
            owned,
            &[
                (UnitDropPoint::AfterExpression(copied), first),
                (UnitDropPoint::AfterExpression(delivered), second),
            ],
        );
    });
}

#[test]
fn grouped_borrow_keeps_the_original_backing_until_call_return() {
    let text = "fun read(item: String) {}\n\
                fun run() { read(((listOf(listOf(\"x\"))[0])[0])) }";
    mutable_unit::check_each(text, &[], |sources, parsed, unit, owned| {
        let owner = expression(sources, parsed, unit, "listOf(listOf(\"x\"))", |e| {
            matches!(e, Expression::Call { .. })
        });
        let call = expression(
            sources,
            parsed,
            unit,
            "read(((listOf(listOf(\"x\"))[0])[0]))",
            |e| matches!(e, Expression::Call { .. }),
        );
        let argument = expression(
            sources,
            parsed,
            unit,
            "((listOf(listOf(\"x\"))[0])[0])",
            |e| matches!(e, Expression::Group { .. }),
        );
        let [loan] = owned.loans() else {
            panic!("one completed argument loan: {:?}", owned.loans())
        };
        assert_eq!(loan.call(), call);
        assert_eq!(loan.argument(), argument);
        assert_eq!(loan.kind(), LoanKind::Shared);
        assert_eq!(loan.target(), &UnitLoanTarget::Temporary(owner));
        assert_backing_drops(parsed, owned, &[(UnitDropPoint::CallReturn(call), owner)]);
    });
}

#[test]
fn nested_mutable_element_inout_uses_the_original_backing() {
    let text = "fun mutate(inout item: Int) { item = 2 }\n\
                fun run() {\n\
                    mutate(&((arrayOf(arrayOf(1))[0])[0]))\n\
                    mutate(&((mutableListOf(mutableListOf(3))[0])[0]))\n\
                }";
    mutable_unit::check_each(text, &[], |sources, parsed, unit, owned| {
        assert_eq!(owned.loans().len(), 2);
        let mut drops = Vec::new();
        for (position, container, value) in [(0, "arrayOf", 1), (1, "mutableListOf", 3)] {
            let owner_text = format!("{container}({container}({value}))");
            let argument_text = format!("(({owner_text}[0])[0])");
            let call_text = format!("mutate(&{argument_text})");
            let owner = expression(sources, parsed, unit, &owner_text, |e| {
                matches!(e, Expression::Call { .. })
            });
            let argument = expression(sources, parsed, unit, &argument_text, |e| {
                matches!(e, Expression::Group { .. })
            });
            let call = expression(sources, parsed, unit, &call_text, |e| {
                matches!(e, Expression::Call { .. })
            });
            let loan = &owned.loans()[position];
            assert_eq!(loan.call(), call);
            assert_eq!(loan.argument(), argument);
            assert_eq!(loan.kind(), LoanKind::Exclusive);
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(owner));
            assert_eq!(sources.slice(loan.begin_span()).unwrap(), "&");
            drops.push((UnitDropPoint::CallReturn(call), owner));
        }
        assert_backing_drops(parsed, owned, &drops);
    });
}

#[test]
fn move_only_reads_and_value_arguments_label_the_original_backing() {
    let text = "fun take(own item: String) {}\n\
                fun run() {\n\
                    val invalid = (listOf(listOf(\"read\"))[0])[0]\n\
                    take(((listOf(listOf(\"value\"))[0])[0]))\n\
                }";
    mutable_unit::check_each(text, &["L0136", "L0136"], |sources, parsed, unit, owned| {
        for (position, primary, backing) in [
            (
                0,
                "(listOf(listOf(\"read\"))[0])[0]",
                "listOf(listOf(\"read\"))",
            ),
            (
                1,
                "(listOf(listOf(\"value\"))[0])[0]",
                "listOf(listOf(\"value\"))",
            ),
        ] {
            let owner = expression(sources, parsed, unit, backing, |e| {
                matches!(e, Expression::Call { .. })
            });
            let diagnostic = &owned.diagnostics()[position];
            assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), primary);
            let labels = diagnostic
                .details()
                .iter()
                .filter_map(|detail| match detail {
                    DiagnosticDetail::Label(label) => Some(label),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(labels.len(), position + 1);
            assert_eq!(
                labels[0].span(),
                parsed
                    .ast()
                    .expressions()
                    .get(owner.expression())
                    .unwrap()
                    .span()
            );
            assert_eq!(
                labels[0].message(),
                "temporary container owns every initialized element"
            );
            if position == 1 {
                assert_eq!(sources.slice(labels[1].span()).unwrap(), "own item: String");
            }
        }
        assert!(owned.loans().is_empty());
        assert!(owned.value_deliveries().is_empty());
        assert!(owned.drops().is_empty());
    });
}

#[test]
fn later_index_return_cleans_only_the_evaluated_backing() {
    let text = "fun read(item: String) {}\n\
                fun run(flag: Boolean) {\n\
                    read((listOf(listOf(\"x\"))[0])[if (flag) return else 0])\n\
                }";
    mutable_unit::check_each(text, &[], |sources, parsed, unit, owned| {
        let owner = expression(sources, parsed, unit, "listOf(listOf(\"x\"))", |e| {
            matches!(e, Expression::Call { .. })
        });
        let argument = expression(
            sources,
            parsed,
            unit,
            "(listOf(listOf(\"x\"))[0])[if (flag) return else 0]",
            |e| matches!(e, Expression::Index { .. }),
        );
        let call = expression(
            sources,
            parsed,
            unit,
            "read((listOf(listOf(\"x\"))[0])[if (flag) return else 0])",
            |e| matches!(e, Expression::Call { .. }),
        );
        let returned = expression(sources, parsed, unit, "return", |e| {
            matches!(e, Expression::Return { .. })
        });
        let [loan] = owned.loans() else {
            panic!("only the completed argument establishes a loan")
        };
        assert_eq!(loan.call(), call);
        assert_eq!(loan.argument(), argument);
        assert_eq!(loan.kind(), LoanKind::Shared);
        assert_eq!(loan.target(), &UnitLoanTarget::Temporary(owner));
        assert_backing_drops(
            parsed,
            owned,
            &[
                (UnitDropPoint::ControlTransfer(returned), owner),
                (UnitDropPoint::CallReturn(call), owner),
            ],
        );
    });
}
