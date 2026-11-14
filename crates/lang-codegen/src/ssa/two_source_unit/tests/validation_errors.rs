use super::*;

#[test]
fn ownership_validation_errors_are_not_unsupported_selection() {
    let text = source(
        false,
        "fun take(own value:String):Unit {}\nfun bad():Unit {val first=\"one\";val second=\"two\";take(first);take(second);consume(first);consume(second)}",
    );
    let mut sources = SourceMap::new();
    let id = sources.add_source("diagnostics.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let inputs = [SourceUnitInput::new("root", "diagnostics.ko", id, &parsed)];
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "main")
        .unwrap()
        .id();
    let TwoSourceAnalysisOutcome::ValidationRejected(expected) =
        analyze_two_source_outcome(&sources, &inputs, &names, &types, &typed).unwrap()
    else {
        panic!("actual ownership failure required");
    };
    assert_eq!(
        expected
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect::<Vec<_>>(),
        ["L0164", "L0131", "L0131"]
    );
    assert_eq!(
        expected
            .diagnostics()
            .iter()
            .filter(|d| d.code().to_string() == "L0131")
            .map(|d| sources.slice(d.primary_span()).unwrap())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    let Err(TwoSourceUnitError::OwnershipValidation(actual)) =
        prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry)
    else {
        panic!("must preserve validation failure, not unsupported subset");
    };
    assert_eq!(actual.diagnostics(), expected.diagnostics());
    assert_eq!(actual.deferred(), expected.deferred());
    assert!(
        lang_frontend::ownership_checking::two_source_experiment::analyze_two_source_experiment(
            &sources, &inputs, &names, &types, &typed
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn deferred_rejection_is_distinct_from_valid_unsupported_subset() {
    for deferred in [false, true] {
        let text = if deferred {
            source(
                false,
                "fun deferred():Unit {var callback:()->Int={1};val old=replace(&callback,{2})}",
            )
        } else {
            "fun main():Unit {}".to_owned()
        };
        let mut sources = SourceMap::new();
        let id = sources.add_source("outcome.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let inputs = [SourceUnitInput::new("root", "outcome.ko", id, &parsed)];
        let (env, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "main")
            .unwrap()
            .id();
        assert!(matches!(
            analyze_two_source_outcome(&sources, &[], &names, &types, &typed),
            Err(OwnershipCheckingError::MismatchedCompilationUnitTypes)
        ));
        assert!(matches!(
            prepare_two_source_unit(&sources, &[], &names, &types, &typed, entry),
            Err(TwoSourceUnitError::Ownership(
                OwnershipCheckingError::MismatchedCompilationUnitTypes
            ))
        ));
        let outcome =
            analyze_two_source_outcome(&sources, &inputs, &names, &types, &typed).unwrap();
        let prepared = prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry);
        if deferred {
            let TwoSourceAnalysisOutcome::ValidationRejected(expected) = outcome else {
                panic!("real deferred validation must fail")
            };
            assert!(
                expected.diagnostics().is_empty(),
                "{:?}",
                expected.diagnostics()
            );
            let [deferred] = expected.deferred() else {
                panic!("one real closure replacement boundary")
            };
            assert_eq!(deferred.reason(), lang_frontend::ownership_checking::OwnershipDeferredReason::OwnershipPrimitiveClosureTransport);
            let node = parsed
                .ast()
                .expressions()
                .get(deferred.expression().expression())
                .unwrap();
            assert_eq!(
                sources.slice(node.span()).unwrap(),
                "replace(&callback,{2})"
            );
            let Err(TwoSourceUnitError::OwnershipValidation(actual)) = prepared else {
                panic!("do not flatten deferred to UnsupportedSelection")
            };
            assert_eq!(actual.deferred(), expected.deferred());
            assert_eq!(actual.diagnostics(), expected.diagnostics());
        } else {
            assert!(matches!(
                outcome,
                TwoSourceAnalysisOutcome::UnsupportedSubset
            ));
            assert!(matches!(
                prepared,
                Err(TwoSourceUnitError::UnsupportedSelection)
            ));
        }
    }
}
