//! Ordinary bounded-counter sources; no producer or receipt fault injection.
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
        "fun view(source:String):borrow String from source=source\nfun observe(item:String):Unit {{}}\nfun observe_number(item:Int):Unit {{}}\nfun run({mode}rounds:Int,source:String,other:String,flag:Boolean,choose:Boolean):Unit {{{body}}}"
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
                unit.no_carry_counter_branches().len(),
                usize::from(accepted)
            );
            if accepted {
                let fact = &unit.no_carry_counter_branches()[0];
                assert!(unit.owns_no_carry_counter_branch(fact));
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
                assert!(!other_check.owns_no_carry_counter_branch(fact));
                assert!(!fact.same_identity(&other_check.no_carry_counter_branches()[0]));
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
                assert_eq!(fact, &other_check.no_carry_counter_branches()[0]);
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
fn bounded(arms: &str) -> String {
    format!("var remaining=rounds;while(remaining>0){{{arms};remaining=remaining-1}}")
}
#[test]
fn ordinary_counter_loop_arms_close_before_unique_update() {
    for mode in ["", "own "] {
        for (left, right) in [(1, 1), (1, 3), (5, 2)] {
            check(
                &format!("{};observe(source)", bounded(&arms(left, right))),
                mode,
                true,
            );
        }
    }
}
#[test]
fn ordinary_counter_loop_rejects_unsealed_counter_and_control() {
    let arms = arms(1, 2);
    let valid = bounded(&arms);
    let cases = [
        valid.replace("remaining-1", "remaining-0"),
        valid.replace("remaining-1", "remaining-2"),
        valid.replace("remaining>0", "remaining>=0"),
        valid.replace("rounds;while", "rounds;observe(source);while"),
        valid.replace(
            ";remaining=remaining-1",
            ";remaining=remaining-1;remaining=remaining-1",
        ),
        format!("{valid};val alias=remaining;observe_number(alias)"),
        format!("{valid};observe_number(remaining)"),
        valid.replace(
            "borrow val right=view(source)",
            "borrow val right=view(other)",
        ),
        valid.replace("observe(left);", "observe(left);continue;"),
        valid.replace("observe(left);", "observe(left);break;"),
        valid.replace("observe(left);", "if(flag){observe(left)};"),
        format!("while(flag){{{valid}}}"),
        valid.replace("rounds;while", "rounds;val alias=remaining;while"),
        valid.replace("rounds;while", "rounds;remaining=rounds;while"),
        format!("{valid};while(flag){{borrow val later=view(source);observe(later);continue}}"),
        format!("while(flag){{borrow val earlier=view(source);observe(earlier);continue}};{valid}"),
    ];
    for mode in ["", "own "] {
        for body in &cases {
            check(body, mode, false);
        }
    }
}

#[test]
fn ordinary_counter_loop_rejects_inout_rounds() {
    check(&bounded(&arms(1, 2)), "inout ", false);
}
