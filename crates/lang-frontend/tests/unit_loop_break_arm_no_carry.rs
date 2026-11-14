//! Ordinary terminal-break coverage; never injects producer or receipt faults.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        UnitDropPoint, UnitLoopNoCarryExitKind, check_compilation_unit_ownership, check_ownership,
    },
    parser::parse_file,
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
fn check(body: &str, accepted: bool) {
    let mut sources = SourceMap::new();
    let text = format!(
        "fun view(source:String):borrow String from source=source\nfun observe(item:String):Unit {{}}\nfun run(rounds:Int,source:String,other:String,flag:Boolean,choose:Boolean):Unit {{{body}}}"
    );
    let source = sources.add_source("break.ko", &text).unwrap();
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
    for enabled in [false, true] {
        let single = observing(enabled, || {
            check_ownership(&sources, &parsed, &names, &typed).unwrap()
        });
        assert_eq!(
            single.diagnostics().is_empty(),
            accepted,
            "Single {body}: {:?}",
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
                SourceUnitInput::new("root", "break.ko", source, &parsed)
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
            let unit = observing(enabled, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            });
            assert_eq!(
                unit.diagnostics().is_empty(),
                accepted,
                "Unit {body}: {:?}",
                unit.diagnostics()
            );
            assert!(unit.no_carry_loops().is_empty());
            assert!(unit.no_carry_branches().is_empty());
            assert!(unit.no_carry_counter_branches().is_empty());
            assert_eq!(unit.no_carry_break_branches().len(), usize::from(accepted));
            assert_eq!(
                unit.borrow_results().bindings().len(),
                if accepted { 2 } else { 0 }
            );
            assert_eq!(
                unit.borrow_results().ends().len(),
                if accepted { 2 } else { 0 }
            );
            if accepted {
                let fact = &unit.no_carry_break_branches()[0];
                assert!(unit.owns_no_carry_break_branch(fact));
                assert!(!unit.owns_no_carry_branch(fact.pair()));
                assert_eq!(fact.exit_kind(), UnitLoopNoCarryExitKind::Break);
                assert_eq!(
                    fact.cleanup_point(),
                    UnitDropPoint::ControlTransfer(fact.break_expression())
                );
                assert_eq!(
                    fact.exit_cleanup_point(),
                    UnitDropPoint::LoopExit(fact.exit_target())
                );
                assert_eq!(fact.exit_target(), fact.target_loop());
                assert_ne!(fact.loop_body(), fact.break_statement());
                let [left, right] = fact.pair().arms();
                assert_eq!(left.loop_statement(), fact.exit_target());
                assert_eq!(right.loop_statement(), fact.exit_target());
                assert_eq!(left.source(), right.source());
                assert_ne!(left.binding(), right.binding());
                assert_ne!(left.body(), right.body());
                assert_ne!(left.end(), right.end());
                assert!(!left.same_identity(right));
                for arm in fact.pair().arms() {
                    assert!(!unit.owns_no_carry_loop(arm));
                    assert_eq!(unit.borrow_results().ends().iter().filter(|end|end.binding()==arm.binding()&&end.point()==arm.end()).count(),1);
                }
                let foreign =
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap();
                assert!(!foreign.owns_no_carry_break_branch(fact));
                assert!(!fact.same_identity(&foreign.no_carry_break_branches()[0]));
                assert_eq!(fact, &foreign.no_carry_break_branches()[0]);
            }
        }
    }
}
fn arms(left: usize, right: usize) -> String {
    format!(
        "if(choose){{borrow val left=view(source);{}}}else{{borrow val right=view(source);{}}}",
        "observe(left);".repeat(left),
        "observe(right);".repeat(right)
    )
}
fn terminal(arms: &str) -> String {
    format!("while(flag){{{arms};break}}")
}
#[test]
fn ordinary_break_loop_closes_arms_and_real_exit() {
    check(
        "while(flag){if(choose){borrow val left=view(source);observe(left)}else{borrow val right=view(source);observe(right)};break}",
        true,
    );
    for (left, right) in [(1, 1), (1, 3), (5, 2)] {
        check(
            &format!("{};observe(source)", terminal(&arms(left, right))),
            true,
        );
    }
}
#[test]
fn ordinary_break_loop_rejects_unsealed_jumps_and_releases() {
    let valid = terminal(&arms(1, 2));
    for body in [
        valid.replace(";break}", ";continue}"),
        valid.replace(";break}", ";return}"),
        valid.replace(";break}", ";if(choose){break}}"),
        valid.replace(";break}", ";observe(source);break}"),
        valid.replace(";break}", ";break;observe(source)}"),
        valid.replace("observe(left);", "observe(left);break;"),
        valid.replace("observe(left);", "continue;observe(left);"),
        valid.replace("view(source);observe(right)", "view(other);observe(right)"),
        valid.replace("if(choose)", "if(true)"),
        valid.replace("while(flag)", "while(true)"),
        format!("while(flag){{{valid}}}"),
        format!("{valid};while(flag){{borrow val later=view(source);observe(later);continue}}"),
        format!("while(flag){{borrow val earlier=view(source);observe(earlier);continue}};{valid}"),
        format!(
            "var remaining=rounds;while(remaining>0){{{};remaining=remaining-1;break}}",
            arms(1, 1)
        ),
    ] {
        check(&body, false);
    }
}
