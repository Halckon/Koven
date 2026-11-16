//! Real same-run source fixtures; no fabricated loan, catalog or cleanup facts.
use super::*;
use crate::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    type_checking::{check_compilation_unit_types, standard_environments},
};
#[test]
fn callable_loan_links_use_original_nonempty_normal_regions() {
    for reverse in [false, true] {
        let mut sources = SourceMap::new();
        let a = sources
            .add_source(
                "a.ko",
                "fun inspect(value: String, other: String): Int = 1\nfun first(): Unit { inspect(\"first\", \"second\") }",
            )
            .unwrap();
        let b = sources
            .add_source(
                "b.ko",
                "fun number(): Int = 1\nfun second(): Unit { inspect(\"third\", \"fourth\"); number() }",
            )
            .unwrap();
        let pa = parse_file(&sources, &lex(&sources, a).unwrap()).unwrap();
        let pb = parse_file(&sources, &lex(&sources, b).unwrap()).unwrap();
        let mut inputs = vec![
            SourceUnitInput::new("root", "a.ko", a, &pa),
            SourceUnitInput::new("root", "b.ko", b, &pb),
        ];
        if reverse {
            inputs.reverse();
        }
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let owned = analysis::analyze(
            &sources,
            &inputs,
            names.names(),
            &types,
            typed.types(),
            false,
        )
        .unwrap()
        .validate()
        .unwrap();
        let ledger = source_collection_ledger(&owned).unwrap();
        let mut linked = 0;
        let mut empty = 0;
        for observation in owned.ownership().loan_action_observations() {
            for region in observation.regions() {
                if region.action_ids().len() == 0 {
                    assert_eq!(
                        ledger.link_literal_call(&owned, region.call()),
                        Err(CallableLoanLinkError::Unsupported)
                    );
                    empty += 1;
                } else {
                    assert_eq!(
                        ledger.link_literal_call(&owned, region.call()).unwrap(),
                        region.callable().unwrap()
                    );
                    linked += 1;
                }
            }
        }
        assert_eq!((linked, empty), (2, 1));
    }
}
#[test]
fn lambda_body_restores_outer_producer_and_intrinsic_is_not_a_region() {
    let text = "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"before\"); val f: () -> Unit = { val ignored = inspect(\"inside\") }; f(); inspect(\"after\"); val s = \"text\"; val n = s.clone() }";
    inspect_source(text, |owned| {
        let ledger = source_collection_ledger(owned).unwrap();
        let record = &ledger.records()[0];
        let original = record
            .loans()
            .iter()
            .zip(record.loan_producers().unwrap())
            .map(|(loan, producer)| {
                let source = loan.begin_span();
                (source, *producer)
            })
            .collect::<Vec<_>>();
        assert_eq!(original.len(), 4);
        assert!(matches!(original[0].1, Some(LoanProducer::Function(_))));
        assert!(matches!(original[1].1, Some(LoanProducer::Lambda(_))));
        assert_eq!(original[0].1, original[2].1);
        assert_eq!(original[0].1, original[3].1);
        for loan in record.loans() {
            assert_eq!(
                ledger.link_literal_call(owned, loan.call()),
                Err(CallableLoanLinkError::Unsupported)
            );
        }
    });
}
fn inspect_source(text: &str, check: impl FnOnce(&ValidatedCompilationUnitOwnership)) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("producers.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new(
        "root",
        "producers.ko",
        source,
        &parsed,
    )];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let owned = analysis::analyze(
        &sources,
        &inputs,
        names.names(),
        &types,
        typed.types(),
        false,
    )
    .unwrap()
    .validate()
    .unwrap();
    check(&owned);
}

#[test]
fn second_source_assignment_rollback_keeps_index_aligned_before_revocation() {
    for reverse in [false, true] {
        let mut sources = SourceMap::new();
        let a = sources
            .add_source(
                "a.ko",
                "fun inspect(value: String): Int = 1\nfun first(): Unit { inspect(\"prefix\") }",
            )
            .unwrap();
        // Ordinary immutable-assignment diagnostic after its RHS published a real call loan.
        let b = sources.add_source("b.ko", "fun second(): Unit { val target = 1; target = inspect(\"rolled\"); inspect(\"kept\") }").unwrap();
        let pa = parse_file(&sources, &lex(&sources, a).unwrap()).unwrap();
        let pb = parse_file(&sources, &lex(&sources, b).unwrap()).unwrap();
        let mut inputs = vec![
            SourceUnitInput::new("root", "a.ko", a, &pa),
            SourceUnitInput::new("root", "b.ko", b, &pb),
        ];
        if reverse {
            inputs.reverse();
        }
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let raw = analysis::analyze(
            &sources,
            &inputs,
            names.names(),
            &types,
            typed.types(),
            false,
        )
        .unwrap();
        assert!(
            raw.diagnostics()
                .iter()
                .any(|d| d.severity() == crate::diagnostic::Severity::Error
                    && d.message() == "assignment target is not a mutable place")
        );
        // Diagnostics revoke published unit facts, including the previously valid source.
        assert!(raw.loans().is_empty());
        assert!(raw.source_collection_ledger.is_none());
        assert!(raw.loan_action_observations().is_empty());
        // Real rollback checks the producer length against global_len - source_base.
        assert!(raw.validate().is_err());
    }
}
