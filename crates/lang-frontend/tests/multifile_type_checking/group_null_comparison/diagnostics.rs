//! Exact diagnostic construction and equality assertions for the private matrix.
use super::{Fixture, declaration, leaf, node_types, payload, span};
use lang_frontend::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity, codes},
    parser::{Expression, LiteralKind},
    source::Span,
};

pub(super) fn diagnostic(
    fixture: &Fixture,
    code: &str,
    message: &str,
    primary: Span,
) -> Diagnostic {
    Diagnostic::new(
        &fixture.sources,
        Severity::Error,
        codes::catalog().unwrap().resolve(code).unwrap(),
        message,
        primary,
    )
    .unwrap()
}

pub(super) fn cannot_infer(fixture: &Fixture, expression: ExpressionId) -> Diagnostic {
    let expression = leaf(fixture, expression);
    assert!(matches!(
        payload(fixture, expression),
        Expression::Literal(LiteralKind::Null)
    ));
    diagnostic(
        fixture,
        "L0083",
        "cannot infer the type of null without a nullable expected type",
        span(fixture, expression),
    )
}

pub(super) fn mismatch(fixture: &Fixture, argument: ExpressionId) -> Diagnostic {
    // The producer descends Group with expected type, so the Name owns the error.
    let argument = leaf(fixture, argument);
    assert_eq!(fixture.sources.slice(span(fixture, argument)), Ok("x"));
    let (node, nullable) = node_types(fixture);
    let observe = fixture
        .typed
        .signatures()
        .declaration(declaration(&fixture.names, "observe"))
        .unwrap()
        .callable()
        .unwrap();
    let [parameter] = observe.parameters() else {
        panic!("one observe parameter");
    };
    assert_eq!(parameter.name(), Some("value"));
    assert_eq!(parameter.ty(), node);
    assert_eq!(fixture.sources.slice(parameter.span()), Ok("value: Node"));
    let mut expected = diagnostic(
        fixture,
        "L0084",
        "expression type does not match the expected type",
        span(fixture, argument),
    );
    expected
        .add_label(
            &fixture.sources,
            parameter.span(),
            format!(
                "expected declaration#{}, found {:?}",
                declaration(&fixture.names, "Node").index(),
                fixture.typed.types().get(nullable).unwrap(),
            ),
        )
        .unwrap();
    expected
}

pub(super) fn assert_diagnostics(fixture: &Fixture, expected: &[Diagnostic]) {
    assert_eq!(
        fixture.typed.body_diagnostics(),
        expected,
        "{}: exact body diagnostics",
        fixture.label
    );
    assert_eq!(
        fixture.typed.diagnostics(),
        expected,
        "{}: exact full diagnostics",
        fixture.label
    );
    assert_eq!(
        fixture.typed.clone().validate().is_ok(),
        expected.is_empty(),
        "{}: validation cannot hide diagnostics",
        fixture.label
    );
}
