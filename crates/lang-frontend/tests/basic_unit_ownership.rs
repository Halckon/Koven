//! SPEC-0252: compare pure basic advancement with the unchanged capability/checker chain.

use lang_frontend::{
    analysis::{
        UnitNameSnapshot, UnitSourceDescriptor, analyze_basic_unit_ownership, analyze_unit_names,
    },
    name_resolution::{SourceUnitInput, resolve_compilation_unit_names},
    ownership_checking::{
        OwnershipCheckingError, OwnershipDeferredReason, check_compilation_unit_ownership,
        owned_compilation_unit_view,
    },
    source::SourceMap,
    type_checking::{
        CompilationUnitTypes, TypeEnvironment, check_compilation_unit_types, standard_environments,
    },
};

const PROVIDER: &str = "package p\r\nclass Resource {}\r\nclass Holder(var payload: Resource)\r\nfun take(own value: Resource): Unit {}\r\nfun inspect(value: Resource): Int = 1";
const VALID: &str = "package q\r\nimport p.Resource\r\nimport p.take\r\nimport p.inspect as read\r\nfun use(own value: Resource): Unit { val count = /* 界é😀 */ read(value)\r\ntake(value) }";
const MOVED: &str = "package q\r\nimport p.Resource\r\nimport p.take\r\nfun use(own value: Resource): Unit { take(value)\r\n/* 界é😀 */ take(value) }";
const DEFERRED: &str = "package q\r\nimport p.Holder\r\nfun use(holders: List<Holder>): Unit { val projected = holders[0].payload }";

fn fixture(text: &str, reverse: bool) -> (UnitNameSnapshot, TypeEnvironment) {
    let mut sources = SourceMap::new();
    // Not a unit member: the adapter must not start scanning the full map.
    sources.add_source("unselected.ko", "# invalid").unwrap();
    let mut definitions = vec![
        ("z-consumer", "q/use.ko", text),
        ("a-provider", "p/api.ko", PROVIDER),
    ];
    if reverse {
        definitions.reverse();
    }
    let descriptors = definitions
        .into_iter()
        .map(|(root, path, text)| {
            let id = sources.add_source(path, text).unwrap();
            UnitSourceDescriptor::new(root, path, id)
        })
        .collect();
    let (ne, te) = standard_environments();
    let snapshot = analyze_unit_names(sources, descriptors, ne).unwrap();
    assert!(snapshot.names().diagnostics().is_empty());
    (snapshot, te)
}

fn typed(snapshot: &UnitNameSnapshot, environment: &TypeEnvironment) -> CompilationUnitTypes {
    check_compilation_unit_types(
        snapshot.sources(),
        &snapshot.inputs(),
        snapshot.validated_names().unwrap(),
        environment,
    )
    .unwrap()
}

fn assert_manual_parity(
    snapshot: &UnitNameSnapshot,
    environment: &TypeEnvironment,
    raw: CompilationUnitTypes,
) {
    let mut inputs = snapshot.inputs();
    inputs.reverse();
    let names = snapshot.validated_names().unwrap();
    let expected_typed = raw.clone().validate().unwrap();
    let expected_owned = check_compilation_unit_ownership(
        snapshot.sources(),
        &inputs,
        names,
        environment,
        &expected_typed,
    )
    .unwrap();
    let (actual_typed, actual_owned) =
        analyze_basic_unit_ownership(snapshot.sources(), &inputs, names, environment, raw)
            .unwrap()
            .into_result()
            .unwrap();
    assert_eq!(actual_typed, expected_typed);
    assert!(
        actual_typed
            .types()
            .is_same_analysis(expected_typed.types())
    );
    assert_eq!(
        actual_owned, expected_owned,
        "complete recovery facts, diagnostics, spans and deferred plans"
    );
    assert!(actual_owned.is_compatible_with(&actual_typed));
    assert!(actual_owned.is_compatible_with(&expected_typed));
    assert!(
        !actual_owned.is_same_analysis(&expected_owned),
        "two checker runs have separate ownership identities"
    );
}

#[test]
fn empty_and_multiroot_subset_preserve_complete_manual_facts_and_identity() {
    let (ne, te) = standard_environments();
    let empty = analyze_unit_names(SourceMap::new(), vec![], ne).unwrap();
    assert_manual_parity(&empty, &te, typed(&empty, &te));
    for reverse in [false, true] {
        let (snapshot, environment) = fixture(VALID, reverse);
        assert_manual_parity(&snapshot, &environment, typed(&snapshot, &environment));
    }
}

#[test]
fn raw_ownership_diagnostics_and_deferred_are_not_validated_or_discarded() {
    for (text, codes, deferred) in [(MOVED, vec!["L0131"], 0), (DEFERRED, vec![], 1)] {
        let (snapshot, environment) = fixture(text, false);
        let raw = typed(&snapshot, &environment);
        assert!(raw.diagnostics().is_empty());
        assert_manual_parity(&snapshot, &environment, raw.clone());
        let (typed, owned) = analyze_basic_unit_ownership(
            snapshot.sources(),
            &snapshot.inputs(),
            snapshot.validated_names().unwrap(),
            &environment,
            raw,
        )
        .unwrap()
        .into_result()
        .unwrap();
        assert_eq!(
            owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            codes
        );
        assert_eq!(owned.deferred().len(), deferred);
        if deferred != 0 {
            assert_eq!(
                owned.deferred()[0].reason(),
                OwnershipDeferredReason::IndexPlace
            );
        }
        assert!(owned.is_compatible_with(&typed));
        assert!(owned.validate().is_err());
    }
}

#[test]
fn not_basic_returns_original_recovery_and_constant_facts_without_advancing() {
    for text in [
        "package q\nfun bad(): Int = true",
        "package q\nconst val UNUSED = 42\nfun valid(): Unit {}",
        "package q\nconst val VALUE = 42\nfun read(): Int = VALUE",
    ] {
        let (snapshot, environment) = fixture(text, false);
        let raw = typed(&snapshot, &environment);
        let expected = raw.clone().validate().unwrap_err();
        let actual = analyze_basic_unit_ownership(
            snapshot.sources(),
            &snapshot.inputs(),
            snapshot.validated_names().unwrap(),
            &environment,
            raw,
        )
        .unwrap()
        .into_result()
        .unwrap_err();
        assert_eq!(actual, expected);
        assert!(actual.is_same_analysis(&expected));
        assert_eq!(
            actual.clone().validate_constants().is_ok(),
            expected.validate_constants().is_ok()
        );
    }
}

#[test]
fn not_basic_precedes_all_input_provenance_checks() {
    for text in [
        "package q\nfun bad(): Int = true",
        "package q\nconst val UNUSED = 42",
    ] {
        let (snapshot, environment) = fixture(text, false);
        let raw = typed(&snapshot, &environment);
        let expected = raw.clone();
        let (foreign, foreign_environment) = fixture(VALID, true);
        // Even all foreign context is ignored by the old validate-first NotBasic path.
        let actual = analyze_basic_unit_ownership(
            foreign.sources(),
            &foreign.inputs(),
            foreign.validated_names().unwrap(),
            &foreign_environment,
            raw,
        )
        .unwrap()
        .into_result()
        .unwrap_err();
        assert_eq!(*actual, expected);
        assert!(actual.is_same_analysis(&expected));
        let only_environment = analyze_basic_unit_ownership(
            snapshot.sources(),
            &snapshot.inputs(),
            snapshot.validated_names().unwrap(),
            &foreign_environment,
            expected.clone(),
        )
        .unwrap()
        .into_result()
        .unwrap_err();
        assert!(only_environment.is_same_analysis(&expected));
    }
}

#[test]
fn basic_provenance_rejections_keep_the_original_checker_error() {
    let (snapshot, environment) = fixture(VALID, false);
    let (foreign, foreign_environment) = fixture(VALID, false);
    let inputs = snapshot.inputs();
    let names = snapshot.validated_names().unwrap();
    let fresh_names = resolve_compilation_unit_names(
        snapshot.sources(),
        &inputs,
        snapshot.names().index(),
        snapshot.environment(),
    )
    .unwrap()
    .validate()
    .unwrap();
    assert_eq!(fresh_names, *names);
    let changed = [
        SourceUnitInput::new(
            "different-root",
            inputs[0].logical_path(),
            inputs[0].source_id(),
            inputs[0].parsed(),
        ),
        inputs[1],
    ];
    let duplicated = [inputs[0], inputs[0]];
    let foreign_inputs = foreign.inputs();
    let raw = typed(&snapshot, &environment);
    for (sources, inputs, names, environment) in [
        (
            snapshot.sources(),
            inputs.as_slice(),
            names,
            &foreign_environment,
        ),
        (
            snapshot.sources(),
            inputs.as_slice(),
            &fresh_names,
            &environment,
        ),
        (
            foreign.sources(),
            foreign_inputs.as_slice(),
            names,
            &environment,
        ),
        (snapshot.sources(), changed.as_slice(), names, &environment),
        (
            snapshot.sources(),
            duplicated.as_slice(),
            names,
            &environment,
        ),
    ] {
        let old = check_compilation_unit_ownership(
            sources,
            inputs,
            names,
            environment,
            &raw.clone().validate().unwrap(),
        )
        .unwrap_err();
        let new =
            match analyze_basic_unit_ownership(sources, inputs, names, environment, raw.clone()) {
                Err(error) => error,
                Ok(_) => panic!("foreign context was accepted"),
            };
        assert!(matches!(
            new,
            OwnershipCheckingError::MismatchedCompilationUnitTypes
        ));
        assert_eq!(format!("{new:?}"), format!("{old:?}"));
        assert_eq!(new.to_string(), old.to_string());
        assert!(std::error::Error::source(&new).is_none());
    }
}

#[test]
fn cloned_chain_and_rechecked_ownership_keep_view_identity_but_fresh_typed_does_not() {
    let (snapshot, environment) = fixture(VALID, false);
    let inputs = snapshot.inputs();
    let names = snapshot.validated_names().unwrap().clone();
    let environment = environment.clone();
    let raw = typed(&snapshot, &environment);
    let (first_typed, first_owned) = analyze_basic_unit_ownership(
        snapshot.sources(),
        &inputs,
        &names,
        &environment,
        raw.clone(),
    )
    .unwrap()
    .into_result()
    .unwrap();
    let (same_typed, same_owned) =
        analyze_basic_unit_ownership(snapshot.sources(), &inputs, &names, &environment, raw)
            .unwrap()
            .into_result()
            .unwrap();
    assert!(first_typed.types().is_same_analysis(same_typed.types()));
    assert!(!first_owned.is_same_analysis(&same_owned));
    assert_eq!(first_owned, same_owned);
    let first_owned = first_owned.validate().unwrap();
    let same_owned = same_owned.validate().unwrap();
    owned_compilation_unit_view(
        snapshot.sources(),
        &inputs,
        &names,
        &environment,
        &first_typed,
        &same_owned,
    )
    .unwrap();
    let fresh_typed = typed(&snapshot, &environment).validate().unwrap();
    assert_eq!(first_typed, fresh_typed);
    assert!(!first_typed.types().is_same_analysis(fresh_typed.types()));
    assert!(
        owned_compilation_unit_view(
            snapshot.sources(),
            &inputs,
            &names,
            &environment,
            &fresh_typed,
            &first_owned
        )
        .is_err()
    );
}
