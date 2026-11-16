//! Real assignment failure must not retain a consumer of a rolled-back source loan.
use super::*;

fn source(mutable: bool) -> String {
    let binding = if mutable { "var" } else { "val" };
    format!(
        "class Holder(var target:Int)\nfun view(source:String):borrow String from source=source\nfun count(value:String):Int=1\nfun run(source:String):Unit {{count(view(source));{binding} fixed=0;fixed=count(view(source))}}"
    )
}

fn assert_source_loans_present(checker: &Checker<'_>) {
    for usage in checker.borrow_results.call_uses() {
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
fn rejected_assignment_keeps_prefix_and_revokes_rhs_borrow_call_use() {
    let text = source(false);
    let fixture = Fixture::new(&[("rollback.ko", &text)]);
    let shared = fixture.shared();
    let parts = fixture.parts(&shared);
    with_checker_phase(&fixture, &parts, false, false, |checker| {
        assert_eq!(checker.diagnostics.len(), 1);
        assert_eq!(checker.diagnostics[0].code().to_string(), "L0134");
        let uses = checker.borrow_results.call_uses();
        assert_eq!(uses.len(), 1, "only the real pre-assignment use survives");
        let call_span = checker
            .parsed
            .ast()
            .expressions()
            .get(uses[0].call().expression())
            .unwrap()
            .span();
        assert_eq!(
            fixture.sources.slice(call_span).unwrap(),
            "count(view(source))"
        );
        assert!(call_span.start() < checker.diagnostics[0].primary_span().start());
        assert_source_loans_present(checker);
    });
    // The existing aggregate failure gate already revokes public/raw products.
    let raw = fixture.dataflow(&parts);
    assert!(!raw.diagnostics.is_empty());
    assert!(raw.borrow_results.call_uses().is_empty());
}

#[test]
fn successful_assignment_keeps_both_borrow_call_uses_and_sources() {
    let text = source(true);
    let fixture = Fixture::new(&[("accepted.ko", &text)]);
    let shared = fixture.shared();
    let parts = fixture.parts(&shared);
    with_checker_phase(&fixture, &parts, false, false, |checker| {
        assert!(checker.diagnostics.is_empty());
        let uses = checker.borrow_results.call_uses();
        assert_eq!(uses.len(), 2);
        assert_ne!(uses[0].call(), uses[1].call());
        assert_source_loans_present(checker);
    });
    let raw = fixture.dataflow(&parts);
    assert!(raw.diagnostics.is_empty());
    assert!(raw.deferred.is_empty());
    assert_eq!(raw.borrow_results.call_uses().len(), 2);
}
