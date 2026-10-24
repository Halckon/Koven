//! SPEC-0025 Stage 2 package/import/visibility 名称解析契约。

use lang_frontend::{
    diagnostic::{Diagnostic, Severity},
    lexer::lex,
    name_resolution::{
        CompilationUnitNames, NameEnvironment, Namespace, SourceUnitInput, UnitReferenceTarget,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::standard_environments,
};

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique test source");
    let lexed = lex(sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (source, parsed)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn resolve<'a>(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'a>],
    environment: &NameEnvironment,
) -> CompilationUnitNames {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    resolve_compilation_unit_names(sources, inputs, &index, environment)
        .expect("unit names resolve internally")
}

#[test]
fn exact_alias_same_package_and_input_permutation_publish_stable_declarations() {
    let mut sources = SourceMap::new();
    let (api_id, api) = parsed(
        &mut sources,
        "api.ko",
        "package lib\npublic class Widget\npublic fun make(): Unit {}\npublic fun merge(x: Int): Unit {}\npublic val shared = 1",
    );
    let (more_id, more) = parsed(
        &mut sources,
        "more.ko",
        "package lib\npublic fun merge(x: String): Unit {}\nfun samePackage(): Unit {\nval seen = shared\nval merged = merge(1)\n}",
    );
    let (app_id, app) = parsed(
        &mut sources,
        "app.ko",
        "package app\nimport lib.Widget as W\nimport lib.make\nfun use(x: W): W = W()\nfun call(): Unit { make() }",
    );
    let forward = [
        SourceUnitInput::new("root", "lib/api.ko", api_id, &api),
        SourceUnitInput::new("root", "lib/more.ko", more_id, &more),
        SourceUnitInput::new("root", "app/app.ko", app_id, &app),
    ];
    let reverse = [forward[2], forward[1], forward[0]];
    let (environment, _) = standard_environments();

    let forward = resolve(&sources, &forward, &environment);
    let reverse = resolve(&sources, &reverse, &environment);

    assert_eq!(forward, reverse);
    assert_eq!(forward.declaration_symbols(), reverse.declaration_symbols());
    assert_eq!(
        forward.declaration_symbols().len(),
        forward.index().declarations().len()
    );
    for declaration in forward.index().declarations() {
        let unit_symbol = forward
            .declaration_symbol(declaration.id())
            .expect("every indexed declaration has a canonical source symbol");
        assert_eq!(unit_symbol.source_unit(), declaration.source_unit());
        let symbol = &forward.source_units()[unit_symbol.source_unit().index()]
            .resolution()
            .symbols()[unit_symbol.symbol().index()];
        assert_eq!(symbol.span(), declaration.name_span());
        assert_eq!(symbol.namespace(), declaration.namespace());
        assert_eq!(symbol.kind(), declaration.kind());
    }
    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert!(forward.references().iter().any(|reference| {
        sources.slice(reference.span()).ok() == Some("W")
            && matches!(reference.target(), UnitReferenceTarget::Declaration(_))
    }));
    assert!(forward.references().iter().any(|reference| {
        sources.slice(reference.span()).ok() == Some("shared")
            && matches!(reference.target(), UnitReferenceTarget::Declaration(_))
    }));
    assert!(forward.references().iter().any(|reference| {
        sources.slice(reference.span()).ok() == Some("merge")
            && matches!(reference.target(), UnitReferenceTarget::OverloadSet(ids) if ids.len() == 2)
    }));
}

#[test]
fn private_exact_is_l0149_and_blocks_validated_view() {
    let mut sources = SourceMap::new();
    let (lib_id, lib) = parsed(
        &mut sources,
        "lib.ko",
        "package lib\nprivate val secret = 1",
    );
    let (app_id, app) = parsed(
        &mut sources,
        "app.ko",
        "package app\nimport lib.secret\nval use = 1",
    );
    let inputs = [
        SourceUnitInput::new("root", "lib/lib.ko", lib_id, &lib),
        SourceUnitInput::new("root", "app/app.ko", app_id, &app),
    ];

    let (environment, _) = standard_environments();
    let names = resolve(&sources, &inputs, &environment);

    assert_eq!(codes(names.diagnostics()), ["L0149"]);
    assert_eq!(
        sources
            .slice(names.diagnostics()[0].primary_span())
            .unwrap(),
        "secret"
    );
    assert_eq!(names.diagnostics()[0].details().len(), 1);
    assert!(names.validate().is_err());
}

#[test]
fn exact_alias_conflict_is_per_namespace_and_repeated_target_is_idempotent() {
    let mut sources = SourceMap::new();
    let (left_id, left) = parsed(&mut sources, "left.ko", "package left\nclass Item");
    let (right_id, right) = parsed(&mut sources, "right.ko", "package right\nclass Other");
    let (app_id, app) = parsed(
        &mut sources,
        "app.ko",
        "package app\nimport left.Item as Local\nimport left.Item as Local\nimport right.Other as Local\nval ok = 1",
    );
    let inputs = [
        SourceUnitInput::new("root", "left/left.ko", left_id, &left),
        SourceUnitInput::new("root", "right/right.ko", right_id, &right),
        SourceUnitInput::new("root", "app/app.ko", app_id, &app),
    ];

    let names = resolve(&sources, &inputs, &NameEnvironment::new());

    assert_eq!(codes(names.diagnostics()), ["L0150"]);
    assert_eq!(
        sources
            .slice(names.diagnostics()[0].primary_span())
            .unwrap(),
        "Local"
    );
}

#[test]
fn wildcard_conflict_is_lazy_and_exact_import_wins() {
    let mut sources = SourceMap::new();
    let (left_id, left) = parsed(&mut sources, "left.ko", "package left\nval item = 1");
    let (right_id, right) = parsed(&mut sources, "right.ko", "package right\nval item = 2");
    let (unused_id, unused) = parsed(
        &mut sources,
        "unused.ko",
        "package unused\nimport left.*\nimport right.*\nval ok = 1",
    );
    let (used_id, used) = parsed(
        &mut sources,
        "used.ko",
        "package used\nimport left.*\nimport right.*\nval bad = item",
    );
    let environment = NameEnvironment::new();
    let unused_inputs = [
        SourceUnitInput::new("root", "left/left.ko", left_id, &left),
        SourceUnitInput::new("root", "right/right.ko", right_id, &right),
        SourceUnitInput::new("root", "unused/unused.ko", unused_id, &unused),
    ];
    assert!(
        resolve(&sources, &unused_inputs, &environment)
            .diagnostics()
            .is_empty()
    );

    let used_inputs = [
        SourceUnitInput::new("root", "left/left.ko", left_id, &left),
        SourceUnitInput::new("root", "right/right.ko", right_id, &right),
        SourceUnitInput::new("root", "used/used.ko", used_id, &used),
    ];
    let ambiguous = resolve(&sources, &used_inputs, &environment);
    assert_eq!(codes(ambiguous.diagnostics()), ["L0151"]);
    assert_eq!(
        sources
            .slice(ambiguous.diagnostics()[0].primary_span())
            .unwrap(),
        "item"
    );

    let (exact_id, exact) = parsed(
        &mut sources,
        "exact.ko",
        "package exact\nimport left.*\nimport right.*\nimport left.item\nval selected = item",
    );
    let exact_inputs = [
        SourceUnitInput::new("root", "left/left.ko", left_id, &left),
        SourceUnitInput::new("root", "right/right.ko", right_id, &right),
        SourceUnitInput::new("root", "exact/exact.ko", exact_id, &exact),
    ];
    assert!(
        resolve(&sources, &exact_inputs, &environment)
            .diagnostics()
            .is_empty()
    );
}

#[test]
fn exact_member_is_l0148_while_longest_package_prefix_resolves_top_level() {
    let mut sources = SourceMap::new();
    let (short_id, short) = parsed(&mut sources, "short.ko", "package alpha\nclass beta");
    let (long_id, long) = parsed(&mut sources, "long.ko", "package alpha.beta\nclass Item");
    let (app_id, app) = parsed(
        &mut sources,
        "app.ko",
        "package app\nimport alpha.beta.Item\nimport alpha.beta.Item.Member\nval ok = 1",
    );
    let inputs = [
        SourceUnitInput::new("root", "alpha/short.ko", short_id, &short),
        SourceUnitInput::new("root", "alpha/beta/long.ko", long_id, &long),
        SourceUnitInput::new("root", "app/app.ko", app_id, &app),
    ];

    let names = resolve(&sources, &inputs, &NameEnvironment::new());

    assert_eq!(codes(names.diagnostics()), ["L0148"]);
    assert_eq!(
        sources
            .slice(names.diagnostics()[0].primary_span())
            .unwrap(),
        "Member"
    );
    assert!(names.references().iter().any(|reference| {
        sources.slice(reference.span()).ok() == Some("Item")
            && matches!(reference.target(), UnitReferenceTarget::Declaration(_))
    }));
}

#[test]
fn package_qualified_enum_case_resolves_the_complete_member_chain() {
    let mut sources = SourceMap::new();
    let (shapes_id, shapes) = parsed(
        &mut sources,
        "shapes.ko",
        "package geometry\nenum class Shape { Circle }",
    );
    let (app_id, app) = parsed(
        &mut sources,
        "app.ko",
        "package app\nfun make(): Unit { geometry.Shape.Circle() }",
    );
    let inputs = [
        SourceUnitInput::new("root", "geometry/shapes.ko", shapes_id, &shapes),
        SourceUnitInput::new("root", "app/app.ko", app_id, &app),
    ];

    let (environment, _) = standard_environments();
    let names = resolve(&sources, &inputs, &environment);

    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    for segment in ["geometry", "Shape", "Circle"] {
        assert!(
            names.references().iter().any(|reference| {
                sources.slice(reference.span()).ok() == Some(segment)
                    && !matches!(reference.target(), UnitReferenceTarget::Unresolved)
            }),
            "missing resolved segment {segment}"
        );
    }
}

#[test]
fn imported_type_qualified_case_resolves_and_lexical_value_beats_package_prefix() {
    let mut sources = SourceMap::new();
    let (shapes_id, shapes) = parsed(
        &mut sources,
        "shapes.ko",
        "package geometry\nenum class Shape { Circle }",
    );
    let (catalog_id, catalog) = parsed(
        &mut sources,
        "catalog.ko",
        "package catalog\nval member = 1",
    );
    let (app_id, app) = parsed(
        &mut sources,
        "app.ko",
        "package app\nimport geometry.Shape\nfun make(): Unit {\nval made = Shape.Circle()\nval catalog = 1\nval observed = catalog.member\n}",
    );
    let inputs = [
        SourceUnitInput::new("root", "geometry/shapes.ko", shapes_id, &shapes),
        SourceUnitInput::new("root", "catalog/catalog.ko", catalog_id, &catalog),
        SourceUnitInput::new("root", "app/app.ko", app_id, &app),
    ];

    let (environment, _) = standard_environments();
    let names = resolve(&sources, &inputs, &environment);

    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    assert!(names.references().iter().any(|reference| {
        sources.slice(reference.span()).ok() == Some("Circle")
            && matches!(reference.target(), UnitReferenceTarget::Symbol(_))
    }));
    assert!(names.references().iter().any(|reference| {
        sources.slice(reference.span()).ok() == Some("catalog")
            && matches!(reference.target(), UnitReferenceTarget::Symbol(_))
    }));
    assert!(!names.references().iter().any(|reference| {
        sources.slice(reference.span()).ok() == Some("catalog")
            && matches!(reference.target(), UnitReferenceTarget::Package(_))
    }));
}

#[test]
fn compiler_external_root_beats_an_absolute_package_path() {
    let mut sources = SourceMap::new();
    let (values_id, values) = parsed(
        &mut sources,
        "values.ko",
        "package runtimeValue\nval member = 1",
    );
    let (types_id, types) = parsed(
        &mut sources,
        "types.ko",
        "package runtimeType\nclass Member",
    );
    let (app_id, app) = parsed(
        &mut sources,
        "app.ko",
        "package app\nval result = runtimeValue.member\nval typed: runtimeType.Member = runtimeType.Member()",
    );
    let inputs = [
        SourceUnitInput::new("root", "runtimeValue/values.ko", values_id, &values),
        SourceUnitInput::new("root", "runtimeType/types.ko", types_id, &types),
        SourceUnitInput::new("root", "app/app.ko", app_id, &app),
    ];
    let mut environment = NameEnvironment::new();
    environment
        .declare_value("runtimeValue")
        .expect("unique external value");
    environment
        .declare_type("runtimeType")
        .expect("unique external type");

    let names = resolve(&sources, &inputs, &environment);

    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    for root in ["runtimeValue", "runtimeType"] {
        assert!(names.references().iter().any(|reference| {
            sources.slice(reference.span()).ok() == Some(root)
                && matches!(reference.target(), UnitReferenceTarget::External(_))
        }));
        assert!(!names.references().iter().any(|reference| {
            sources.slice(reference.span()).ok() == Some(root)
                && matches!(reference.target(), UnitReferenceTarget::Package(_))
        }));
    }
}

#[test]
fn invisible_absolute_qualified_top_level_is_l0149() {
    let mut sources = SourceMap::new();
    let (lib_id, lib) = parsed(&mut sources, "lib.ko", "package lib\nprivate class Hidden");
    let (app_id, app) = parsed(
        &mut sources,
        "app.ko",
        "package app\nfun use(x: lib.Hidden): Unit {}",
    );
    let inputs = [
        SourceUnitInput::new("root", "lib/lib.ko", lib_id, &lib),
        SourceUnitInput::new("root", "app/app.ko", app_id, &app),
    ];

    let (environment, _) = standard_environments();
    let names = resolve(&sources, &inputs, &environment);

    assert_eq!(codes(names.diagnostics()), ["L0149"]);
    assert_eq!(
        sources
            .slice(names.diagnostics()[0].primary_span())
            .unwrap(),
        "Hidden"
    );
}

#[test]
fn static_members_require_qualified_use_and_do_not_expose_private_symbols() {
    let mut sources = SourceMap::new();
    let (lib_id, lib) = parsed(
        &mut sources,
        "lib.ko",
        "package lib\nobject Util { private fun open(x: Boolean): Unit {}; fun open(x: Int): Unit {}; fun open(x: String): Unit {}; private fun hidden(): Unit {} }\nclass Host { companion { fun make(): Unit {}; private fun secret(): Unit {} } }",
    );
    let (ok_id, ok) = parsed(
        &mut sources,
        "ok.ko",
        "package ok\nfun run(): Unit {\nval opened = lib.Util.open(1)\nval made = lib.Host.make()\n}",
    );
    let (bad_id, bad) = parsed(
        &mut sources,
        "bad.ko",
        "package bad\nimport lib.Util.open\nfun run(): Unit {\nval hidden = lib.Util.hidden()\nval secret = lib.Host.secret()\n}",
    );
    let (environment, _) = standard_environments();
    let ok_inputs = [
        SourceUnitInput::new("root", "lib/lib.ko", lib_id, &lib),
        SourceUnitInput::new("root", "ok/ok.ko", ok_id, &ok),
    ];
    let ok_names = resolve(&sources, &ok_inputs, &environment);
    assert!(
        ok_names.diagnostics().is_empty(),
        "{:?}",
        ok_names.diagnostics()
    );
    assert!(ok_names.references().iter().any(|reference| {
        sources.slice(reference.span()).ok() == Some("open")
            && matches!(reference.target(), UnitReferenceTarget::Symbols(ids) if ids.len() == 2)
    }));
    assert!(ok_names.references().iter().any(|reference| {
        sources.slice(reference.span()).ok() == Some("make")
            && matches!(reference.target(), UnitReferenceTarget::Symbol(_))
    }));

    let bad_inputs = [
        SourceUnitInput::new("root", "lib/lib.ko", lib_id, &lib),
        SourceUnitInput::new("root", "bad/bad.ko", bad_id, &bad),
    ];
    let bad_names = resolve(&sources, &bad_inputs, &environment);
    assert_eq!(codes(bad_names.diagnostics()), ["L0148", "L0080", "L0080"]);
    let primary_names = bad_names
        .diagnostics()
        .iter()
        .map(|diagnostic| sources.slice(diagnostic.primary_span()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(primary_names, ["open", "hidden", "secret"]);
}

#[test]
fn unloaded_exact_alias_and_wildcard_imports_explain_the_source_set() {
    let mut sources = SourceMap::new();
    let text = "package app\n// 界é😀\nimport q.value\nimport q.Missing_pkg.value as Alias\nimport q.Missing_pkg.*\nval localValue = 1\nval seen = localValue";
    let (app_id, app) = parsed(&mut sources, "app.ko", text);
    let inputs = [SourceUnitInput::new("root", "app/app.ko", app_id, &app)];
    let names = resolve(&sources, &inputs, &NameEnvironment::new());
    let code = lang_frontend::diagnostic::codes::catalog()
        .unwrap()
        .resolve("L0148")
        .unwrap();
    let mut expected = Vec::new();
    for (start, length, message, package) in [
        (
            text.find("q.value").unwrap() + 2,
            5,
            "unresolved exact import target",
            "q",
        ),
        (
            text.find("q.Missing_pkg.value").unwrap() + 14,
            5,
            "unresolved exact import target",
            "q.Missing_pkg",
        ),
        (
            text.find("q.Missing_pkg.*").unwrap() + 2,
            11,
            "unresolved wildcard import package",
            "q.Missing_pkg",
        ),
    ] {
        let mut diagnostic = Diagnostic::new(
            &sources,
            Severity::Error,
            code,
            message,
            sources.span(app_id, start, start + length).unwrap(),
        )
        .unwrap();
        diagnostic
            .add_note(format!(
                "package '{package}' is not loaded in this compilation unit"
            ))
            .unwrap();
        diagnostic
            .add_help(
                "include the required sources in the explicit project source set; single-file commands can use --project",
            )
            .unwrap();
        expected.push(diagnostic);
    }
    assert_eq!(names.diagnostics(), expected);
    for import in app.imports() {
        assert!(!names.references().iter().any(|reference| {
            reference.span().source_id() == app_id
                && reference.span().start() >= import.span.start()
                && reference.span().end() <= import.span.end()
        }));
    }
    let local_value = names
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "localValue")
        .unwrap();
    assert_eq!(local_value.name_span().source_id(), app_id);
    let start = text.rfind("localValue").unwrap();
    let span = sources
        .span(app_id, start, start + "localValue".len())
        .unwrap();
    assert_eq!(
        names
            .references()
            .iter()
            .find(|reference| reference.span() == span)
            .map(|reference| reference.target()),
        Some(&UnitReferenceTarget::Declaration(local_value.id()))
    );
}

#[test]
fn loaded_missing_member_tail_and_unclassified_imports_keep_original_diagnostics() {
    let mut sources = SourceMap::new();
    // This is an ordinary user source in this package, with no loader authority.
    let (algorithms_id, algorithms) = parsed(
        &mut sources,
        "algorithms.ko",
        "package koven.algorithms\nval present = 1",
    );
    let (p_id, p) = parsed(
        &mut sources,
        "p.ko",
        "package p\nclass Type { companion { fun member(): Unit {} } }",
    );
    let (short_id, short) = parsed(&mut sources, "short.ko", "package short\nclass Branch");
    let (long_id, long) = parsed(
        &mut sources,
        "long.ko",
        "package short.Branch\nval present = 1",
    );
    let text = "package app\nimport koven.algorithms.absent\nimport p.Type.member\nimport short.Branch.deep.value\nimport Item\nval ok = 1";
    let (app_id, app) = parsed(&mut sources, "app.ko", text);
    let inputs = [
        SourceUnitInput::new(
            "root",
            "koven/algorithms/algorithms.ko",
            algorithms_id,
            &algorithms,
        ),
        SourceUnitInput::new("root", "p/p.ko", p_id, &p),
        SourceUnitInput::new("root", "short/short.ko", short_id, &short),
        SourceUnitInput::new("root", "short/Branch/long.ko", long_id, &long),
        SourceUnitInput::new("root", "app/app.ko", app_id, &app),
    ];
    let (environment, _) = standard_environments();
    let names = resolve(&sources, &inputs, &environment);
    let code = lang_frontend::diagnostic::codes::catalog()
        .unwrap()
        .resolve("L0148")
        .unwrap();
    let expected = ["absent", "member", "value", "Item"]
        .into_iter()
        .map(|terminal| {
            let start = text.find(terminal).unwrap();
            Diagnostic::new(
                &sources,
                Severity::Error,
                code,
                "unresolved exact import target",
                sources.span(app_id, start, start + terminal.len()).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(names.diagnostics(), expected);
    let algorithms_package = names
        .index()
        .source_units()
        .iter()
        .find(|unit| unit.source_id() == algorithms_id)
        .unwrap()
        .package();
    let import_references = names
        .references()
        .iter()
        .filter(|reference| {
            reference.span().source_id() == app_id
                && app.imports().iter().any(|import| {
                    reference.span().start() >= import.span.start()
                        && reference.span().end() <= import.span.end()
                })
        })
        .map(|reference| {
            (
                reference.span(),
                reference.namespace(),
                reference.target().clone(),
            )
        })
        .collect::<Vec<_>>();
    let expected_references = app.imports()[0].segments[..2]
        .iter()
        .map(|segment| {
            (
                segment.span,
                None,
                UnitReferenceTarget::Package(algorithms_package),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(import_references, expected_references);
}

#[test]
fn default_package_and_real_long_package_imports_keep_declaration_targets() {
    let mut sources = SourceMap::new();
    let (default_id, default) = parsed(&mut sources, "default.ko", "class Item");
    let (short_id, short) = parsed(&mut sources, "short.ko", "package p\nclass Type");
    let (long_id, long) = parsed(&mut sources, "long.ko", "package p.Type\nval member = 1");
    let text = "package app\nimport Item\nimport p.Type.member as Selected\nfun use(x: Item): Item = x\nval chosen = Selected";
    let (app_id, app) = parsed(&mut sources, "app.ko", text);
    let inputs = [
        SourceUnitInput::new("root", "default.ko", default_id, &default),
        SourceUnitInput::new("root", "p/short.ko", short_id, &short),
        SourceUnitInput::new("root", "p/Type/long.ko", long_id, &long),
        SourceUnitInput::new("root", "app/app.ko", app_id, &app),
    ];
    let names = resolve(&sources, &inputs, &NameEnvironment::new());
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let item = names
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "Item")
        .unwrap();
    let member = names
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "member")
        .unwrap();
    assert_eq!(item.name_span().source_id(), default_id);
    assert_eq!(member.name_span().source_id(), long_id);
    let import_references = names
        .references()
        .iter()
        .filter(|reference| {
            reference.span().source_id() == app_id
                && app.imports().iter().any(|import| {
                    reference.span().start() >= import.span.start()
                        && reference.span().end() <= import.span.end()
                })
        })
        .map(|reference| {
            (
                reference.span(),
                reference.namespace(),
                reference.target().clone(),
            )
        })
        .collect::<Vec<_>>();
    let imports = app.imports();
    assert_eq!(
        import_references,
        [
            (
                imports[0].segments[0].span,
                Some(Namespace::Type),
                UnitReferenceTarget::Declaration(item.id()),
            ),
            (
                imports[1].segments[0].span,
                None,
                UnitReferenceTarget::Package(member.package()),
            ),
            (
                imports[1].segments[1].span,
                None,
                UnitReferenceTarget::Package(member.package()),
            ),
            (
                imports[1].segments[2].span,
                Some(Namespace::Value),
                UnitReferenceTarget::Declaration(member.id()),
            ),
            (
                imports[1].alias.unwrap().name_span,
                Some(Namespace::Value),
                UnitReferenceTarget::Declaration(member.id()),
            ),
        ]
    );
    let start = text.rfind("Selected").unwrap();
    let span = sources
        .span(app_id, start, start + "Selected".len())
        .unwrap();
    assert_eq!(
        names
            .references()
            .iter()
            .find(|reference| reference.span() == span)
            .map(|reference| reference.target()),
        Some(&UnitReferenceTarget::Declaration(member.id()))
    );
}
