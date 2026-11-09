//! Named-drop and consume-rejection contracts retire Single observations, not owner-graph witnesses.
use super::{NamedDrop, point_text};
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, OwnershipDeferredReason, UnitDropPoint,
        UnitDropTarget, check_compilation_unit_ownership,
    },
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

pub(super) fn plans(text: &str) -> [Vec<NamedDrop>; 2] {
    inspected_plans(text, |_, _, _| {})
}

// Inspect actual Unit facts before projecting named drops; no Single owner graph is inferred.
pub(super) fn inspected_plans(
    text: &str,
    mut inspect: impl FnMut(&SourceMap, &ParsedFile, &CompilationUnitOwnership),
) -> [Vec<NamedDrop>; 2] {
    let mut sources = SourceMap::new();
    let source = sources.add_source("resource-drop.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(sources.source_text(source), Ok(text));
    let (name_env, type_env) = standard_environments();
    [
        SourceUnitInput::memory_document(source, &parsed),
        SourceUnitInput::new("root", "resource-drop.ko", source, &parsed),
    ]
    .map(|input| {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [entry] = index.source_units() else {
            panic!("one original resource source")
        };
        assert_eq!(entry.source_id(), source);
        let source_unit = entry.id();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_env).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_env).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_env, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        inspect(&sources, &parsed, &owned);
        owned
            .drops()
            .iter()
            .filter_map(|drop| {
                let UnitDropTarget::Named(symbol) = drop.target() else {
                    return None;
                };
                assert_eq!(symbol.source_unit(), source_unit);
                let name = names.names().source_units()[symbol.source_unit().index()]
                    .resolution()
                    .symbols()[symbol.symbol().index()]
                .name()
                .to_string();
                // Resolve the original AST boundary, never infer it from the expected text.
                let (point, point_source) = match drop.point() {
                    UnitDropPoint::AfterStatement(id) => {
                        (DropPoint::AfterStatement(id.statement()), id.source_unit())
                    }
                    UnitDropPoint::AfterExpression(id) => (
                        DropPoint::AfterExpression(id.expression()),
                        id.source_unit(),
                    ),
                    UnitDropPoint::ControlTransfer(id) => (
                        DropPoint::ControlTransfer(id.expression()),
                        id.source_unit(),
                    ),
                    UnitDropPoint::FunctionEntry(id) => {
                        (DropPoint::FunctionEntry(id.item()), id.source_unit())
                    }
                    UnitDropPoint::CallReturn(id) => {
                        (DropPoint::CallReturn(id.expression()), id.source_unit())
                    }
                    UnitDropPoint::BranchExit { control, branch } => (
                        DropPoint::BranchExit {
                            control: control.expression(),
                            branch,
                        },
                        control.source_unit(),
                    ),
                    UnitDropPoint::LoopExit(id) => {
                        (DropPoint::LoopExit(id.statement()), id.source_unit())
                    }
                    other => panic!("unexpected drop boundary: {other:?}"),
                };
                assert_eq!(point_source, source_unit);
                let (boundary, text) = point_text(&sources, &parsed, point);
                Some(NamedDrop {
                    name,
                    boundary,
                    text,
                })
            })
            .collect()
    })
}

// Keep the adjacent deinit capability-difference witness on its original Single path.
pub(super) fn consume_rejection_codes(text: &str) -> [Vec<String>; 2] {
    inspect_unit_ownership(text, "resource-readonly.ko", |owned| {
        assert!(owned.drops().is_empty(), "invalid body published drops");
        owned
            .diagnostics()
            .iter()
            .map(|diag| diag.code().to_string())
            .collect()
    })
}

pub(super) fn resource_lifetime_deferrals(text: &str) -> [bool; 2] {
    inspect_unit_ownership(text, "resource-deferred.ko", |owned| {
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let deferred = owned
            .deferred()
            .iter()
            .any(|fact| fact.reason() == OwnershipDeferredReason::ResourceLifetime);
        if deferred {
            assert!(owned.drops().is_empty());
            assert!(owned.validate().is_err());
        }
        deferred
    })
}

fn inspect_unit_ownership<T>(
    text: &str,
    source_name: &str,
    mut inspect: impl FnMut(CompilationUnitOwnership) -> T,
) -> [T; 2] {
    let mut sources = SourceMap::new();
    let source = sources.add_source(source_name, text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(sources.source_text(source), Ok(text));
    let (name_env, type_env) = standard_environments();
    [
        SourceUnitInput::memory_document(source, &parsed),
        SourceUnitInput::new("root", source_name, source, &parsed),
    ]
    .map(|input| {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [entry] = index.source_units() else {
            panic!("one original resource source");
        };
        assert_eq!(entry.source_id(), source);
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_env).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_env).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_env, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        inspect(owned)
    })
}

#[test]
fn readonly_deinit_rejection_keeps_exact_diagnostics_in_both_unit_entries() {
    use lang_frontend::{
        diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
        name_resolution::SourceUnitKey,
    };

    // These are the unchanged sources of the adjacent Single/Rooted difference witness.
    for body in ["this.value = 3", "update(&this.value)"] {
        let text = format!(
            "fun update(inout value: Int): Unit {{}}\nclass Guard(var value: Int) {{ deinit() {{ {body} }} }}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("resource-readonly.ko", &text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        assert_eq!(sources.source_text(source), Ok(text.as_str()));
        let source_span = |token: &str| {
            let mut matches = text.match_indices(token);
            let (start, _) = matches.next().expect("original source token");
            assert!(matches.next().is_none(), "unambiguous source token");
            sources.span(source, start, start + token.len()).unwrap()
        };
        let catalog = DiagnosticCodeCatalog::try_new(&["L0134"]).unwrap();
        let exclusive = body.starts_with("update");
        let field = source_span("var value: Int");
        let field_start = field.start() + "var ".len();
        let field_name = sources
            .span(source, field_start, field_start + "value".len())
            .unwrap();
        let mut expected = Diagnostic::new(
            &sources,
            Severity::Error,
            catalog.resolve("L0134").unwrap(),
            if exclusive {
                "inout argument is not a mutable place"
            } else {
                "assignment target is not a mutable place"
            },
            source_span(if exclusive { "&" } else { "this.value" }),
        )
        .unwrap();
        expected
            .add_label(&sources, field_name, "immutable binding declared here")
            .unwrap();
        if exclusive {
            expected
                .add_label(
                    &sources,
                    source_span("inout value: Int"),
                    "selected parameter declared here",
                )
                .unwrap();
        }
        let (name_env, type_env) = standard_environments();
        for rooted in [false, true] {
            let inputs = [if rooted {
                SourceUnitInput::new("root", "resource-readonly.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
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
                    assert_eq!(logical_path.as_str(), "resource-readonly.ko");
                }
                _ => panic!("requested source entry"),
            }
            let names =
                resolve_compilation_unit_names(&sources, &inputs, &index, &name_env).unwrap();
            assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
            let names = names.validate().unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_env).unwrap();
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let typed = typed.validate().unwrap();
            let owned =
                check_compilation_unit_ownership(&sources, &inputs, &names, &type_env, &typed)
                    .unwrap();
            assert!(owned.is_compatible_with(&typed));
            assert_eq!(
                owned.diagnostics(),
                [expected.clone()],
                "body={body}, rooted={rooted}"
            );
            assert!(
                owned.deferred().is_empty(),
                "ordinary diagnostic, not unsupported recovery"
            );
            assert!(
                owned.drops().is_empty(),
                "invalid deinit publishes no executable drop plan"
            );
            let rejected = owned
                .validate()
                .expect_err("invalid deinit cannot grant owned capability");
            assert_eq!(rejected.diagnostics(), [expected.clone()]);
            assert!(rejected.drops().is_empty());
        }
    }
}
