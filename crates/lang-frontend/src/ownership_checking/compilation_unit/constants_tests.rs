//! Private driver acceptance: no public constant capability is published by this slice.
use super::{CompilationUnitOwnership, UnitDropTarget, UnitLoanTarget, analysis};
use crate::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

pub(super) fn analyze(text: &str) -> CompilationUnitOwnership {
    analyze_with_sources(text).1
}

pub(super) fn analyze_with_sources(text: &str) -> (SourceMap, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("a.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let inputs = [SourceUnitInput::new("root", "a/source.ko", source, &parsed)];
    let (ne, te) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &te)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned = analysis::analyze(&sources, &inputs, &names, &te, typed.types(), true).unwrap();
    (sources, owned)
}

#[test]
fn constant_recovery_cannot_publish_the_base_owned_capability() {
    let owned = analyze("package a\nconst val X = 7\nfun read(): Int = X");
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert!(
        owned.validate().is_err(),
        "constant analysis must not enter base native"
    );
}

#[test]
fn each_string_binary_read_has_its_own_temporary_drop() {
    for expression in ["S + S", "(S) + (S)", "A.S + A.S"] {
        let owned = analyze(&format!(
            "package a\nconst val S = \"s\"\nobject A {{ const val S = \"s\" }}\nfun read(): String = {expression}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        assert_eq!(owned.drops().len(), 2, "{expression}: {:?}", owned.drops());
        let targets = owned
            .drops()
            .iter()
            .map(|drop| drop.target())
            .collect::<Vec<_>>();
        assert!(
            targets
                .iter()
                .all(|target| matches!(target, UnitDropTarget::Temporary(_)))
        );
        assert_ne!(targets[0], targets[1], "reads cannot share an owner");
    }
}

#[test]
fn constant_borrows_and_value_deliveries_never_use_declaration_places() {
    let owned = analyze(
        "package a\nconst val S = \"s\"\nfun observe(s: String): Unit {}\nfun take(own s: String): Unit {}\nfun run(): Unit { val a = observe(S)\nval b = observe(S)\nval c = take(S)\nval d = take(S) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty());
    assert_eq!(owned.loans().len(), 2);
    assert!(
        owned
            .loans()
            .iter()
            .all(|loan| matches!(loan.target(), UnitLoanTarget::Temporary(_)))
    );
    assert_eq!(owned.value_deliveries().len(), 2);
    assert!(
        owned
            .value_deliveries()
            .iter()
            .all(|delivery| delivery.place().is_none())
    );
}

#[test]
fn constants_do_not_capture_declarations_or_namespaces() {
    let owned = analyze(
        "package a\nconst val X = 7\nobject A { const val X = 7 }\nfun run(): Unit { val f = { X + A.X }\nf() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.captures().is_empty());
    assert_eq!(owned.closures().len(), 1);
}

#[test]
fn groups_forward_the_same_materialization_owner_to_loans_deliveries_and_drops() {
    for value in ["TEXT", "A.TEXT"] {
        for depth in [1, 3] {
            let grouped = format!("{}{value}{}", "(".repeat(depth), ")".repeat(depth));
            let (sources, owned) = analyze_with_sources(&format!(
                "package a\nconst val TEXT = \"hi\"\nobject A {{ const val TEXT = \"hi\" }}\nfun view(text: String): Unit {{}}\nfun take(own text: String, own number: Int): Unit {{}}\nfun run(flag: Boolean): Unit {{ val first = view({grouped})\nval second = take({grouped}, if (flag) {{ return }} else {{ 0 }}) }}\nfun discard(): Unit {{ {grouped} }}"
            ));
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            assert!(owned.deferred().is_empty());
            let plans = owned.constant_materializations.as_ref().unwrap();
            assert_eq!(plans.len(), 3, "groups do not add materializations");
            let has_owner = |owner| {
                plans
                    .iter()
                    .any(|plan| plan.descriptor.expression() == owner)
            };
            assert_eq!(owned.loans().len(), 1);
            let UnitLoanTarget::Temporary(owner) = owned.loans()[0].target() else {
                panic!("constant loan must target a temporary")
            };
            assert!(
                has_owner(*owner),
                "{grouped}: loan must refer to the materialized owner"
            );
            let delivery = owned
                .value_deliveries()
                .iter()
                .find(|delivery| sources.slice(delivery.span()).unwrap() == grouped)
                .unwrap();
            let super::UnitValueDeliverySource::Temporary(owner) = delivery.source() else {
                panic!("constant delivery must use a temporary")
            };
            assert!(
                has_owner(*owner),
                "{grouped}: delivery must refer to the materialized owner"
            );
            let drops = owned
                .drops()
                .iter()
                .filter_map(|drop| {
                    if let UnitDropTarget::Temporary(owner) = drop.target() {
                        Some((drop, owner))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(
                drops.len(),
                3,
                "borrow, abandoned Value prefix, discarded expression"
            );
            let owners = drops
                .iter()
                .map(|(_, owner)| *owner)
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(owners.len(), 3, "each read retains its own owner");
            for (drop, owner) in drops {
                assert!(has_owner(owner), "{grouped}: {drop:?}");
                assert_eq!(sources.slice(drop.value_origin()).unwrap(), value);
            }
        }
    }
}
