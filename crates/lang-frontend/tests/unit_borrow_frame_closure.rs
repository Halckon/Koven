//! Ordinary scopes provide frame evidence; iteration-local borrowing stays gated.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        capture_borrow_frames, check_compilation_unit_ownership, check_ownership,
        with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

#[test]
fn ordinary_frame_end_does_not_prove_external_obligation_restoration() {
    for (case, body, restored) in [
        (
            "closed",
            "{ borrow val item = view(source); observe(item) }",
            true,
        ),
        (
            "external-live-binding",
            "borrow val parent = view(source); { borrow val item = view(source); observe(item) }; observe(parent)",
            true,
        ),
        (
            "sibling-frames",
            "{ borrow val item = view(source); observe(item) }; { borrow val item = view(source); observe(item) }",
            true,
        ),
        (
            "source-moved-after-end",
            "{ borrow val item = view(source); observe(item); take(source) }",
            false,
        ),
    ] {
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun take(own source: String): Unit {{}}\nfun run(own source: String): Unit {{ {body} }}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("frame.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let (single, frames) =
            capture_borrow_frames(|| check_ownership(&sources, &parsed, &names, &typed).unwrap());
        assert!(
            single.diagnostics().is_empty(),
            "{case}: {:?}",
            single.diagnostics()
        );
        let baseline = check_ownership(&sources, &parsed, &names, &typed).unwrap();
        assert_eq!(single.diagnostics(), baseline.diagnostics());
        assert_eq!(single.borrow_results(), baseline.borrow_results());
        assert_eq!(single.loans(), baseline.loans());
        assert_eq!(single.loan_ends(), baseline.loan_ends());
        assert_frames(case, &frames, restored);
        for rooted in [false, true] {
            let inputs = [if rooted {
                SourceUnitInput::new("root", "frame.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty());
            let typed = typed.validate().unwrap();
            for enabled in [false, true] {
                let (checked, frames) = capture_borrow_frames(|| {
                    with_loan_observation(enabled, || {
                        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                            .unwrap()
                    })
                });
                assert!(
                    checked.diagnostics().is_empty(),
                    "{case}/{rooted}/{enabled}: {:?}",
                    checked.diagnostics()
                );
                let baseline = with_loan_observation(enabled, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                });
                assert_eq!(
                    checked, baseline,
                    "research capture must not change the ownership product"
                );
                assert_frames(case, &frames, restored);
            }
        }
    }
}

fn assert_frames(
    case: &str,
    frames: &[lang_frontend::ownership_checking::BorrowFrameObservation],
    restored: bool,
) {
    let frames = frames
        .iter()
        .filter(|frame| frame.local_binding_count() == 1 && frame.end_count() == 1)
        .collect::<Vec<_>>();
    assert!(
        !frames.is_empty(),
        "{case}: real nonempty scope observation"
    );
    if case == "sibling-frames" {
        assert_eq!(frames.len(), 2);
        assert_ne!(frames[0].scope_span(), frames[1].scope_span());
    }
    for frame in frames {
        assert_eq!(
            frame.external_unchanged(),
            restored,
            "{case}: actual external obligations {frame:?}"
        );
        assert_eq!(
            frame.validate(),
            restored,
            "{case}: an End alone is not a restored frame"
        );
    }
}

#[test]
fn ordinary_research_frame_cannot_certify_gated_iteration_bindings() {
    for (body, rejected) in [
        (
            "while (flag) { borrow val item = view(source); observe(item) }",
            false,
        ),
        (
            "while (flag) { { borrow val item = view(source); observe(item) } }",
            true,
        ),
        (
            "{ borrow val item = view(source); make(source); observe(item) }",
            false,
        ),
        (
            "{ borrow val item = view(source); observe(view(source)); observe(item) }",
            false,
        ),
    ] {
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun make(source: String): String = \"made\"\nfun run(source: String, flag: Boolean): Unit {{ {body} }}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("gated-frame.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let direct_loop = body == "while (flag) { borrow val item = view(source); observe(item) }";
        let check_frames =
            |frames: &[lang_frontend::ownership_checking::BorrowFrameObservation]| {
                if direct_loop {
                    let expected = parsed
                        .ast()
                        .statements()
                        .iter()
                        .find_map(|(_, node)| {
                            if let lang_frontend::parser::Statement::While { body, .. } =
                                node.payload()
                            {
                                Some(parsed.ast().statements().get(*body).unwrap().span())
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    let valid = frames.iter().filter(|f| f.validate()).collect::<Vec<_>>();
                    let [frame] = valid.as_slice() else {
                        panic!("one actual analyzed body frame: {frames:?}")
                    };
                    assert_eq!(frame.scope_span(), expected);
                    assert_eq!(frame.local_binding_count(), 1);
                    assert_eq!(frame.end_count(), 1);
                    assert!(frame.external_unchanged());
                    // This observes one static body check, not repeated runtime instances.
                } else {
                    assert!(!frames.iter().any(|f| f.validate()));
                }
            };
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let (single, frames) =
            capture_borrow_frames(|| check_ownership(&sources, &parsed, &names, &typed).unwrap());
        assert_eq!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164"),
            rejected
        );
        if !rejected {
            assert!(
                single.diagnostics().is_empty(),
                "{:?}",
                single.diagnostics()
            );
        }
        check_frames(&frames);
        for rooted in [false, true] {
            let inputs = [if rooted {
                SourceUnitInput::new("root", "gated-frame.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            for enabled in [false, true] {
                let (checked, frames) = capture_borrow_frames(|| {
                    with_loan_observation(enabled, || {
                        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                            .unwrap()
                    })
                });
                assert_eq!(
                    checked
                        .diagnostics()
                        .iter()
                        .any(|d| d.code().to_string() == "L0164"),
                    rejected
                );
                if !rejected {
                    assert!(
                        checked.diagnostics().is_empty(),
                        "{:?}",
                        checked.diagnostics()
                    );
                }
                check_frames(&frames);
            }
        }
    }
}
