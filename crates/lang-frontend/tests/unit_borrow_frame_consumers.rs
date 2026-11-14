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
fn ordinary_local_frame_consumers_complete_closure() {
    for count in [1, 3, 8] {
        let case = "local-consumers";
        let restored = true;
        let body = format!(
            "{{ borrow val item=view(source); {} }}",
            "observe(item);".repeat(count)
        );
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
        assert_frames(case, &frames, restored, count);
        assert_identities(&parsed, &frames, |argument| {
            let lang_frontend::ownership_checking::BorrowLoopExpression::Single {
                source,
                expression,
            } = argument
            else {
                panic!("single identity")
            };
            assert_eq!(source, parsed.source_id());
            let span = parsed.ast().expressions().get(expression).unwrap().span();
            let reference = names
                .references()
                .iter()
                .find(|reference| reference.span() == span)
                .unwrap();
            let lang_frontend::name_resolution::ReferenceTarget::Symbol(symbol) =
                reference.target()
            else {
                panic!("resolved binding")
            };
            lang_frontend::ownership_checking::BorrowLoopBinding::Single {
                source,
                symbol: *symbol,
            }
        });

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
                assert_frames(case, &frames, restored, count);
                assert_identities(&parsed, &frames, |argument| {
                    let lang_frontend::ownership_checking::BorrowLoopExpression::Unit(argument) =
                        argument
                    else {
                        panic!("unit identity")
                    };
                    let span = parsed
                        .ast()
                        .expressions()
                        .get(argument.expression())
                        .unwrap()
                        .span();
                    let reference = names
                        .names()
                        .references()
                        .iter()
                        .find(|reference| {
                            reference.source_unit() == argument.source_unit()
                                && reference.span() == span
                        })
                        .unwrap();
                    let lang_frontend::name_resolution::UnitReferenceTarget::Symbol(symbol) =
                        reference.target()
                    else {
                        panic!("resolved unit binding")
                    };
                    lang_frontend::ownership_checking::BorrowLoopBinding::Unit(*symbol)
                });
            }
        }
    }
}

fn assert_frames(
    case: &str,
    frames: &[lang_frontend::ownership_checking::BorrowFrameObservation],
    restored: bool,
    count: usize,
) {
    let frames = frames
        .iter()
        .filter(|f| f.local_binding_count() == 1 && f.end_count() == 1)
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 1, "{case}");
    let frame = frames[0];
    assert_eq!(frame.validate(), restored);
    assert!(
        frame.complete_closure(),
        "all actual consumer returns and local End must close"
    );
    assert_eq!(frame.consumer_returns().len(), count);
    let mut calls = Vec::new();
    for event in frame.consumer_returns() {
        assert!(frame.owns_consumer_return(event));
        assert!(!calls.contains(&event.call()));
        calls.push(event.call());
        assert_eq!(event.arguments().len(), 1);
        assert_eq!(event.released_loan_count(), 1);
    }
}

fn assert_identities(
    parsed: &lang_frontend::parser::ParsedFile,
    frames: &[lang_frontend::ownership_checking::BorrowFrameObservation],
    resolve: impl Fn(
        lang_frontend::ownership_checking::BorrowLoopExpression,
    ) -> lang_frontend::ownership_checking::BorrowLoopBinding,
) {
    use lang_frontend::{ownership_checking::BorrowLoopExpression, parser::Expression};
    let raw = |id| match id {
        BorrowLoopExpression::Single { source, expression } => {
            assert_eq!(source, parsed.source_id());
            expression
        }
        BorrowLoopExpression::Unit(id) => id.expression(),
    };
    for frame in frames.iter().filter(|f| f.local_binding_count() > 0) {
        for event in frame.consumer_returns() {
            let Expression::Call { arguments, .. } = parsed
                .ast()
                .expressions()
                .get(raw(event.call()))
                .unwrap()
                .payload()
            else {
                panic!("actual call")
            };
            assert_eq!(arguments.len(), event.arguments().len());
            for ((actual, binding), expected) in event.arguments().iter().zip(arguments) {
                assert_eq!(raw(*actual), expected.value);
                assert_eq!(*binding, resolve(*actual));
            }
        }
    }
}

fn check_case(
    case: &str,
    body: &str,
    rejected: bool,
    check: impl Fn(&[lang_frontend::ownership_checking::BorrowFrameObservation]),
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
    let (single, frames) =
        capture_borrow_frames(|| check_ownership(&sources, &parsed, &names, &typed).unwrap());
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
    assert_identities(&parsed, &frames, |argument| {
        let lang_frontend::ownership_checking::BorrowLoopExpression::Single { source, expression } =
            argument
        else {
            panic!("single identity")
        };
        assert_eq!(source, parsed.source_id());
        let span = parsed.ast().expressions().get(expression).unwrap().span();
        let reference = names
            .references()
            .iter()
            .find(|reference| reference.span() == span)
            .unwrap();
        let lang_frontend::name_resolution::ReferenceTarget::Symbol(symbol) = reference.target()
        else {
            panic!("resolved binding")
        };
        lang_frontend::ownership_checking::BorrowLoopBinding::Single {
            source,
            symbol: *symbol,
        }
    });

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
            assert_identities(&parsed, &frames, |argument| {
                let lang_frontend::ownership_checking::BorrowLoopExpression::Unit(argument) =
                    argument
                else {
                    panic!("unit identity")
                };
                let span = parsed
                    .ast()
                    .expressions()
                    .get(argument.expression())
                    .unwrap()
                    .span();
                let reference = names
                    .names()
                    .references()
                    .iter()
                    .find(|reference| {
                        reference.source_unit() == argument.source_unit()
                            && reference.span() == span
                    })
                    .unwrap();
                let lang_frontend::name_resolution::UnitReferenceTarget::Symbol(symbol) =
                    reference.target()
                else {
                    panic!("resolved unit binding")
                };
                lang_frontend::ownership_checking::BorrowLoopBinding::Unit(*symbol)
            });
        }
    }
}

#[test]
fn ordinary_frame_operand_and_binding_matrix() {
    for (case, body, locals, counts) in [
        (
            "same-binding-pair",
            "{borrow val item=view(source);pair(item,item)}",
            1,
            vec![2],
        ),
        (
            "two-bindings-pair",
            "{borrow val left=view(source);borrow val right=view(source);pair(left,right)}",
            2,
            vec![2],
        ),
        (
            "interleaved-bindings",
            "{borrow val left=view(source);borrow val right=view(source);observe(left);observe(right);observe(left);observe(right)}",
            2,
            vec![1, 1, 1, 1],
        ),
    ] {
        check_case(case, body, false, |frames| {
            let frame = frames
                .iter()
                .find(|f| f.local_binding_count() == locals && f.end_count() == locals)
                .expect("local frame");
            assert!(frame.validate());
            assert!(frame.complete_closure());
            assert_eq!(frame.consumer_returns().len(), counts.len());
            for (event, count) in frame.consumer_returns().iter().zip(&counts) {
                assert!(frame.owns_consumer_return(event));
                assert_eq!(event.arguments().len(), *count);
                assert_eq!(event.released_loan_count(), *count);
            }
            if counts == [2] {
                let args = frame.consumer_returns()[0].arguments();
                assert_eq!(args[0].1 == args[1].1, locals == 1);
                assert_ne!(args[0].0, args[1].0);
            }
        });
    }
}
#[test]
fn ordinary_frame_siblings_and_inherited_binding_do_not_share_local_proof() {
    check_case(
        "siblings",
        "{borrow val item=view(source);observe(item)};{borrow val item=view(source);observe(item)}",
        false,
        |frames| {
            let local = frames
                .iter()
                .filter(|f| f.local_binding_count() == 1 && f.end_count() == 1)
                .collect::<Vec<_>>();
            assert_eq!(local.len(), 2);
            for frame in &local {
                assert!(frame.validate());
                assert!(frame.complete_closure());
                assert_eq!(frame.consumer_returns().len(), 1);
            }
            assert!(!local[0].owns_consumer_return(&local[1].consumer_returns()[0]));
            assert!(!local[1].owns_consumer_return(&local[0].consumer_returns()[0]));
        },
    );
    check_case(
        "inherited",
        "borrow val parent=view(source);{observe(parent)};observe(parent)",
        false,
        |frames| {
            assert!(!frames.iter().any(|f| f.complete_closure()));
        },
    );
}
#[test]
fn ordinary_local_closure_preserves_live_parent_obligations() {
    check_case(
        "parent-live",
        "borrow val parent=view(source);{borrow val item=view(source);observe(item)};observe(parent)",
        false,
        |frames| {
            let frame = frames
                .iter()
                .find(|f| f.local_binding_count() == 1 && f.end_count() == 1)
                .expect("inner local frame");
            assert!(frame.external_unchanged());
            assert!(frame.validate());
            assert!(frame.complete_closure());
            assert_eq!(frame.consumer_returns().len(), 1);
        },
    );
}

#[test]
fn ordinary_complete_frame_preserves_rejection_boundaries() {
    for (case, body, rejected) in [
        (
            "moved",
            "{borrow val item=view(source);observe(item);take(source)}",
            false,
        ),
        (
            "loop",
            "while(flag){borrow val item=view(source);observe(item)}",
            true,
        ),
        (
            "complex",
            "{borrow val item=view(source);observe(view(source));observe(item)}",
            false,
        ),
    ] {
        check_case(case, body, rejected, |frames| {
            assert!(!frames.iter().any(|f| f.complete_closure()), "{case}")
        });
    }
}
