//! Original single-source materialization contracts through both Unit entries.
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SourceUnitKey, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        ConstEnabledOwnedUnit, ConstantMaterializationKind, UnitDropTarget, UnitLoanTarget,
        check_compilation_unit_constant_ownership,
    },
    source::SourceMap,
    type_checking::{ConstEnabledTypedUnit, check_compilation_unit_types, standard_environments},
};

pub(super) fn checked(
    text: &str,
    rooted: bool,
) -> (SourceMap, ConstEnabledTypedUnit, ConstEnabledOwnedUnit) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("constants.ko", text).unwrap();
    let parsed = super::parser_test_assertions::parse_file_twice(
        &sources,
        source,
        "constant ownership Unit",
    );
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let input = if rooted {
        SourceUnitInput::new("root", "constants.ko", source, &parsed)
    } else {
        SourceUnitInput::memory_document(source, &parsed)
    };
    let inputs = [input];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    assert!(index.diagnostics().is_empty());
    let [entry] = index.source_units() else {
        panic!("one original source")
    };
    assert_eq!(entry.source_id(), source);
    match (rooted, entry.key()) {
        (false, SourceUnitKey::MemoryDocument) => {}
        (true, SourceUnitKey::Rooted { root, logical_path }) => {
            assert_eq!(root.as_str(), "root");
            assert_eq!(logical_path.as_str(), "constants.ko");
        }
        _ => panic!("original entry identity"),
    }
    let (environment, types) = standard_environments();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let typed = typed.validate_constants().unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap();
    assert!(
        owned.ownership().diagnostics().is_empty(),
        "{:?}",
        owned.ownership().diagnostics()
    );
    assert!(
        owned.ownership().deferred().is_empty(),
        "{:?}",
        owned.ownership().deferred()
    );
    assert!(owned.is_compatible_with(&typed));
    let owned = owned.validate().unwrap();
    (sources, typed, owned)
}

pub(super) fn scalar(text: &str) {
    for rooted in [false, true] {
        let (_, _, owned) = checked(text, rooted);
        assert_eq!(
            owned.materializations().len(),
            2,
            "group must not rematerialize"
        );
        assert!(
            owned
                .materializations()
                .iter()
                .all(|p| p.kind() == ConstantMaterializationKind::InlineCopy)
        );
        assert!(owned.ownership().loans().is_empty());
        assert!(owned.ownership().drops().is_empty());
        assert!(owned.ownership().captures().is_empty());
    }
}

pub(super) fn identity(text: &str) {
    for rooted in [false, true] {
        let (sources, typed, owned) = checked(text, rooted);
        assert!(owned.is_compatible_with(&typed));
        assert_eq!(owned.materializations().len(), 3);
        assert_eq!(owned.ownership().drops().len(), 2);
        let (_, other, repeated) = checked(text, rooted);
        assert!(!owned.is_compatible_with(&other));
        let snapshot = |owned: &ConstEnabledOwnedUnit| {
            owned
                .materializations()
                .iter()
                .map(|plan| {
                    let d = plan.descriptor();
                    (
                        d.expression().expression().index(),
                        d.target().symbol().index(),
                        d.value().clone(),
                        plan.kind(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(snapshot(&owned), snapshot(&repeated));
        for (plan, kind) in owned.materializations().iter().zip([
            ConstantMaterializationKind::StringTemporary,
            ConstantMaterializationKind::StringTemporary,
            ConstantMaterializationKind::InlineCopy,
        ]) {
            assert_eq!(plan.kind(), kind);
            assert_eq!(
                owned
                    .materialization_at(plan.descriptor().expression())
                    .unwrap()
                    .descriptor()
                    .target(),
                plan.descriptor().target()
            );
        }
        assert_eq!(
            sources
                .slice(owned.ownership().drops()[0].value_origin())
                .unwrap(),
            "COPY"
        );
    }
}

pub(super) fn grouped(text: &str) {
    for rooted in [false, true] {
        let (_, _, owned) = checked(text, rooted);
        assert_eq!(owned.materializations().len(), 3);
        assert_eq!(owned.ownership().loans().len(), 1);
        for loan in owned.ownership().loans() {
            if let UnitLoanTarget::Temporary(owner) = loan.target() {
                assert!(owned.materialization_at(*owner).is_some(), "{loan:?}");
            }
        }
        let temporaries = owned
            .ownership()
            .drops()
            .iter()
            .filter_map(|drop| match drop.target() {
                UnitDropTarget::Temporary(owner) => Some(owner),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(temporaries.len(), 3);
        for owner in temporaries {
            assert!(owned.materialization_at(owner).is_some(), "{owner:?}");
        }
    }
}

#[test]
fn loop_successor_cleanup_requires_a_reachable_break() {
    for (body, reaches_tail) in [
        ("loop { continue }", false),
        ("loop { break }", true),
        ("loop { loop { break }; continue }", false),
        ("loop { loop { continue }; break }", false),
        ("loop { loop { break }; break }", true),
        ("loop { if (flag) { break } else { continue } }", true),
    ] {
        let text = format!(
            "const val TEXT = \"hi\"\nfun view(text: String): Unit {{}}\nfun run(flag: Boolean): Unit {{ {body}; val tail = view(TEXT) }}"
        );
        for rooted in [false, true] {
            let (sources, _, owned) = checked(&text, rooted);
            let expected = usize::from(reaches_tail);
            assert_eq!(owned.materializations().len(), expected, "{body}");
            assert_eq!(owned.ownership().drops().len(), expected, "{body}");
            for drop in owned.ownership().drops() {
                let UnitDropTarget::Temporary(expression) = drop.target() else {
                    panic!("tail String owner")
                };
                assert!(owned.materialization_at(expression).is_some());
                assert_eq!(sources.slice(drop.value_origin()), Ok("TEXT"));
            }
        }
    }
}
