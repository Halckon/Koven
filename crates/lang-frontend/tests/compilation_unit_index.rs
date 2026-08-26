//! SPEC-0025 Stage 1 compilation-unit identity 与 package/declaration index 契约。

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail, Severity, codes as diagnostic_codes},
    lexer::lex,
    name_resolution::{
        CompilationUnitInputError, DeclarationVisibility, Namespace, SourceUnitInput, SymbolKind,
        UnitDiagnosticOrderError, index_compilation_unit, ordered_unit_diagnostics,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
};

fn parsed_source(
    sources: &mut SourceMap,
    presentation_name: &str,
    text: &str,
) -> (SourceId, ParsedFile) {
    let source_id = sources
        .add_source(presentation_name, text)
        .expect("test source names are unique");
    let lexed = lex(sources, source_id).expect("test source lexes without an internal failure");
    let parsed =
        parse_file(sources, &lexed).expect("test source parses without an internal failure");
    (source_id, parsed)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn canonical_identity_is_input_order_independent_and_merges_roots_by_package() {
    let mut sources = SourceMap::new();
    let (models_a_id, models_a) = parsed_source(
        &mut sources,
        "models-a.ko",
        "package models\npublic class Alpha",
    );
    let (models_b_id, models_b) = parsed_source(
        &mut sources,
        "models-b.ko",
        "package models\ninternal object Shared",
    );
    let (default_id, default) = parsed_source(&mut sources, "default.ko", "private val local = 1");

    let forward = [
        SourceUnitInput::new("root-a", "models/alpha.ko", models_a_id, &models_a),
        SourceUnitInput::new("root-b", "models/shared.ko", models_b_id, &models_b),
        SourceUnitInput::new("root-z", "default.ko", default_id, &default),
    ];
    let reverse = [forward[2], forward[1], forward[0]];

    let forward = index_compilation_unit(&sources, &forward).expect("valid unit");
    let reverse = index_compilation_unit(&sources, &reverse).expect("valid reversed unit");

    assert_eq!(forward, reverse);
    assert_eq!(forward.packages().len(), 2);
    assert!(forward.packages()[0].name().is_default());
    assert_eq!(forward.packages()[1].name().segments(), ["models"]);
    assert_eq!(forward.source_units().len(), 3);
    assert_eq!(
        forward.source_units()[0].key().logical_path().as_str(),
        "models/alpha.ko"
    );
    assert_eq!(
        forward.source_units()[1].key().logical_path().as_str(),
        "models/shared.ko"
    );
    assert_eq!(
        forward.source_units()[0].package(),
        forward.source_units()[1].package(),
        "different roots must contribute to one structural package"
    );
    assert!(forward.validate().is_ok());
}

#[test]
fn collector_maps_top_level_kinds_namespaces_and_visibility() {
    let mut sources = SourceMap::new();
    let (source_id, parsed) = parsed_source(
        &mut sources,
        "declarations.ko",
        "public val visible = 1\n\
         internal const val VERSION = 1\n\
         private fun hidden() {}\n\
         class Plain\n\
         object Singleton",
    );
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "declarations.ko",
        source_id,
        &parsed,
    )];

    let index = index_compilation_unit(&sources, &inputs).expect("valid unit");
    let declarations = index.declarations();
    assert_eq!(declarations.len(), 6);
    let facts: Vec<_> = declarations
        .iter()
        .map(|declaration| {
            (
                declaration.name(),
                declaration.namespace(),
                declaration.kind(),
                declaration.visibility(),
            )
        })
        .collect();
    assert_eq!(
        facts,
        [
            (
                "visible",
                Namespace::Value,
                SymbolKind::Variable,
                DeclarationVisibility::Public,
            ),
            (
                "VERSION",
                Namespace::Value,
                SymbolKind::Constant,
                DeclarationVisibility::Internal,
            ),
            (
                "hidden",
                Namespace::Value,
                SymbolKind::Function,
                DeclarationVisibility::Private,
            ),
            (
                "Plain",
                Namespace::Type,
                SymbolKind::Classifier,
                DeclarationVisibility::Public,
            ),
            (
                "Singleton",
                Namespace::Type,
                SymbolKind::Classifier,
                DeclarationVisibility::Public,
            ),
            (
                "Singleton",
                Namespace::Value,
                SymbolKind::ObjectValue,
                DeclarationVisibility::Public,
            ),
        ]
    );
    for (expected, declaration) in declarations.iter().enumerate() {
        assert_eq!(declaration.id().index(), expected);
    }
}

#[test]
fn invalid_unit_inputs_fail_before_indexing() {
    let mut sources = SourceMap::new();
    let (first_id, first) = parsed_source(&mut sources, "first.ko", "val first = 1");
    let (second_id, second) = parsed_source(&mut sources, "second.ko", "val second = 2");

    let invalid_paths = ["", "/absolute.ko", "a//b.ko", "a/./b.ko", "a/../b.ko"];
    for path in invalid_paths {
        let inputs = [SourceUnitInput::new("root", path, first_id, &first)];
        assert!(
            matches!(
                index_compilation_unit(&sources, &inputs),
                Err(CompilationUnitInputError::InvalidLogicalPath { .. })
            ),
            "path {path:?} must be rejected"
        );
    }

    let duplicate_source = [
        SourceUnitInput::new("root", "first.ko", first_id, &first),
        SourceUnitInput::new("root", "renamed.ko", first_id, &first),
    ];
    assert!(matches!(
        index_compilation_unit(&sources, &duplicate_source),
        Err(CompilationUnitInputError::DuplicateSource { .. })
    ));

    let duplicate_key = [
        SourceUnitInput::new("root", "same.ko", first_id, &first),
        SourceUnitInput::new("root", "same.ko", second_id, &second),
    ];
    assert!(matches!(
        index_compilation_unit(&sources, &duplicate_key),
        Err(CompilationUnitInputError::DuplicateSourceKey { .. })
    ));

    let mismatched = [SourceUnitInput::new("root", "first.ko", second_id, &first)];
    assert!(matches!(
        index_compilation_unit(&sources, &mismatched),
        Err(CompilationUnitInputError::MismatchedParsedSource { .. })
    ));

    let mut foreign_sources = SourceMap::new();
    let foreign_id = foreign_sources
        .add_source("foreign.ko", "val foreign = 1")
        .expect("unique foreign source");
    let foreign = [SourceUnitInput::new(
        "root",
        "foreign.ko",
        foreign_id,
        &first,
    )];
    assert!(matches!(
        index_compilation_unit(&sources, &foreign),
        Err(CompilationUnitInputError::Source(_))
    ));
}

#[test]
fn invalid_input_selection_is_independent_of_caller_order() {
    let mut sources = SourceMap::new();
    let (first_id, first) = parsed_source(&mut sources, "first.ko", "val first = 1");
    let (second_id, second) = parsed_source(&mut sources, "second.ko", "val second = 2");
    let forward = [
        SourceUnitInput::new("root-a", "/absolute.ko", first_id, &first),
        SourceUnitInput::new("root-b", "bad//path.ko", second_id, &second),
    ];
    let reverse = [forward[1], forward[0]];

    let forward_error = index_compilation_unit(&sources, &forward).expect_err("invalid unit");
    let reverse_error = index_compilation_unit(&sources, &reverse).expect_err("invalid unit");

    assert!(matches!(
        forward_error,
        CompilationUnitInputError::InvalidLogicalPath { .. }
    ));
    assert!(matches!(
        reverse_error,
        CompilationUnitInputError::InvalidLogicalPath { .. }
    ));
    assert_eq!(forward_error.to_string(), reverse_error.to_string());
    assert!(forward_error.to_string().contains("/absolute.ko"));
}

#[test]
fn logical_package_directories_are_identifiers_and_root_identity_is_opaque() {
    let mut sources = SourceMap::new();
    let (source_id, parsed) = parsed_source(&mut sources, "source.ko", "val item = 1");

    for logical_path in [
        "class/source.ko",
        "async/source.ko",
        "bad-name/source.ko",
        "9bad/source.ko",
    ] {
        let input = [SourceUnitInput::new(
            "root",
            logical_path,
            source_id,
            &parsed,
        )];
        assert!(
            matches!(
                index_compilation_unit(&sources, &input),
                Err(CompilationUnitInputError::InvalidLogicalPath { .. })
            ),
            "logical parent {logical_path:?} cannot denote a Koven package"
        );
    }

    let empty_root = [SourceUnitInput::new("", "source.ko", source_id, &parsed)];
    assert!(
        index_compilation_unit(&sources, &empty_root)
            .expect("root identity is an opaque stable key")
            .validate()
            .is_ok()
    );
}

#[test]
fn cross_file_functions_merge_but_non_function_collisions_report_l0147() {
    let mut sources = SourceMap::new();
    let (first_id, first) = parsed_source(
        &mut sources,
        "first.ko",
        "package shared\n\
         fun overload(x: Int): Int = x\n\
         val collision = 1\n\
         class Duplicate",
    );
    let (second_id, second) = parsed_source(
        &mut sources,
        "second.ko",
        "package shared\n\
         fun overload(x: Int, y: Int): Int = x\n\
         fun collision(): Unit {}\n\
         class Duplicate",
    );
    let inputs = [
        SourceUnitInput::new("a", "shared/first.ko", first_id, &first),
        SourceUnitInput::new("b", "shared/second.ko", second_id, &second),
    ];

    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
    assert_eq!(
        index
            .declarations()
            .iter()
            .filter(|declaration| declaration.name() == "overload")
            .count(),
        2
    );
    assert_eq!(codes(index.diagnostics()), ["L0147", "L0147"]);
    assert!(index.clone().validate().is_err());
    let primary_names: Vec<_> = index
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            sources
                .slice(diagnostic.primary_span())
                .expect("valid diagnostic span")
        })
        .collect();
    assert_eq!(primary_names, ["collision", "Duplicate"]);

    let DiagnosticDetail::Label(label) = &index.diagnostics()[0].details()[0] else {
        panic!("L0147 must label the first conflicting declaration");
    };
    assert_eq!(label.span().source_id(), first_id);
    assert_eq!(
        sources.slice(label.span()).expect("valid label span"),
        "collision"
    );
    assert_eq!(label.message(), "first declaration with this package name");
}

#[test]
fn same_file_function_value_conflict_prevents_cross_file_function_merge() {
    let mut sources = SourceMap::new();
    let (first_id, first) = parsed_source(
        &mut sources,
        "first.ko",
        "package shared\n\
         fun mixed(): Unit {}\n\
         val mixed = 1",
    );
    let (second_id, second) = parsed_source(
        &mut sources,
        "second.ko",
        "package shared\nfun mixed(arg: Int): Int = arg",
    );
    let inputs = [
        SourceUnitInput::new("a", "shared/first.ko", first_id, &first),
        SourceUnitInput::new("b", "shared/second.ko", second_id, &second),
    ];

    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");

    assert_eq!(codes(index.diagnostics()), ["L0147"]);
    assert_eq!(index.diagnostics()[0].primary_span().source_id(), second_id);
    assert_eq!(
        sources
            .slice(index.diagnostics()[0].primary_span())
            .expect("valid conflict primary"),
        "mixed"
    );
    let DiagnosticDetail::Label(label) = &index.diagnostics()[0].details()[0] else {
        panic!("cross-file conflict must label the non-function declaration");
    };
    assert_eq!(label.span().source_id(), first_id);
    assert_eq!(sources.slice(label.span()).expect("valid label"), "mixed");
    assert!(index.validate().is_err());
}

#[test]
fn unit_diagnostics_follow_stable_keys_not_input_or_presentation_order() {
    let mut sources = SourceMap::new();
    let (stable_first_id, stable_first) = parsed_source(
        &mut sources,
        "z-presentation.ko",
        "package wrong\nval collision = 1",
    );
    let (stable_second_id, stable_second) = parsed_source(
        &mut sources,
        "a-presentation.ko",
        "package shared\nval collision = 2",
    );
    let forward = [
        SourceUnitInput::new("root-a", "shared/first.ko", stable_first_id, &stable_first),
        SourceUnitInput::new(
            "root-z",
            "shared/second.ko",
            stable_second_id,
            &stable_second,
        ),
    ];
    let reverse = [forward[1], forward[0]];

    let forward = index_compilation_unit(&sources, &forward).expect("valid unit input");
    let reverse = index_compilation_unit(&sources, &reverse).expect("valid reversed input");

    assert_eq!(forward, reverse);
    assert_eq!(codes(forward.diagnostics()), ["L0146", "L0147"]);
    assert_eq!(
        forward.diagnostics()[0].primary_span().source_id(),
        stable_first_id,
        "root-a sorts before root-z even though its presentation name sorts later"
    );
    assert_eq!(
        forward.diagnostics()[1].primary_span().source_id(),
        stable_second_id
    );
    assert!(forward.validate().is_err());
}

#[test]
fn package_mismatch_reports_l0146_with_source_anchored_primary_spans() {
    let mut sources = SourceMap::new();
    let (declared_id, declared) =
        parsed_source(&mut sources, "declared.ko", "package wrong\nval item = 1");
    let (omitted_id, omitted) = parsed_source(&mut sources, "omitted.ko", "val item = 1");
    let inputs = [
        SourceUnitInput::new("a", "expected/declared.ko", declared_id, &declared),
        SourceUnitInput::new("b", "nested/omitted.ko", omitted_id, &omitted),
    ];

    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
    assert_eq!(index.package_path_mismatches().len(), 2);
    assert_eq!(codes(index.diagnostics()), ["L0146", "L0146"]);
    assert_eq!(
        sources
            .slice(index.diagnostics()[0].primary_span())
            .expect("valid explicit package span"),
        "package wrong"
    );
    let omitted_diagnostic = index
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.primary_span().source_id() == omitted_id)
        .expect("omitted package diagnostic");
    assert!(omitted_diagnostic.primary_span().is_empty());
    assert_eq!(omitted_diagnostic.primary_span().start(), 0);
    assert!(index.validate().is_err());
}

#[test]
fn malformed_package_recovery_does_not_create_a_second_package_diagnostic() {
    let mut sources = SourceMap::new();
    let (source_id, parsed) = parsed_source(
        &mut sources,
        "recovery.ko",
        "package wrong.\nval retained = 1",
    );
    assert_eq!(codes(parsed.diagnostics()), ["L0048"]);
    let inputs = [SourceUnitInput::new(
        "root",
        "expected/recovery.ko",
        source_id,
        &parsed,
    )];

    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
    assert!(
        !codes(index.diagnostics())
            .iter()
            .any(|code| code == "L0146"),
        "malformed package syntax already has a parser root cause"
    );
    assert!(
        index
            .declarations()
            .iter()
            .any(|declaration| declaration.name() == "retained"),
        "recovery product must preserve independent valid declarations"
    );
    assert!(index.validate().is_err());
}

#[test]
fn unit_diagnostic_order_rejects_diagnostics_from_outside_the_unit() {
    let mut sources = SourceMap::new();
    let (source_id, parsed) = parsed_source(&mut sources, "inside.ko", "val inside = 1");
    let input = [SourceUnitInput::new(
        "root",
        "inside.ko",
        source_id,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &input).expect("valid unit");
    let outside_id = sources
        .add_source("outside.ko", "outside")
        .expect("unique outside source");
    let outside_span = sources.span(outside_id, 0, 7).expect("valid outside span");
    let code = diagnostic_codes::catalog()
        .expect("valid production catalog")
        .resolve(diagnostic_codes::TARGET_LAYOUT)
        .expect("published code");
    let diagnostic = Diagnostic::new(
        &sources,
        Severity::Error,
        code,
        "outside diagnostic",
        outside_span,
    )
    .expect("valid diagnostic");

    assert!(matches!(
        ordered_unit_diagnostics(&sources, index.source_units(), &[diagnostic]),
        Err(UnitDiagnosticOrderError::SourceOutsideUnit(source)) if source == outside_id
    ));
}
