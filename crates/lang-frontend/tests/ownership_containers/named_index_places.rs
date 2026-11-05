//! SPEC-0341: named Index paths use the actual Unit producer and final type gate.
//! These regressions do not grant nested container lowering or retire Single witnesses.

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
    name_resolution::{SourceUnitId, UnitSymbolId},
    ownership_checking::{
        CompilationUnitOwnership, ElementIndexIdentity, LoanKind, OwnershipBindingKind,
        UnitDropPoint, UnitDropTarget, UnitLoanTarget, UnitOwnershipPlace, UnitValueDeliveryKind,
        UnitValueDeliverySource,
    },
    parser::{Item, NameMarker, ParsedFile, ValueParameter},
    source::{SourceMap, Span},
    type_checking::UnitExpressionId,
};

use super::mutable_unit::check_each;
use ElementIndexIdentity::{Known, Unknown};

fn parameter<'a>(
    sources: &SourceMap,
    parsed: &'a ParsedFile,
    function: &str,
    index: usize,
) -> &'a ValueParameter {
    parsed
        .ast()
        .items()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            Item::Function {
                name: NameMarker::Present(name),
                parameters,
                ..
            } if sources.slice(*name).unwrap() == function => Some(&parameters[index]),
            _ => None,
        })
        .expect("independent source parameter")
}

fn root(
    sources: &SourceMap,
    parsed: &ParsedFile,
    unit: SourceUnitId,
    owned: &CompilationUnitOwnership,
) -> UnitSymbolId {
    let span = parameter(sources, parsed, "run", 0).span;
    let bindings = owned
        .bindings()
        .iter()
        .filter(|binding| binding.declaration_span() == span)
        .collect::<Vec<_>>();
    let [binding] = bindings.as_slice() else {
        panic!("one independently located run parameter");
    };
    assert_eq!(binding.kind(), OwnershipBindingKind::Owned);
    assert_eq!(binding.symbol().source_unit(), unit);
    binding.symbol()
}

// Keep the existing public const-call surface checked by the Rust type checker.
const fn const_identity(
    place: &UnitOwnershipPlace,
) -> (UnitSymbolId, bool, Option<ElementIndexIdentity>) {
    (place.root(), place.is_root(), place.element())
}

fn assert_place(place: &UnitOwnershipPlace, root: UnitSymbolId, path: &[ElementIndexIdentity]) {
    assert_eq!(const_identity(place), (root, false, path.last().copied()));
    assert!(place.fields().is_empty());
    assert_eq!(place.elements(), path);
}

fn expression_text<'a>(
    sources: &'a SourceMap,
    parsed: &ParsedFile,
    unit: SourceUnitId,
    expression: UnitExpressionId,
) -> &'a str {
    assert_eq!(expression.source_unit(), unit);
    let span = parsed
        .ast()
        .expressions()
        .get(expression.expression())
        .unwrap()
        .span();
    assert_eq!(span.source_id(), parsed.source_id());
    sources.slice(span).unwrap()
}

fn assert_named_cleanup(
    sources: &SourceMap,
    parsed: &ParsedFile,
    unit: SourceUnitId,
    owned: &CompilationUnitOwnership,
    root: UnitSymbolId,
    last_call: &str,
) {
    let [drop] = owned.drops() else {
        panic!(
            "only the original named container owns cleanup: {:?}",
            owned.drops()
        );
    };
    assert_eq!(drop.target(), UnitDropTarget::Named(root));
    let NameMarker::Present(origin) = parameter(sources, parsed, "run", 0).name else {
        panic!("named run parameter");
    };
    assert_eq!(drop.value_origin(), origin);
    let UnitDropPoint::CallReturn(call) = drop.point() else {
        panic!("last element loan protects its named owner until call return");
    };
    assert_eq!(expression_text(sources, parsed, unit, call), last_call);
}

#[test]
fn named_multi_index_copy_shared_and_inout_keep_each_path_component() {
    let text = "fun take(own n: Int) {}\n\
                fun look(n: Int) {}\n\
                fun change(inout n: Int) {}\n\
                fun run(own xs: Array<MutableList<Int>>, i: Int) {\n\
                    take((xs[1])[2])\n\
                    look(xs[3][4])\n\
                    change(&xs[5][6])\n\
                    look(xs[i][7])\n\
                }";
    check_each(text, &[], |sources, parsed, unit, owned| {
        let root = root(sources, parsed, unit, owned);
        let [delivery] = owned.value_deliveries() else {
            panic!("one Copy Value argument");
        };
        assert_eq!(delivery.kind(), UnitValueDeliveryKind::Copy);
        assert_eq!(
            expression_text(sources, parsed, unit, delivery.argument()),
            "(xs[1])[2]"
        );
        let UnitValueDeliverySource::Place(place) = delivery.source() else {
            panic!("named nested Copy is not temporary-backed");
        };
        assert_place(place, root, &[Known(1), Known(2)]);
        let expected = [
            (LoanKind::Shared, "xs[3][4]", [Known(3), Known(4)]),
            (LoanKind::Exclusive, "xs[5][6]", [Known(5), Known(6)]),
            (LoanKind::Shared, "xs[i][7]", [Unknown, Known(7)]),
        ];
        assert_eq!(owned.loans().len(), expected.len());
        for (loan, (kind, argument, path)) in owned.loans().iter().zip(expected) {
            assert_eq!(loan.kind(), kind);
            assert_eq!(
                expression_text(sources, parsed, unit, loan.argument()),
                argument
            );
            let UnitLoanTarget::Place(place) = loan.target() else {
                panic!("named nested loan retains its root");
            };
            assert_place(place, root, &path);
        }
        assert_named_cleanup(sources, parsed, unit, owned, root, "look(xs[i][7])");
    });
}

#[test]
fn named_multi_index_exclusive_loans_prove_outer_and_inner_siblings() {
    let text = "fun pair(inout first: Int, inout second: Int) {}\n\
                fun run(own xs: MutableList<MutableList<Int>>) {\n\
                    pair(&xs[0][3], &xs[1][3])\n\
                    pair(&xs[2][4], &xs[2][5])\n\
                }";
    check_each(text, &[], |sources, parsed, unit, owned| {
        let root = root(sources, parsed, unit, owned);
        let expected = [
            ("xs[0][3]", [Known(0), Known(3)]),
            ("xs[1][3]", [Known(1), Known(3)]),
            ("xs[2][4]", [Known(2), Known(4)]),
            ("xs[2][5]", [Known(2), Known(5)]),
        ];
        assert_eq!(owned.loans().len(), expected.len());
        for (loan, (argument, path)) in owned.loans().iter().zip(expected) {
            assert_eq!(loan.kind(), LoanKind::Exclusive);
            assert_eq!(
                expression_text(sources, parsed, unit, loan.argument()),
                argument
            );
            let UnitLoanTarget::Place(place) = loan.target() else {
                panic!("named sibling loan");
            };
            assert_place(place, root, &path);
        }
        assert_named_cleanup(
            sources,
            parsed,
            unit,
            owned,
            root,
            "pair(&xs[2][4], &xs[2][5])",
        );
    });
}

fn assert_conflict(
    sources: &SourceMap,
    parsed: &ParsedFile,
    owned: &CompilationUnitOwnership,
    message: &str,
    primary: Span,
    origin: Span,
    parameter: Span,
) {
    let catalog = DiagnosticCodeCatalog::try_new(&["L0135"]).unwrap();
    let mut expected = Diagnostic::new(
        sources,
        Severity::Error,
        catalog.resolve("L0135").unwrap(),
        message,
        primary,
    )
    .unwrap();
    expected
        .add_label(sources, origin, "conflicting loan starts here")
        .unwrap();
    expected
        .add_label(sources, parameter, "selected parameter declared here")
        .unwrap();
    assert_eq!(owned.diagnostics(), &[expected]);
    assert_eq!(primary.source_id(), parsed.source_id());
    assert!(owned.loans().is_empty());
    assert!(owned.value_deliveries().is_empty());
    assert!(owned.drops().is_empty());
}

#[test]
fn named_multi_index_same_unknown_and_ancestor_loans_conflict() {
    for (first, second, second_type) in [
        ("xs[0][2]", "xs[0][2]", "Int"),
        ("xs[i][2]", "xs[0][2]", "Int"),
        ("xs[0][i]", "xs[0][2]", "Int"),
        ("xs[0][2]", "xs[0]", "MutableList<Int>"),
    ] {
        let text = format!(
            "fun clash(inout first: Int, inout second: {second_type}) {{}}\n\
             fun run(own xs: MutableList<MutableList<Int>>, i: Int) {{\n\
                 clash(&{first}, &{second})\n\
             }}"
        );
        check_each(&text, &["L0135"], |sources, parsed, _, owned| {
            let source = parsed.source_id();
            let origin = text.find('&').unwrap();
            let primary = text.rfind('&').unwrap();
            assert_conflict(
                sources,
                parsed,
                owned,
                "new loan conflicts with an active loan",
                sources.span(source, primary, primary + 1).unwrap(),
                sources.span(source, origin, origin + 1).unwrap(),
                parameter(sources, parsed, "clash", 1).span,
            );
        });
    }
}

#[test]
fn named_multi_index_loan_blocks_moving_the_original_root() {
    let text = "fun consume(own all: MutableList<MutableList<Int>>) {}\n\
                fun outer(first: Int, own effect: Unit) {}\n\
                fun run(own xs: MutableList<MutableList<Int>>) {\n\
                    outer(xs[0][1], consume(xs))\n\
                }";
    check_each(text, &["L0135"], |sources, parsed, _, owned| {
        let source = parsed.source_id();
        let origin = text.find("xs[0][1]").unwrap();
        let primary = text.rfind("xs").unwrap();
        assert_conflict(
            sources,
            parsed,
            owned,
            "move conflicts with an active loan",
            sources.span(source, primary, primary + 2).unwrap(),
            sources
                .span(source, origin, origin + "xs[0][1]".len())
                .unwrap(),
            parameter(sources, parsed, "consume", 0).span,
        );
    });
}

#[test]
fn named_multi_index_move_only_value_keeps_the_original_owner_label() {
    let text = "class Resource {}\n\
                fun take(own item: Resource) {}\n\
                fun run(own xs: List<List<Resource>>) { take(xs[0][1]) }";
    check_each(text, &["L0136"], |sources, parsed, _, owned| {
        let source = parsed.source_id();
        let start = text.find("xs[0][1]").unwrap();
        let catalog = DiagnosticCodeCatalog::try_new(&["L0136"]).unwrap();
        let mut expected = Diagnostic::new(
            sources,
            Severity::Error,
            catalog.resolve("L0136").unwrap(),
            "cannot move a non-Copyable element out of a sequential container",
            sources
                .span(source, start, start + "xs[0][1]".len())
                .unwrap(),
        )
        .unwrap();
        let NameMarker::Present(origin) = parameter(sources, parsed, "run", 0).name else {
            panic!("named owner parameter");
        };
        expected
            .add_label(
                sources,
                origin,
                "container owner remains responsible for every initialized element",
            )
            .unwrap();
        expected
            .add_label(
                sources,
                parameter(sources, parsed, "take", 0).span,
                "selected parameter declared here",
            )
            .unwrap();
        assert_eq!(owned.diagnostics(), &[expected]);
        assert!(owned.loans().is_empty());
        assert!(owned.value_deliveries().is_empty());
        assert!(owned.drops().is_empty());
    });
}
