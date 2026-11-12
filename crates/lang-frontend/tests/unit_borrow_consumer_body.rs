//! Ordinary scopes provide frame evidence; iteration-local borrowing stays gated.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        capture_borrow_consumer_bodies, check_compilation_unit_ownership, check_ownership,
        with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn check_case(
    case: &str,
    body: &str,
    rejected: bool,
    check: impl Fn(&[lang_frontend::ownership_checking::BorrowConsumerBodyObservation]),
) {
    let text = format!(
        "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun pair(left: String, right: String): Unit {{}}\nfun take(own source: String): Unit {{}}\nfun run(source: String, other: String, flag: Boolean): Unit {{ {body} }}"
    );
    let text=text.replace("fun take(own source:","fun ready(): Boolean = true\nfun make(source: String): String = \"made\"\nfun take(own source:");
    let text = if case == "own-source" {
        text.replace("fun run(source:", "fun run(own source:")
    } else if case == "inout-source" {
        text.replace("fun run(source:", "fun run(inout source:")
    } else {
        text
    };
    let mut sources = SourceMap::new();
    let source = sources.add_source("frame.ko", &text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let (single, frames) = capture_borrow_consumer_bodies(|| {
        check_ownership(&sources, &parsed, &names, &typed).unwrap()
    });
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
            let (checked, frames) = capture_borrow_consumer_bodies(|| {
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
fn assert_identities(
    parsed: &lang_frontend::parser::ParsedFile,
    frames: &[lang_frontend::ownership_checking::BorrowConsumerBodyObservation],
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
    for frame in frames {
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

#[test]
fn ordinary_consumer_body_records_actual_parameter_call_returns() {
    for count in [1, 3, 8] {
        check_case(
            "repeated",
            &format!("while(flag){{{}}}", "observe(source);".repeat(count)),
            false,
            |observed| {
                assert_eq!(observed.len(), 1);
                let body = &observed[0];
                assert!(body.validate());
                assert_eq!(body.consumer_returns().len(), count);
                assert!(body.same_loop_as(body));
                for event in body.consumer_returns() {
                    assert!(body.owns_consumer_return(event));
                    assert_eq!(event.arguments().len(), 1);
                    assert_eq!(event.acquired_count(), 1);
                    assert_eq!(event.released_count(), 1);
                }
            },
        );
    }
    for (case, args, same) in [
        ("same-operand", "source,source", true),
        ("two-parameters", "source,other", false),
    ] {
        check_case(
            case,
            &format!("while(flag){{pair({args})}}"),
            false,
            |observed| {
                assert_eq!(observed.len(), 1);
                let body = &observed[0];
                assert!(body.validate());
                assert_eq!(body.consumer_returns().len(), 1);
                let event = &body.consumer_returns()[0];
                assert!(body.owns_consumer_return(event));
                assert_eq!(event.acquired_count(), 2);
                assert_eq!(event.released_count(), 2);
                assert_eq!(event.arguments().len(), 2);
                assert_eq!(event.arguments()[0].1 == event.arguments()[1].1, same);
                assert_ne!(event.arguments()[0].0, event.arguments()[1].0);
            },
        );
    }
}

#[test]
fn ordinary_consumer_body_siblings_never_share_return_identity() {
    check_case(
        "siblings",
        "while(flag){observe(source)};while(flag){observe(source)}",
        false,
        |bodies| {
            assert_eq!(bodies.len(), 2);
            assert!(bodies.iter().all(|b| b.validate()));
            assert!(!bodies[0].same_loop_as(&bodies[1]));
            assert!(!bodies[0].owns_consumer_return(&bodies[1].consumer_returns()[0]));
            assert!(!bodies[1].owns_consumer_return(&bodies[0].consumer_returns()[0]));
        },
    );
}
#[test]
fn ordinary_consumer_body_strict_neighbors_remain_ineligible() {
    for (case, body, rejected) in [
        (
            "local-source",
            "val local=\"value\";while(flag){observe(local)}",
            false,
        ),
        ("inout-source", "while(flag){observe(source)}", false),
        ("own-source", "while(flag){observe(source)}", false),
        (
            "fresh",
            "while(flag){borrow val item=view(source);observe(item)}",
            true,
        ),
        ("call-condition", "while(ready()){observe(source)}", false),
        ("constant-condition", "while(true){observe(source)}", false),
        ("continue", "while(flag){observe(source);continue}", false),
        ("break", "while(flag){observe(source);break}", false),
        ("return", "while(flag){observe(source);return}", false),
        ("nested", "while(flag){{observe(source)}}", false),
        ("empty", "while(flag){}", false),
        ("nonunit", "while(flag){make(source)}", false),
        ("wrong-operand", "while(flag){observe(\"literal\")}", false),
    ] {
        check_case(case, body, rejected, |bodies| {
            assert!(!bodies.iter().any(|b| b.validate()), "{case}")
        });
    }
}
