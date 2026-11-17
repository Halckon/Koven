//! Real assignment failure must not retain a consumer of a rolled-back source loan.
use super::*;

fn source(mutable: bool) -> String {
    let binding = if mutable { "var" } else { "val" };
    format!(
        "class Holder(var target:Int)\nfun count(value:View<String>):Int=1\nfun run(source:List<String>):Unit {{count(rangeView(source,0,1));{binding} fixed=0;fixed=count(rangeView(source,0,1))}}"
    )
}

fn assert_source_loans_present(checker: &Checker<'_>) {
    for usage in checker.borrow_results.range_uses() {
        let source = usage.source_loan();
        assert!(
            checker.loans.iter().any(|loan| {
                loan.call() == source.call()
                    && loan.argument() == source.argument()
                    && loan.kind() == crate::ownership_checking::LoanKind::Shared
                    && loan.target() == usage.origin()
            }),
            "every retained consumer must still reference its original source loan"
        );
    }
}

#[test]
fn rejected_assignment_keeps_prefix_and_revokes_rhs_range_use() {
    let text = source(false);
    let fixture = Fixture::range_source(&[("rollback.ko", &text)], 0);
    let shared = fixture.shared();
    let parts = fixture.parts(&shared);
    with_checker_phase(&fixture, &parts, false, false, |checker| {
        assert_eq!(checker.diagnostics.len(), 1);
        assert_eq!(checker.diagnostics[0].code().to_string(), "L0134");
        let diagnostic = &checker.diagnostics[0];
        let target = fixture.symbol(0, "fixed");
        let span = diagnostic.primary_span();
        assert_eq!(fixture.sources.slice(span).unwrap(), "fixed");
        assert_eq!(span.start(), text.rfind("fixed=").unwrap());
        let mut expected = crate::diagnostic::Diagnostic::new(
            &fixture.sources,
            crate::diagnostic::Severity::Error,
            checker.codes.immutable_inout,
            "assignment target is not a mutable place",
            span,
        )
        .unwrap();
        expected
            .add_label(
                &fixture.sources,
                checker.symbol_span(target).unwrap(),
                "immutable binding declared here",
            )
            .unwrap();
        assert_eq!(*diagnostic, expected);
        let uses = checker.borrow_results.range_uses();
        assert_eq!(uses.len(), 1, "only the real pre-assignment use survives");
        let call_span = checker
            .parsed
            .ast()
            .expressions()
            .get(uses[0].expression().expression())
            .unwrap()
            .span();
        assert_eq!(
            fixture.sources.slice(call_span).unwrap(),
            "rangeView(source,0,1)"
        );
        assert!(call_span.start() < checker.diagnostics[0].primary_span().start());
        assert_source_loans_present(checker);
    });
    // The existing aggregate failure gate already revokes public/raw products.
    let raw = fixture.dataflow(&parts);
    assert!(!raw.diagnostics.is_empty());
    assert!(raw.borrow_results.range_uses().is_empty());
}

#[test]
fn successful_assignment_keeps_both_range_uses_and_sources() {
    let text = source(true);
    let fixture = Fixture::range_source(&[("accepted.ko", &text)], 0);
    let shared = fixture.shared();
    let parts = fixture.parts(&shared);
    with_checker_phase(&fixture, &parts, false, false, |checker| {
        assert!(checker.diagnostics.is_empty());
        let uses = checker.borrow_results.range_uses();
        assert_eq!(uses.len(), 2);
        assert_ne!(uses[0].expression(), uses[1].expression());
        assert_source_loans_present(checker);
    });
    let raw = fixture.dataflow(&parts);
    assert!(raw.diagnostics.is_empty());
    assert!(raw.deferred.is_empty());
    assert_eq!(raw.borrow_results.range_uses().len(), 2);
}
