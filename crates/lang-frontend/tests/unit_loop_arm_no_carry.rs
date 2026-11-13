//! Ordinary sources for independently closed branch arms; no fault injection.
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
fn check(body: &str, accepted: bool) {
    let mut sources = SourceMap::new();
    let text = format!(
        "fun view(source:String):borrow String from source=source\nfun observe(item:String):Unit {{}}\nfun run(source:String,other:String,flag:Boolean,choose:Boolean):Unit {{{body}}}"
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
            assert_eq!(
                unit.borrow_results().bindings().len(),
                if accepted { 2 } else { 0 }
            );
            assert_eq!(unit.no_carry_branches().len(), usize::from(accepted));
            if accepted {
                let pair = &unit.no_carry_branches()[0];
                assert!(unit.owns_no_carry_branch(pair));
                let other_check =
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap();
                assert!(!other_check.owns_no_carry_branch(pair));
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
                assert_eq!(pair, &other_check.no_carry_branches()[0]);
            }
        }
    }
}
#[test]
fn ordinary_loop_arm_borrows_close_independently() {
    for (left, right) in [(1, 1), (1, 3), (5, 2)] {
        check(
            &format!(
                "while(flag){{if(choose){{borrow val left=view(source);{}}}else{{borrow val right=view(source);{}}}}}",
                "observe(left);".repeat(left),
                "observe(right);".repeat(right)
            ),
            true,
        );
    }
}
#[test]
fn ordinary_loop_arm_borrows_reject_unsealed_boundaries() {
    for body in [
        "while(flag){if(choose){borrow val left=view(source);observe(left)}else{borrow val right=view(other);observe(right)}}",
        "while(flag){if(choose){borrow val left=view(source);observe(left)}}",
        "while(flag){if(true){borrow val left=view(source);observe(left)}else{borrow val right=view(source);observe(right)}}",
        "while(flag){if(choose){borrow val left=view(source);observe(left);continue}else{borrow val right=view(source);observe(right)}}",
        "while(flag){if(choose){borrow val left=view(source);observe(left)}else{borrow val right=view(source);observe(right);break}}",
        "while(flag){if(choose){borrow val left=view(source);observe(left);return}else{borrow val right=view(source);observe(right)}}",
        "while(flag){if(choose){borrow val left=view(source);if(flag){observe(left)}}else{borrow val right=view(source);observe(right)}}",
        "while(flag){borrow val left=view(source);if(choose){observe(left)}else{observe(left)}}",
        "while(flag){if(choose){borrow val left=view(source);observe(left)}else{borrow val right=view(source);observe(right)};observe(source)}",
        "while(flag){while(flag){if(choose){borrow val left=view(source);observe(left)}else{borrow val right=view(source);observe(right)}}}",
        "while(flag){if(choose){borrow val left=view(source);observe(left)}else{borrow val right=view(source);observe(right)}};while(flag){borrow val later=view(source);observe(later);continue}",
    ] {
        check(body, false);
    }
}

#[test]
fn ordinary_loop_arm_finite_exit_candidates_remain_outside_selected_shape() {
    // Ordinary source changes, not producer/receipt fault injection. The current
    // stable Boolean-parameter loop has no finite positive normal-return path.
    for body in [
        "var again=flag;while(again){if(choose){borrow val left=view(source);observe(left)}else{borrow val right=view(source);observe(right)};again=false}",
        "var rounds=2;while(rounds>0){if(choose){borrow val left=view(source);observe(left)}else{borrow val right=view(source);observe(right)};rounds=rounds-1}",
        "while(flag){if(choose){borrow val left=view(source);observe(left)}else{borrow val right=view(source);observe(right)};break}",
    ] {
        check(body, false);
    }
}
