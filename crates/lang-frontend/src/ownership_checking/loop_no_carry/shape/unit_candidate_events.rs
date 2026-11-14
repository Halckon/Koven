//! Test-only tentative Unit analysis. Never retains failed observations or publishes facts.
use crate::ownership_checking::UnitLoopNoCarryFact;
use std::cell::Cell;
thread_local! { static STATE: Cell<(bool, usize)> = const { Cell::new((false, 0)) }; }
pub(crate) fn enabled() -> bool {
    STATE.get().0
}
fn run<T>(analyze: impl FnOnce() -> T) -> (T, usize) {
    struct Restore((bool, usize));
    impl Drop for Restore {
        fn drop(&mut self) {
            STATE.set(self.0);
        }
    }
    let _restore = Restore(STATE.replace((true, 0)));
    let result = analyze();
    (result, STATE.get().1)
}
/// Called only after both real final End facts passed the existing unique receipt check.
pub(crate) fn checked_ends(left: &UnitLoopNoCarryFact, right: &UnitLoopNoCarryFact) {
    if !enabled() || left.source() == right.source() {
        return;
    }
    assert_eq!(left.callable(), right.callable());
    assert_eq!(left.loop_statement(), right.loop_statement());
    assert_ne!(left.body(), right.body());
    assert_ne!(left.binding(), right.binding());
    assert_ne!(left.end(), right.end());
    assert!(!left.same_identity(right));
    assert!(!left.consumers().is_empty() && !right.consumers().is_empty());
    STATE.set((true, STATE.get().1 + 1));
}

#[cfg(feature = "loan-observation-test-support")]
#[test]
fn tentative_core_ends_do_not_escape_the_failed_unit() {
    use crate::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{check_compilation_unit_ownership, with_loan_observation},
        parser::parse_file,
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let original = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b);consume(b)};remaining=remaining-1}}";
    for (source, accepted, hits) in [
        (original.to_owned(), false, 1),
        (original.replace("view(right)", "view(left)"), true, 0),
        (
            original
                .replace("remaining>0", "choose")
                .replace(";remaining=remaining-1", ""),
            false,
            0,
        ),
        (
            original.replace("consume(b);consume(b)", "consume(b);return"),
            false,
            0,
        ),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("tentative-unit.ko", source).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        for input in [
            SourceUnitInput::memory_document(id, &parsed),
            SourceUnitInput::new("root", "tentative-unit.ko", id, &parsed),
        ] {
            let inputs = [input];
            let (env, types) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            for observe in [false, true] {
                let analyze = || {
                    with_loan_observation(observe, || {
                        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                            .unwrap()
                    })
                };
                let (tentative, count) = run(|| {
                    let single_names =
                        crate::name_resolution::resolve_names(&sources, &parsed, &env).unwrap();
                    let single_types =
                        crate::type_checking::check_types(&sources, &parsed, &single_names, &types)
                            .unwrap();
                    let single = crate::ownership_checking::check_ownership(
                        &sources,
                        &parsed,
                        &single_names,
                        &single_types,
                    )
                    .unwrap();
                    assert_eq!(
                        single.diagnostics().is_empty(),
                        accepted,
                        "Unit hook cannot enable Single"
                    );
                    assert_eq!(STATE.get().1, 0);
                    analyze()
                });
                assert_eq!(count, hits);
                assert!(!enabled());
                let normal = analyze();
                for owned in [tentative, normal] {
                    assert_eq!(owned.diagnostics().is_empty(), accepted);
                    if !accepted {
                        assert!(owned.loan_action_observations().is_empty());
                        assert!(owned.borrow_results().bindings().is_empty());
                        assert!(
                            owned.no_carry_loops().is_empty()
                                && owned.no_carry_branches().is_empty()
                                && owned.no_carry_counter_branches().is_empty()
                        );
                        assert!(
                            owned.no_carry_break_branches().is_empty()
                                && owned.no_carry_break_prefixes().is_empty()
                                && owned.no_carry_counter_break_branches().is_empty()
                                && owned.no_carry_prefix_counter_breaks().is_empty()
                        );
                    }
                    assert_eq!(owned.validate().is_ok(), accepted);
                }
            }
        }
    }
}

#[test]
fn tentative_state_restores_nested_and_panicking_scopes() {
    assert!(!enabled());
    let (_, count) = run(|| {
        STATE.set((true, 7));
        let (_, inner) = run(|| assert!(enabled()));
        assert_eq!(inner, 0);
        assert_eq!(STATE.get(), (true, 7));
        let failure = std::panic::catch_unwind(|| run(|| panic!("RAII sentinel")));
        assert!(failure.is_err());
        assert_eq!(STATE.get(), (true, 7));
    });
    assert_eq!(count, 7);
    assert!(!enabled());
}
