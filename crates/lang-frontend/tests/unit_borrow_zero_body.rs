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
fn ordinary_zero_body_requires_actual_frame_lifecycle() {
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
            assert!(
                observed.body_complete(),
                "actual empty body frame must close before merge"
            );
            let body = observed.body_frame().expect("real body frame");
            assert!(observed.owns_body_frame(body));
            assert_eq!(body.acquisition_count(), 0);
            assert_eq!(body.end_count(), 0);
            assert!(body.preserved_before_cleanup());
            assert_eq!(
                body.scope_span().source_id(),
                observed.loop_span().source_id()
            );
            assert!(body.scope_span().start() >= observed.loop_span().start());
            assert!(body.scope_span().end() <= observed.loop_span().end());
            assert!(observed.same_loop_as(observed));
        });
    }
}

#[test]
fn ordinary_zero_body_identity_is_local_to_actual_loop_invocation() {
    let frame = "{borrow val item=view(source);observe(item)}";
    check_case(
        "siblings",
        &format!("{frame};while(flag){{}};{frame};while(flag){{}}"),
        false,
        |loops| {
            assert_eq!(loops.len(), 2);
            for observed in loops {
                assert!(observed.validate());
                assert!(observed.body_complete());
            }
            let first = loops[0].body_frame().unwrap();
            let second = loops[1].body_frame().unwrap();
            assert!(!loops[0].owns_body_frame(second));
            assert!(!loops[1].owns_body_frame(first));
            assert_ne!(first.scope_span(), second.scope_span());
        },
    );
}
#[test]
fn ordinary_zero_body_never_authorizes_nonempty_or_fresh_body() {
    for (case, body, rejected) in [
        ("no-prior", "while(flag){}", false),
        (
            "nonempty",
            "{borrow val item=view(source);observe(item)};while(flag){observe(source)}",
            false,
        ),
        (
            "literal",
            "{borrow val item=view(source);observe(item)};while(true){}",
            false,
        ),
        (
            "continue",
            "{borrow val item=view(source);observe(item)};while(flag){continue}",
            false,
        ),
        (
            "break",
            "{borrow val item=view(source);observe(item)};while(flag){break}",
            false,
        ),
        (
            "return",
            "{borrow val item=view(source);observe(item)};while(flag){return}",
            false,
        ),
        (
            "fresh",
            "{borrow val item=view(source);observe(item)};while(flag){borrow val fresh=view(source);observe(fresh)}",
            true,
        ),
    ] {
        check_case(case, body, rejected, |loops| {
            assert!(!loops.iter().any(|o| o.body_complete()))
        });
    }
}
