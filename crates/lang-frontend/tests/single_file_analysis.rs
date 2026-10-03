//! SPEC-0253：固定单文件链的阶段短路、完整facts与独立身份合同。

use std::{cell::RefCell, convert::Infallible, error::Error};

use lang_frontend::{
    analysis::{SingleFileAnalysisError, SingleFileStage, analyze_single_file},
    diagnostic::Diagnostic,
    lexer::{LexerInternalError, lex},
    name_resolution::{NameEnvironment, NameResolution, resolve_names},
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        TypeCheckingError, TypeEnvironment, TypedFile, check_types, standard_environments,
    },
};

const STAGES: [SingleFileStage; 5] = [
    SingleFileStage::Lexer,
    SingleFileStage::Parser,
    SingleFileStage::NameResolution,
    SingleFileStage::TypeChecking,
    SingleFileStage::OwnershipChecking,
];
const MOVED: &str = "class Resource()\nfun take(own r: Resource): Unit {}\nfun moves(own r: Resource): Unit { take(r)\ntake(r) }";

struct Manual {
    parsed: ParsedFile,
    names: NameResolution,
    typed: TypedFile,
    owned: OwnershipCheckedFile,
    stages: Vec<(SingleFileStage, Vec<Diagnostic>)>,
}

// Frozen pre-façade production order, with no call to the new runner.
fn manual(
    sources: &SourceMap,
    source: SourceId,
    ne: &NameEnvironment,
    te: &TypeEnvironment,
) -> Manual {
    let lexed = lex(sources, source).unwrap();
    let parsed = parse_file(sources, &lexed).unwrap();
    let names = resolve_names(sources, &parsed, ne).unwrap();
    let typed = check_types(sources, &parsed, &names, te).unwrap();
    let owned = check_ownership(sources, &parsed, &names, &typed).unwrap();
    let diagnostics = [
        lexed.diagnostics(),
        parsed.diagnostics(),
        names.diagnostics(),
        typed.diagnostics(),
        owned.diagnostics(),
    ];
    let stages = STAGES
        .into_iter()
        .zip(diagnostics)
        .map(|(stage, diagnostics)| (stage, diagnostics.to_vec()))
        .collect();
    Manual {
        parsed,
        names,
        typed,
        owned,
        stages,
    }
}

#[test]
fn full_recovery_facts_and_original_source_match_frozen_manual_chain() {
    for text in [
        String::new(),
        "fun main(): Unit { val greeting = \"你好🌍\" }\r\n".into(),
        format!("#\nfun broken( {{\nfun names(): Unit {{ missing }}\nfun types(): Unit {{ val item: String = 1 }}\n{MOVED}"),
        format!("const val unused: Int = 7\n{MOVED}"),
        "class Resource {}\nclass Holder(var payload: Resource)\nfun deferred(holders: List<Holder>): Unit { val projected = holders[0].payload }".into(),
    ] {
        let mut sources = SourceMap::new();
        sources.add_source("excluded.ko", "# ignored subset").unwrap();
        let source = sources.add_source("memory/原始.ko", text).unwrap();
        let (ne, te) = standard_environments();
        let expected = manual(&sources, source, &ne, &te);
        let input = sources.source_text(source).unwrap();
        if input.contains("const val unused") {
            assert!(expected.typed.constants().is_some());
            assert!(expected.owned.diagnostics().iter().any(|d| d.code().to_string() == "L0131"));
        }
        if input.contains("fun deferred") {
            assert!(expected.owned.diagnostics().is_empty());
            assert!(!expected.owned.deferred().is_empty());
        }
        let mut stages = Vec::new();
        let actual = analyze_single_file(&sources, source, &ne, &te,
            |stage, diagnostics| { stages.push((stage, diagnostics.to_vec())); Ok::<_, Infallible>(()) },
            |view| {
                assert_eq!(view.parsed().source_id(), source);
                assert!(view.typed().is_compatible_with_names(view.names()));
                Ok((format!("{:?}", view.parsed().ast()), view.names().clone(), format!("{:?}", view.typed())))
            }).unwrap();
        assert_eq!(stages, expected.stages);
        assert_eq!(actual.parsed().source_id(), source);
        assert_eq!(actual.names().source_id(), source);
        assert_eq!(actual.typed().source_id(), source);
        assert_eq!(actual.owned().source_id(), source);
        assert_eq!(format!("{:?}", actual.parsed()), format!("{:?}", expected.parsed));
        assert_eq!(actual.names(), &expected.names);
        assert_eq!(format!("{:?}", actual.typed()), format!("{:?}", expected.typed));
        assert_eq!(format!("{:?}", actual.owned()), format!("{:?}", expected.owned));
        assert_eq!(actual.parsed().diagnostics(), expected.parsed.diagnostics());
        assert_eq!(actual.typed().diagnostics(), expected.typed.diagnostics());
        assert_eq!(actual.owned().diagnostics(), expected.owned.diagnostics());
        assert!(actual.owned().is_compatible_with(actual.names(), actual.typed()));
        assert_eq!(actual.observed().0, format!("{:?}", actual.parsed().ast()));
        assert_eq!(&actual.observed().1, actual.names());
        assert_eq!(actual.observed().2, format!("{:?}", actual.typed()));
        assert!(!actual.typed().is_compatible_with_names(&expected.names));
        assert!(!actual.owned().is_compatible_with(actual.names(), &expected.typed));
    }
}

#[test]
fn each_gate_and_observer_stop_before_every_following_callback() {
    let mut sources = SourceMap::new();
    let id = sources.add_source("stages.ko", MOVED).unwrap();
    let (ne, te) = standard_environments();
    for stop in 0..=5 {
        let trace = RefCell::new(Vec::new());
        let result = analyze_single_file(
            &sources,
            id,
            &ne,
            &te,
            |stage, _| {
                let index = STAGES
                    .iter()
                    .position(|candidate| *candidate == stage)
                    .unwrap();
                // The observer is event 4, ownership gate is event 5.
                let event = if index == 4 { 5 } else { index };
                trace.borrow_mut().push(event);
                if event == stop { Err(stop) } else { Ok(()) }
            },
            |_| {
                trace.borrow_mut().push(4);
                if stop == 4 { Err(stop) } else { Ok(()) }
            },
        );
        assert!(matches!(result, Err(SingleFileAnalysisError::Host(actual)) if actual == stop));
        assert_eq!(*trace.borrow(), (0..=stop).collect::<Vec<_>>());
    }
}

#[test]
fn nonempty_diagnostic_gates_stop_at_the_first_actual_stage() {
    for (text, expected) in [
        (format!("#\n{MOVED}"), SingleFileStage::Lexer),
        ("fun bad( {".into(), SingleFileStage::Parser),
        (
            format!("fun names(): Unit {{ missing }}\n{MOVED}"),
            SingleFileStage::NameResolution,
        ),
        (
            format!("fun types(): Unit {{ val x: String = 1 }}\n{MOVED}"),
            SingleFileStage::TypeChecking,
        ),
        (MOVED.into(), SingleFileStage::OwnershipChecking),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("gate.ko", text).unwrap();
        let (ne, te) = standard_environments();
        let old = manual(&sources, id, &ne, &te);
        let mut visited = Vec::new();
        let result = analyze_single_file(
            &sources,
            id,
            &ne,
            &te,
            |stage, diagnostics| {
                visited.push(stage);
                if diagnostics.is_empty() {
                    Ok(())
                } else {
                    Err((stage, diagnostics.to_vec()))
                }
            },
            |_| Ok(()),
        );
        let Err(SingleFileAnalysisError::Host((stage, diagnostics))) = result else {
            panic!("expected diagnostic stop")
        };
        assert_eq!(stage, expected);
        let index = STAGES
            .iter()
            .position(|candidate| *candidate == expected)
            .unwrap();
        assert_eq!(visited, STAGES[..=index]);
        assert_eq!(diagnostics, old.stages[index].1);
    }
}

#[test]
fn foreign_environment_is_rejected_only_after_earlier_gates() {
    let mut sources = SourceMap::new();
    let id = sources.add_source("env.ko", "fun main(): Unit {}").unwrap();
    let (ne, _) = standard_environments();
    let (_, foreign) = standard_environments();
    for stop in STAGES[..3].iter().copied() {
        let result = analyze_single_file(
            &sources,
            id,
            &ne,
            &foreign,
            |stage, _| if stage == stop { Err(stage) } else { Ok(()) },
            |_| Ok(()),
        );
        assert!(matches!(result, Err(SingleFileAnalysisError::Host(actual)) if actual == stop));
    }
    let mut visited = Vec::new();
    let result = analyze_single_file(
        &sources,
        id,
        &ne,
        &foreign,
        |stage, _| {
            visited.push(stage);
            Ok::<_, Infallible>(())
        },
        |_| -> Result<(), Infallible> { unreachable!("type mismatch precedes observer") },
    );
    let Err(SingleFileAnalysisError::Type(error)) = result else {
        panic!("expected original type error")
    };
    assert!(matches!(
        error,
        TypeCheckingError::MismatchedNameEnvironment
    ));
    assert_eq!(visited, STAGES[..3]);
    let expected = TypeCheckingError::MismatchedNameEnvironment;
    assert_eq!(error.to_string(), expected.to_string());
    assert_eq!(error.source().is_none(), expected.source().is_none());
}

#[test]
fn foreign_source_fails_before_callbacks_and_preserves_error_chain() {
    let sources = SourceMap::new();
    let mut foreign = SourceMap::new();
    let id = foreign.add_source("foreign.ko", "").unwrap();
    let (ne, te) = standard_environments();
    let result = analyze_single_file(
        &sources,
        id,
        &ne,
        &te,
        |_, _| -> Result<(), Infallible> { panic!("lex failure precedes gate") },
        |_| -> Result<(), Infallible> { panic!("lex failure precedes observer") },
    );
    let error = result.unwrap_err();
    assert!(matches!(
        error,
        SingleFileAnalysisError::Lexer(LexerInternalError::Source(_))
    ));
    let expected = lex(&sources, id).unwrap_err();
    assert_eq!(error.to_string(), expected.to_string());
    assert_eq!(error.source().unwrap().to_string(), expected.to_string());
}

#[test]
fn owned_result_moves_without_cloning_and_keeps_clone_and_fresh_identity_boundaries() {
    let result = {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("move.ko", "fun main(): Unit {}")
            .unwrap();
        let (ne, te) = standard_environments();
        let result = analyze_single_file(
            &sources,
            id,
            &ne,
            &te,
            |_, _| Ok::<_, Infallible>(()),
            |_| Ok(String::from("observed")),
        )
        .unwrap();
        let cloned_names = result.names().clone();
        let cloned_typed = result.typed().clone();
        assert!(
            result
                .owned()
                .is_compatible_with(&cloned_names, &cloned_typed)
        );
        let rechecked =
            check_ownership(&sources, result.parsed(), result.names(), result.typed()).unwrap();
        assert!(rechecked.is_compatible_with(result.names(), result.typed()));
        let fresh = check_types(&sources, result.parsed(), result.names(), &te).unwrap();
        assert!(!result.owned().is_compatible_with(result.names(), &fresh));
        result
    };
    let (parsed, names, typed, owned, observed) = result.into_parts();
    assert_eq!(parsed.source_id(), names.source_id());
    assert!(owned.is_compatible_with(&names, &typed));
    assert_eq!(observed, "observed");
}

#[test]
fn success_orders_observer_once_and_drops_its_payload_on_later_failure() {
    use std::{cell::Cell, rc::Rc};
    struct Payload(Rc<Cell<usize>>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("observer.ko", "fun main(): Unit {}")
        .unwrap();
    let (ne, te) = standard_environments();
    for fail in [false, true] {
        let trace = RefCell::new(Vec::new());
        let drops = Rc::new(Cell::new(0));
        let result = analyze_single_file(
            &sources,
            id,
            &ne,
            &te,
            |stage, _| {
                trace.borrow_mut().push(format!("{stage:?}"));
                if fail && stage == SingleFileStage::OwnershipChecking {
                    Err("stop")
                } else {
                    Ok(())
                }
            },
            |_| {
                trace.borrow_mut().push("Observer".into());
                Ok(Payload(Rc::clone(&drops)))
            },
        );
        assert_eq!(
            *trace.borrow(),
            [
                "Lexer",
                "Parser",
                "NameResolution",
                "TypeChecking",
                "Observer",
                "OwnershipChecking"
            ]
        );
        if fail {
            assert!(matches!(result, Err(SingleFileAnalysisError::Host("stop"))));
            assert_eq!(drops.get(), 1);
        } else {
            assert!(result.is_ok());
            assert_eq!(drops.get(), 0);
            drop(result);
            assert_eq!(drops.get(), 1);
        }
    }
}

#[test]
fn explicit_nonstandard_environment_is_neither_replaced_nor_mutated() {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("explicit.ko", "fun main(): Unit {}")
        .unwrap();
    let ne = NameEnvironment::new();
    let te = TypeEnvironment::new(&ne);
    let expected = manual(&sources, id, &ne, &te);
    let actual = analyze_single_file(
        &sources,
        id,
        &ne,
        &te,
        |_, _| Ok::<_, Infallible>(()),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(actual.names(), &expected.names);
    assert!(
        actual
            .names()
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0080")
    );
    assert_eq!(
        format!("{:?}", actual.typed()),
        format!("{:?}", expected.typed)
    );
    assert_eq!(
        format!("{:?}", actual.owned()),
        format!("{:?}", expected.owned)
    );
}
