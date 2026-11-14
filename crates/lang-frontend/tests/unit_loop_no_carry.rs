//! Core no-carry loop borrowing, independent of research observation features.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        DropPoint, UnitDropPoint, check_compilation_unit_ownership, check_ownership,
    },
    parser::{Expression, Statement, parse_file},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};
fn observing<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    #[cfg(feature = "loan-observation-test-support")]
    {
        lang_frontend::ownership_checking::with_loan_observation(enabled, run)
    }
    #[cfg(not(feature = "loan-observation-test-support"))]
    {
        let _ = enabled;
        run()
    }
}
#[test]
fn ordinary_loop_local_borrow_ends_at_last_consumer_without_carry() {
    for (count, flag_mode) in [
        (1, ""),
        (3, ""),
        (9, ""),
        (17, ""),
        (1, "own "),
        (9, "own "),
    ] {
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun observe(item:String):Unit {{}}\nfun run(source:String,{flag_mode}flag:Boolean):Unit {{while(flag){{borrow val item=view(source);{}}}}}",
            "observe(item);".repeat(count)
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("no-carry.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let last = parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, n)| {
                matches!(n.payload(), Statement::Expression { .. })
                    && sources
                        .slice(n.span())
                        .unwrap()
                        .starts_with("observe(item)")
            })
            .max_by_key(|(_, n)| n.span().start())
            .unwrap()
            .0;
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        for enabled in [false, true] {
            let checked = observing(enabled, || {
                check_ownership(&sources, &parsed, &names, &typed).unwrap()
            });
            assert!(
                checked.diagnostics().is_empty(),
                "single/{count}/{enabled}: {:?}",
                checked.diagnostics()
            );
            let baseline = check_ownership(&sources, &parsed, &names, &typed).unwrap();
            assert_eq!(checked.diagnostics(), baseline.diagnostics());
            assert_eq!(checked.borrow_results(), baseline.borrow_results());
            assert_eq!(checked.loans(), baseline.loans());
            assert_eq!(checked.loan_ends(), baseline.loan_ends());
            let [binding] = checked.borrow_results().bindings() else {
                panic!("one real local binding")
            };
            let [end] = checked.borrow_results().ends() else {
                panic!("one exact last-use End")
            };
            assert_eq!(end.binding(), binding.binding());
            assert_eq!(end.point(), DropPoint::AfterStatement(last));
            let loan = binding.source_loan().expect("actual call handoff");
            assert_eq!(loan.call(), binding.initializer());
            let Expression::Call { arguments, .. } = parsed
                .ast()
                .expressions()
                .get(loan.call())
                .unwrap()
                .payload()
            else {
                panic!()
            };
            assert_eq!(arguments.len(), 1);
            assert_eq!(loan.argument(), arguments[0].value);
        }
        for rooted in [false, true] {
            let inputs = [if rooted {
                SourceUnitInput::new("root", "no-carry.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names =
                resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
            assert!(names.diagnostics().is_empty());
            let names = names.validate().unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty());
            let typed = typed.validate().unwrap();
            let mut prior_raw = None;
            for enabled in [false, true] {
                let checked = observing(enabled, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                });
                assert!(
                    checked.diagnostics().is_empty(),
                    "unit/{count}/{rooted}/{enabled}: {:?}",
                    checked.diagnostics()
                );
                let actual_raw = (
                    checked.borrow_results().clone(),
                    checked.loans().to_vec(),
                    checked.no_carry_loops().to_vec(),
                );
                if let Some(previous) = prior_raw.as_ref() {
                    assert_eq!(previous, &actual_raw, "observer-independent core facts");
                }
                prior_raw = Some(actual_raw);
                let baseline = observing(enabled, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                });
                assert_eq!(checked, baseline);
                let [binding] = checked.borrow_results().bindings() else {
                    panic!("one local binding")
                };
                let [end] = checked.borrow_results().ends() else {
                    panic!("one exact End")
                };
                assert_eq!(end.binding(), binding.binding());
                let UnitDropPoint::AfterStatement(actual) = end.point() else {
                    panic!("not scope cleanup")
                };
                assert_eq!(actual.statement(), last);
                assert_eq!(actual.source_unit(), binding.binding().source_unit());
                let loan = binding.source_loan().expect("actual handoff");
                assert_eq!(loan.call(), binding.initializer());
                let Expression::Call { arguments, .. } = parsed
                    .ast()
                    .expressions()
                    .get(loan.call().expression())
                    .unwrap()
                    .payload()
                else {
                    panic!()
                };
                assert_eq!(arguments.len(), 1);
                assert_eq!(loan.argument().expression(), arguments[0].value);
                assert_eq!(loan.argument().source_unit(), loan.call().source_unit());
                let [proof] = checked.no_carry_loops() else {
                    panic!("one actual core loop proof")
                };
                assert!(checked.owns_no_carry_loop(proof));
                assert!(!baseline.owns_no_carry_loop(proof));
                assert_eq!(proof.binding(), binding.binding());
                assert_eq!(proof.initializer(), loan.call());
                assert_eq!(proof.source_argument(), loan.argument());
                assert_eq!(proof.end(), end.point());
                assert_eq!(proof.consumers().len(), count);
                let Statement::While { body, .. } = parsed
                    .ast()
                    .statements()
                    .get(proof.loop_statement().statement())
                    .unwrap()
                    .payload()
                else {
                    panic!("actual while")
                };
                assert_eq!(*body, proof.body().statement());
                let (Statement::ControlBody { elements } | Statement::Block { elements }) =
                    parsed.ast().statements().get(*body).unwrap().payload()
                else {
                    panic!("actual body")
                };
                assert_eq!(elements.len(), count + 1);
                for ((statement, call, arg), expected) in
                    proof.consumers().iter().zip(&elements[1..])
                {
                    assert_eq!(statement.statement(), *expected);
                    assert_eq!(statement.source_unit(), proof.binding().source_unit());
                    let Statement::Expression { expression } =
                        parsed.ast().statements().get(*expected).unwrap().payload()
                    else {
                        panic!()
                    };
                    assert_eq!(call.expression(), *expression);
                    let Expression::Call { arguments, .. } = parsed
                        .ast()
                        .expressions()
                        .get(*expression)
                        .unwrap()
                        .payload()
                    else {
                        panic!()
                    };
                    assert_eq!(arg.expression(), arguments[0].value);
                    assert_eq!(arg.source_unit(), call.source_unit());
                    assert!(checked.loans().iter().any(|f| f.call() == *call
                        && f.argument() == *arg
                        && f.target() == binding.origin()));
                }
            }
        }
    }
}
