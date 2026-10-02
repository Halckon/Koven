//! SPEC-0250：旧手工前缀与封闭 owner 的全事实、来源、恢复和输入合同。

use lang_frontend::{
    analysis::{UnitNameAnalysisError, UnitNameSnapshot, UnitSourceDescriptor, analyze_unit_names},
    lexer::{LexerInternalError, lex},
    name_resolution::{
        CompilationUnitInputError, CompilationUnitNames, LogicalPathError, NameEnvironment,
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
    parser::{ParsedFile, parse_file},
    source::{SourceError, SourceId, SourceMap},
    type_checking::{
        CompilationUnitTypeError, check_compilation_unit_types, standard_environments,
    },
};

fn manual_prefix(
    sources: &SourceMap,
    descriptors: &[UnitSourceDescriptor],
    environment: &NameEnvironment,
) -> (Vec<ParsedFile>, CompilationUnitNames) {
    let parsed = descriptors
        .iter()
        .map(|unit| {
            let lexed = lex(sources, unit.source_id()).expect("manual lex");
            parse_file(sources, &lexed).expect("manual parse")
        })
        .collect::<Vec<_>>();
    let inputs = descriptors
        .iter()
        .zip(&parsed)
        .map(|(unit, parsed)| {
            SourceUnitInput::new(
                unit.root_identity(),
                unit.logical_path(),
                unit.source_id(),
                parsed,
            )
        })
        .collect::<Vec<_>>();
    let index = index_compilation_unit(sources, &inputs).expect("manual index");
    let names = resolve_compilation_unit_names(sources, &inputs, &index, environment)
        .expect("manual names");
    (parsed, names)
}

fn paired(
    sources: SourceMap,
    descriptors: Vec<UnitSourceDescriptor>,
    environment: NameEnvironment,
) -> UnitNameSnapshot {
    let (parsed, expected) = manual_prefix(&sources, &descriptors, &environment);
    let snapshot = analyze_unit_names(sources, descriptors, environment).expect("snapshot prefix");
    // Full derived equality covers every public name/index fact and every diagnostic field,
    // including code/severity/message, primary identity/range, labels, notes, and ordering.
    assert_eq!(snapshot.names(), &expected);
    let inputs = snapshot.inputs();
    assert_eq!(inputs.len(), parsed.len());
    assert_eq!(snapshot.descriptors().len(), parsed.len());
    assert_eq!(snapshot.parsed_files().len(), parsed.len());
    for ((input, unit), actual) in inputs
        .iter()
        .zip(snapshot.names().index().source_units())
        .zip(snapshot.parsed_files())
    {
        assert_eq!(input.source_id(), unit.source_id());
        assert_eq!(input.root_identity(), unit.key().root().as_str());
        assert_eq!(input.logical_path(), unit.key().logical_path().as_str());
        assert_eq!(actual.source_id(), unit.source_id());
        let old = parsed
            .iter()
            .find(|old| old.source_id() == unit.source_id())
            .unwrap();
        assert_eq!(actual.package(), old.package());
        assert_eq!(actual.imports(), old.imports());
        assert_eq!(actual.roots(), old.roots());
        assert_eq!(actual.diagnostics(), old.diagnostics());
        assert_eq!(
            format!("{:?}", actual.ast()),
            format!("{:?}", old.ast()),
            "canonical sorting must move the matching whole AST"
        );
        assert!(std::ptr::eq(input.parsed(), actual));
    }
    snapshot
}

// Cross-map presentation normalizes all SourceId occurrences to the semantic source key.
// Same-map equality above remains the stronger source-identity check; Debug hides map owners.
fn normalized_names(snapshot: &UnitNameSnapshot) -> String {
    let mut result = format!("{:?}", snapshot.names());
    for unit in snapshot.names().index().source_units() {
        result = result.replace(
            &format!("{:?}", unit.source_id()),
            &format!(
                "SourceKey({:?},{:?})",
                unit.key().root().as_str(),
                unit.key().logical_path().as_str()
            ),
        );
    }
    result
}

#[test]
fn empty_unit_and_explicit_subset_match_the_manual_prefix() {
    let empty = paired(SourceMap::new(), vec![], NameEnvironment::new());
    assert!(empty.validated_names().is_some());
    assert!(empty.inputs().is_empty());
    assert!(empty.names().index().source_units().is_empty());

    let mut sources = SourceMap::new();
    let excluded = sources
        .add_source("excluded", "@\nval broken = absent")
        .unwrap();
    let included = sources.add_source("included", "val present = 1").unwrap();
    let snapshot = paired(
        sources,
        vec![UnitSourceDescriptor::new("root", "present.ko", included)],
        NameEnvironment::new(),
    );
    assert_eq!(
        snapshot.sources().source_text(excluded).unwrap(),
        "@\nval broken = absent"
    );
    assert_eq!(snapshot.names().index().source_units().len(), 1);
    assert_eq!(
        snapshot.names().index().source_units()[0].source_id(),
        included
    );
    assert!(snapshot.names().diagnostics().is_empty());
    assert!(snapshot.validated_names().is_some());
}

#[test]
fn registration_and_descriptor_permutations_preserve_all_name_facts() {
    let fixtures = [
        (
            "z-presentation",
            "a-root",
            "lib/first.ko",
            "package lib\npublic class Widget\npublic fun merge(x: Int): Unit {}\npublic val shared = \"界é\"",
        ),
        (
            "m-presentation",
            "z-root",
            "lib/second.ko",
            "package lib\npublic fun merge(x: String): Unit {}\nfun same(): Unit { val x = shared\nmerge(1) }",
        ),
        (
            "a-presentation",
            "z-root",
            "app/main.ko",
            "package app\nimport lib.Widget as W\nimport lib.merge\nfun use(x: W): W = W()\nfun call(): Unit { merge(\"界\") }",
        ),
    ];
    let mut expected = None;
    for registration in [[0, 1, 2], [2, 1, 0]] {
        for order in [[0, 1, 2], [2, 0, 1], [1, 2, 0]] {
            let mut sources = SourceMap::new();
            let mut ids = [None; 3];
            for index in registration {
                ids[index] = Some(
                    sources
                        .add_source(fixtures[index].0, fixtures[index].3)
                        .unwrap(),
                );
            }
            let units = order
                .into_iter()
                .map(|index| {
                    UnitSourceDescriptor::new(
                        fixtures[index].1,
                        fixtures[index].2,
                        ids[index].unwrap(),
                    )
                })
                .collect();
            let (environment, _) = standard_environments();
            let snapshot = paired(sources, units, environment);
            assert!(
                snapshot.names().diagnostics().is_empty(),
                "{:?}",
                snapshot.names().diagnostics()
            );
            assert!(snapshot.validated_names().is_some());
            assert_eq!(snapshot.names().index().declarations().len(), 7);
            let normalized = normalized_names(&snapshot);
            if let Some(expected) = &expected {
                assert_eq!(&normalized, expected);
            } else {
                expected = Some(normalized);
            }
        }
    }
}

#[test]
fn recovery_keeps_complete_mixed_diagnostics_once_and_stops_after_names() {
    let mut sources = SourceMap::new();
    let text =
        "// 界é\n#\nval first = absent\nval duplicate = 1\nval duplicate = 2\nval syntax =\n";
    let source = sources.add_source("z-display", text).unwrap();
    let later = sources.add_source("a-display", "fun typeBad(): Int = true\nclass Handle\nfun own(own x: Handle): Unit {}\nfun bad(): Unit { val x = Handle()\nown(x)\nown(x) }").unwrap();
    let (environment, _) = standard_environments();
    let snapshot = paired(
        sources,
        vec![
            UnitSourceDescriptor::new("z-root", "later.ko", later),
            UnitSourceDescriptor::new("a-root", "mixed.ko", source),
        ],
        environment,
    );
    assert!(snapshot.validated_names().is_none());
    let diagnostics = snapshot.names().diagnostics();
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect::<Vec<_>>();
    assert_eq!(codes, ["L0001", "L0080", "L0079", "L0009"]);
    let spans = [
        (text.find('#').unwrap(), 1),
        (text.find("absent").unwrap(), 6),
        (text.rfind("duplicate").unwrap(), 9),
        (text.len(), 0),
    ];
    for (diagnostic, (start, len)) in diagnostics.iter().zip(spans) {
        let span = diagnostic.primary_span();
        assert_eq!(span.source_id(), source);
        assert_eq!((span.start(), span.end()), (start, start + len));
    }
    assert!(
        !diagnostics[2].details().is_empty(),
        "duplicate must retain related declaration"
    );
}

#[test]
fn moved_snapshot_preserves_source_identity_and_the_matched_environment() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("original", "fun entry(): Unit { println(\"界\") }")
        .unwrap();
    let span = sources.span(source, 0, 3).unwrap();
    let (environment, types) = standard_environments();
    let snapshot = paired(
        sources,
        vec![UnitSourceDescriptor::new("root", "main.ko", source)],
        environment,
    );
    let snapshot = Box::new(snapshot);
    assert_eq!(snapshot.sources().slice(span).unwrap(), "fun");
    assert_eq!(
        snapshot.sources().source_text(source).unwrap(),
        "fun entry(): Unit { println(\"界\") }"
    );
    let inputs = snapshot.inputs();
    let names = snapshot.validated_names().unwrap();
    let typed = check_compilation_unit_types(snapshot.sources(), &inputs, names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let owned =
        check_compilation_unit_ownership(snapshot.sources(), &inputs, names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
    let view =
        owned_compilation_unit_view(snapshot.sources(), &inputs, names, &types, &typed, &owned)
            .unwrap();
    assert!(std::ptr::eq(view.inputs(), inputs.as_slice()));
    assert!(std::ptr::eq(view.sources(), snapshot.sources()));
    let rebuilt = resolve_compilation_unit_names(
        snapshot.sources(),
        &inputs,
        snapshot.names().index(),
        snapshot.environment(),
    )
    .unwrap();
    assert_eq!(&rebuilt, snapshot.names());
    let (_, foreign_types) = standard_environments();
    assert!(matches!(
        check_compilation_unit_types(snapshot.sources(), &inputs, names, &foreign_types),
        Err(CompilationUnitTypeError::MismatchedNameEnvironment)
    ));
}

fn foreign_error(error: UnitNameAnalysisError, expected: SourceId) {
    let inner = LexerInternalError::Source(SourceError::InvalidSourceId {
        source_id: expected,
    });
    assert_eq!(error.to_string(), inner.to_string());
    assert_eq!(
        std::error::Error::source(&error).unwrap().to_string(),
        inner.to_string()
    );
    match error {
        UnitNameAnalysisError::Lexer(LexerInternalError::Source(
            SourceError::InvalidSourceId { source_id },
        )) => assert_eq!(source_id, expected),
        other => panic!("expected lexer foreign source {expected:?}, got {other:?}"),
    }
}

#[test]
fn foreign_source_precedes_invalid_path_in_both_descriptor_orders() {
    for reverse in [false, true] {
        let mut sources = SourceMap::new();
        let valid = sources.add_source("valid", "val ok = 1").unwrap();
        let mut foreign = SourceMap::new();
        let invalid = foreign.add_source("foreign", "val no = 2").unwrap();
        let mut units = vec![
            UnitSourceDescriptor::new("a", "../bad.ko", valid),
            UnitSourceDescriptor::new("z", "valid.ko", invalid),
        ];
        if reverse {
            units.reverse();
        }
        foreign_error(
            analyze_unit_names(sources, units, NameEnvironment::new())
                .err()
                .expect("invalid descriptor rejected"),
            invalid,
        );
    }
}

#[test]
fn multiple_foreign_sources_follow_descriptor_order_before_index() {
    for reverse in [false, true] {
        let mut foreign = SourceMap::new();
        let first = foreign.add_source("first", "").unwrap();
        let second = foreign.add_source("second", "").unwrap();
        let mut units = vec![
            UnitSourceDescriptor::new("z", "later.ko", first),
            UnitSourceDescriptor::new("a", "../invalid.ko", second),
        ];
        if reverse {
            units.reverse();
        }
        foreign_error(
            analyze_unit_names(SourceMap::new(), units, NameEnvironment::new())
                .err()
                .expect("invalid descriptor rejected"),
            if reverse { second } else { first },
        );
    }
}

#[test]
fn valid_source_ids_keep_index_path_and_duplicate_classification() {
    for kind in 0..3 {
        let mut sources = SourceMap::new();
        let first = sources.add_source("first", "val first = 1").unwrap();
        let second = sources.add_source("second", "val second = 2").unwrap();
        let units = match kind {
            0 => vec![UnitSourceDescriptor::new("root", "../bad.ko", first)],
            1 => vec![
                UnitSourceDescriptor::new("root", "same.ko", first),
                UnitSourceDescriptor::new("root", "same.ko", second),
            ],
            _ => vec![
                UnitSourceDescriptor::new("root", "first.ko", first),
                UnitSourceDescriptor::new("root", "second.ko", first),
            ],
        };
        let error = analyze_unit_names(sources, units, NameEnvironment::new())
            .err()
            .expect("invalid descriptor rejected");
        match (kind, error) {
            (
                0,
                UnitNameAnalysisError::Input(CompilationUnitInputError::InvalidLogicalPath {
                    path,
                    reason,
                }),
            ) => {
                assert_eq!(path, "../bad.ko");
                assert_eq!(reason, LogicalPathError::ParentSegment);
            }
            (
                1,
                UnitNameAnalysisError::Input(CompilationUnitInputError::DuplicateSourceKey {
                    root,
                    logical_path,
                }),
            ) => {
                assert_eq!(root, "root");
                assert_eq!(logical_path, "same.ko");
            }
            (
                2,
                UnitNameAnalysisError::Input(CompilationUnitInputError::DuplicateSource {
                    source_id,
                }),
            ) => assert_eq!(source_id, first),
            (_, other) => panic!("unexpected classification for {kind}: {other:?}"),
        }
    }
}
