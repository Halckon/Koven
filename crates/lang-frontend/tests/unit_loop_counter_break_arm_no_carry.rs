//! Ordinary conditional counter-break sources; no fault injection.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};
fn observing<T>(on: bool, run: impl FnOnce() -> T) -> T {
    #[cfg(feature = "loan-observation-test-support")]
    {
        lang_frontend::ownership_checking::with_loan_observation(on, run)
    }
    #[cfg(not(feature = "loan-observation-test-support"))]
    {
        let _ = on;
        run()
    }
}
fn check(body: &str, mode: &str, accepted: bool) {
    let mut sources = SourceMap::new();
    let text = format!(
        "fun view(source:String):borrow String from source=source\nfun observe(item:String):Unit {{}}\nfun observe_number(item:Int):Unit {{}}\nfun run({mode}rounds:Int,source:String,other:String,flag:Boolean,{mode}choose:Boolean,{mode}stop:Boolean):Unit {{{body}}}"
    );
    let source = sources.add_source("arms.ko", &text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (env, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &env).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    for on in [false, true] {
        let single = observing(on, || {
            check_ownership(&sources, &parsed, &names, &typed).unwrap()
        });
        assert_eq!(
            single.diagnostics().is_empty(),
            accepted,
            "single {body}: {:?}",
            single.diagnostics()
        );
        assert_eq!(
            single.borrow_results().bindings().len(),
            if accepted { 2 } else { 0 }
        );
        assert_eq!(
            single.borrow_results().ends().len(),
            if accepted { 2 } else { 0 }
        );
        for rooted in [false, true] {
            let inputs = [if rooted {
                SourceUnitInput::new("root", "arms.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let unit = observing(on, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            });
            assert_eq!(
                unit.diagnostics().is_empty(),
                accepted,
                "unit {body}: {:?}",
                unit.diagnostics()
            );
            assert!(
                unit.no_carry_loops().is_empty(),
                "arms must not impersonate straight loop facts"
            );
            assert!(unit.no_carry_branches().is_empty());
            assert_eq!(
                unit.borrow_results().ends().len(),
                if accepted { 2 } else { 0 }
            );
            assert_eq!(
                unit.borrow_results().bindings().len(),
                if accepted { 2 } else { 0 }
            );
            assert_eq!(
                unit.no_carry_counter_break_branches().len(),
                usize::from(accepted)
            );
            if accepted {
                let conditional = &unit.no_carry_counter_break_branches()[0];
                assert!(unit.owns_no_carry_counter_break_branch(conditional));
                assert!(unit.no_carry_counter_branches().is_empty());
                assert!(unit.no_carry_break_branches().is_empty());
                assert_ne!(conditional.break_control(), conditional.break_expression());
                assert_ne!(
                    conditional.break_condition(),
                    conditional.break_expression()
                );
                let fact = conditional.counter();
                assert!(!unit.owns_no_carry_counter_branch(fact));
                assert_ne!(fact.counter(), fact.rounds());
                assert_ne!(fact.initializer(), fact.condition());
                assert_ne!(fact.condition(), fact.decrement());
                assert_ne!(fact.decrement(), fact.decrement_operand());
                assert_ne!(fact.initial_statement(), fact.update());
                let pair = fact.pair();
                assert!(!unit.owns_no_carry_branch(pair));
                let other_check =
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap();
                assert!(!other_check.owns_no_carry_counter_break_branch(conditional));
                assert!(
                    !conditional.same_identity(&other_check.no_carry_counter_break_branches()[0])
                );
                let [left, right] = pair.arms();
                assert_eq!(left.source(), right.source());
                assert_eq!(left.loop_statement(), right.loop_statement());
                assert_ne!(left.binding(), right.binding());
                assert_ne!(left.body(), right.body());
                assert_ne!(left.initializer(), right.initializer());
                assert_ne!(left.end(), right.end());
                assert!(!left.same_identity(right));
                for arm in pair.arms() {
                    assert!(!unit.owns_no_carry_loop(arm));
                    assert_eq!(
                        unit.borrow_results()
                            .ends()
                            .iter()
                            .filter(|e| e.binding() == arm.binding() && e.point() == arm.end())
                            .count(),
                        1
                    );
                    assert_eq!(
                        unit.borrow_results()
                            .bindings()
                            .iter()
                            .filter(|b| b.binding() == arm.binding()
                                && b.initializer() == arm.initializer())
                            .count(),
                        1
                    );
                }
                assert_eq!(
                    conditional,
                    &other_check.no_carry_counter_break_branches()[0]
                );
            }
        }
    }
}

fn body(transfer: &str) -> String {
    format!(
        "var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(source);observe(a)}}else{{borrow val b=view(source);observe(b);observe(b)}};{transfer};remaining=remaining-1}};observe(source)"
    )
}
#[test]
fn counter_conditional_break_seals_both_paths() {
    for mode in ["", "own "] {
        check(&body("if(stop){break}"), mode, true);
    }
}
#[test]
fn counter_conditional_break_rejects_extra_shapes() {
    for transfer in [
        "if(stop){break}else{}",
        "if(stop){continue}",
        "if(stop){return}",
        "if(flag){if(stop){break}}",
        "if(stop){observe(source);break}",
        "if(true){break}",
    ] {
        check(&body(transfer), "", false);
    }
}
