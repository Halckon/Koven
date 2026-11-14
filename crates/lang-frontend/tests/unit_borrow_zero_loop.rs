//! Ordinary scopes provide frame evidence; iteration-local borrowing stays gated.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        capture_borrow_frames, capture_borrow_zero_loops, check_compilation_unit_ownership,
        check_ownership, with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn check_case(
    case: &str,
    body: &str,
    rejected: bool,
    check: impl Fn(&[lang_frontend::ownership_checking::BorrowZeroLoopObservation]),
) {
    let text = format!(
        "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun pair(left: String, right: String): Unit {{}}\nfun take(own source: String): Unit {{}}\nfun run(own source: String, flag: Boolean): Unit {{ {body} }}"
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
    if case == "preceding-complete" {
        let (_, prior) =
            capture_borrow_frames(|| check_ownership(&sources, &parsed, &names, &typed).unwrap());
        assert_eq!(
            prior
                .iter()
                .filter(|f| f.validate() && f.complete_closure())
                .count(),
            1,
            "independent real preceding closure"
        );
    }
    let (single, frames) =
        capture_borrow_zero_loops(|| check_ownership(&sources, &parsed, &names, &typed).unwrap());
    assert!(
        single.diagnostics().is_empty() || rejected,
        "{case}: {:?}",
        single.diagnostics()
    );
    assert_eq!(
        single
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0164"),
        rejected
    );
    let baseline = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert_eq!(single.diagnostics(), baseline.diagnostics());
    assert_eq!(single.borrow_results(), baseline.borrow_results());
    assert_eq!(single.loans(), baseline.loans());
    assert_eq!(single.loan_ends(), baseline.loan_ends());
    check(&frames);

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
            if case == "preceding-complete" {
                let (_, prior) = capture_borrow_frames(|| {
                    with_loan_observation(enabled, || {
                        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                            .unwrap()
                    })
                });
                assert_eq!(
                    prior
                        .iter()
                        .filter(|f| f.validate() && f.complete_closure())
                        .count(),
                    1,
                    "independent unit preceding closure"
                );
            }
            let (checked, frames) = capture_borrow_zero_loops(|| {
                with_loan_observation(enabled, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                })
            });
            assert!(
                checked.diagnostics().is_empty() || rejected,
                "{case}/{rooted}/{enabled}: {:?}",
                checked.diagnostics()
            );
            assert_eq!(
                checked
                    .diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0164"),
                rejected
            );
            let baseline = with_loan_observation(enabled, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            });
            assert_eq!(
                checked, baseline,
                "research capture must not change the ownership product"
            );
            check(&frames);
        }
    }
}

#[test]
fn ordinary_preceding_complete_frame_proves_zero_loop_boundary() {
    for count in [1, 3] {
        let body = format!(
            "{{borrow val item=view(source);{}}};while(flag){{}}",
            "observe(item);".repeat(count)
        );
        check_case("preceding-complete", &body, false, |loops| {
            assert_eq!(loops.len(), 1, "one actual zero-loop observation");
            let observed = &loops[0];
            assert!(observed.preceding_frame().validate());
            assert!(observed.preceding_frame().complete_closure());
            assert_eq!(observed.preceding_frame().consumer_returns().len(), count);
            assert!(observed.header_preserved());
            assert!(observed.backedge_preserved());
            assert!(observed.validate());
            assert!(observed.same_loop_as(observed));
        });
    }
}

#[test]
fn ordinary_zero_loop_rejects_nonadjacent_nonempty_and_nonzero_boundaries() {
    let frame = "{borrow val item=view(source);observe(item)}";
    for (case, body, rejected) in [
        ("no-frame", "while(flag){}".to_owned(), false),
        (
            "intervening",
            format!("{frame};observe(source);while(flag){{}}"),
            false,
        ),
        (
            "different-parent",
            format!("{{{frame}}};while(flag){{}}"),
            false,
        ),
        (
            "carried",
            format!("borrow val parent=view(source);{frame};while(flag){{}};observe(parent)"),
            false,
        ),
        (
            "nonempty",
            format!("{frame};while(flag){{observe(source)}}"),
            false,
        ),
        (
            "continue",
            format!("{frame};while(flag){{continue}}"),
            false,
        ),
        ("break", format!("{frame};while(flag){{break}}"), false),
        ("return", format!("{frame};while(flag){{return}}"), false),
        (
            "fresh",
            format!("{frame};while(flag){{borrow val local=view(source);observe(local)}}"),
            true,
        ),
        ("literal", format!("{frame};while(true){{}}"), false),
        (
            "local-bool",
            format!("val local=true;{frame};while(local){{}}"),
            false,
        ),
    ] {
        check_case(case, &body, rejected, |loops| {
            assert!(!loops.iter().any(|l| l.validate()), "{case}")
        });
    }
}
#[test]
fn ordinary_zero_loop_tokens_are_invocation_specific_and_single_use() {
    let frame = "{borrow val item=view(source);observe(item)}";
    check_case(
        "siblings",
        &format!("{frame};while(flag){{}};{frame};while(flag){{}}"),
        false,
        |loops| {
            assert_eq!(loops.len(), 2);
            assert!(loops.iter().all(|l| l.validate()));
            assert!(!loops[0].same_loop_as(&loops[1]));
            assert_ne!(loops[0].loop_span(), loops[1].loop_span());
            assert_ne!(
                loops[0].preceding_frame().scope_span(),
                loops[1].preceding_frame().scope_span()
            );
        },
    );
    check_case(
        "reuse",
        &format!("{frame};while(flag){{}};while(flag){{}}"),
        false,
        |loops| {
            assert_eq!(
                loops.iter().filter(|l| l.validate()).count(),
                1,
                "one preceding ticket cannot authorize both loops"
            );
        },
    );
}
