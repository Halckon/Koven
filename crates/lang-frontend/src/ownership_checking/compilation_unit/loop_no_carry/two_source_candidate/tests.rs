use super::*;
use crate::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{check_compilation_unit_ownership, with_loan_observation},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
const SOURCE: &str = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b);consume(b)};remaining=remaining-1};consume(left);consume(right)}";
#[test]
fn real_domain_success_identity_and_late_failure_revoke() {
    for reversed in [false, true] {
        let base = if reversed {
            SOURCE.replace("left:String,right:String", "right:String,left:String")
        } else {
            SOURCE.to_owned()
        };
        let caller = "\nfun entry(){val a=\"a\";val b=\"b\";run(a,b,2,true);run(a,a,2,false)}";
        for success in [true, false] {
            let mut sources = SourceMap::new();
            let id = sources
                .add_source("candidate.ko", format!("{base}{caller}"))
                .unwrap();
            let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
            assert!(parsed.diagnostics().is_empty());
            let bad_id = sources.add_source("zz-bad.ko", "fun take(own s:String):Unit {}\nfun bad():Unit {val s=\"s\";take(s);consume(s)}").unwrap();
            let bad = parse_file(&sources, &lex(&sources, bad_id).unwrap()).unwrap();
            for (layout, input) in [
                SourceUnitInput::memory_document(id, &parsed),
                SourceUnitInput::new("root", "candidate.ko", id, &parsed),
            ]
            .into_iter()
            .enumerate()
            {
                if !success && layout == 0 {
                    continue;
                } // MemoryDocument is single-source only.
                let mut inputs = vec![input];
                if !success {
                    inputs.push(SourceUnitInput::new("root", "zz-bad.ko", bad_id, &bad));
                }
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
                            check_compilation_unit_ownership(
                                &sources, &inputs, &names, &types, &typed,
                            )
                            .unwrap()
                        })
                    };
                    let (owned, hits) = with_domain(analyze);
                    assert_eq!(
                        hits,
                        1,
                        "real fact constructed before any later failure: success={success} reversed={reversed} diagnostics={:?}",
                        owned.diagnostics()
                    );
                    assert!(!enabled());
                    assert_eq!(
                        owned.diagnostics().is_empty(),
                        success,
                        "{:?}",
                        owned.diagnostics()
                    );
                    assert!(
                        owned.no_carry_branches().is_empty()
                            && owned.no_carry_counter_branches().is_empty()
                            && owned.no_carry_loops().is_empty()
                    );
                    assert!(
                        owned.no_carry_break_branches().is_empty()
                            && owned.no_carry_break_prefixes().is_empty()
                            && owned.no_carry_counter_break_branches().is_empty()
                            && owned.no_carry_prefix_counter_breaks().is_empty()
                    );
                    if success {
                        let [fact] = owned.no_carry.two_sources.as_slice() else {
                            panic!("one independent fact")
                        };
                        assert!(owned.owns_two_source_counter(fact));
                        assert!(owned.clone().owns_two_source_counter(fact));
                        let (other, _) = with_domain(analyze);
                        assert!(
                            !other.owns_two_source_counter(fact),
                            "fresh validation cannot borrow authority"
                        );
                        for arm in &fact.arms {
                            assert!(!owned.owns_no_carry_loop(arm));
                        }
                        let expr = |id: UnitExpressionId| {
                            let span = parsed
                                .ast()
                                .expressions()
                                .get(id.expression())
                                .unwrap()
                                .span();
                            &base[span.start()..span.end()]
                        };
                        assert_eq!(expr(fact.initializer), "rounds");
                        assert_eq!(expr(fact.condition), "remaining>0");
                        assert_eq!(expr(fact.decrement), "remaining-1");
                        assert_eq!(expr(fact.decrement_operand), "remaining");
                        for (arm, source) in fact.arms.iter().zip(["left", "right"]) {
                            assert_eq!(expr(arm.source_argument()), source);
                            assert_eq!(expr(arm.initializer()), format!("view({source})"));
                            let resolution = names
                                .names()
                                .source_units()
                                .iter()
                                .find(|u| u.source_unit() == arm.source().source_unit())
                                .unwrap();
                            assert_eq!(
                                resolution.resolution().symbols()[arm.source().symbol().index()]
                                    .name(),
                                source
                            );
                        }
                        assert_ne!(fact.arms[0].source(), fact.arms[1].source());
                        assert_ne!(fact.counter, fact.rounds);
                        assert_ne!(fact.initial_statement, fact.update);
                        assert_ne!(fact.condition, fact.decrement);
                        assert_ne!(fact.initializer, fact.decrement_operand);
                        assert_eq!(
                            fact.branch.source_unit(),
                            fact.arms[0].callable().source_unit()
                        );
                        let ((combined, combined_hits), old_hits) = crate::ownership_checking::loop_no_carry::shape::unit_candidate_events::run(|| with_domain(analyze));
                        assert_eq!((combined_hits, old_hits), (1, 0));
                        assert!(combined.validate().is_ok());
                        let (legacy, _) = crate::ownership_checking::loop_no_carry::shape::unit_candidate_events::run(analyze);
                        assert!(legacy.no_carry.two_sources.is_empty());
                        assert!(legacy.validate().is_err());
                        let (single, _) = with_domain(|| {
                            let n = crate::name_resolution::resolve_names(&sources, &parsed, &env)
                                .unwrap();
                            let t =
                                crate::type_checking::check_types(&sources, &parsed, &n, &types)
                                    .unwrap();
                            crate::ownership_checking::check_ownership(&sources, &parsed, &n, &t)
                                .unwrap()
                        });
                        assert!(
                            !single.diagnostics().is_empty(),
                            "test Unit domain never enables Single"
                        );
                        assert!(owned.validate().is_ok());
                    } else {
                        assert!(owned.no_carry.two_sources.is_empty());
                        assert!(owned.loan_action_observations().is_empty());
                        assert!(owned.borrow_results().bindings().is_empty());
                        assert!(owned.validate().is_err());
                    }
                    let normal = analyze();
                    assert!(!normal.diagnostics().is_empty());
                    assert!(normal.no_carry.two_sources.is_empty());
                    assert!(normal.validate().is_err());
                }
            }
        }
    }
}

#[test]
fn independent_domain_preserves_old_neighbor_and_rejections() {
    for (source, old_success) in [
        (SOURCE.replace("view(right)", "view(left)"), true),
        (SOURCE.replace("remaining-1", "remaining-2"), false),
        (
            SOURCE.replace("consume(b);consume(b)", "consume(b);return"),
            false,
        ),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("neighbor.ko", source).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::memory_document(id, &parsed)];
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
            let (owned, hits) = with_domain(|| {
                with_loan_observation(observe, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                })
            });
            assert_eq!(hits, 0);
            assert!(owned.no_carry.two_sources.is_empty());
            assert_eq!(
                owned.no_carry_counter_branches().len(),
                usize::from(old_success)
            );
            assert_eq!(owned.validate().is_ok(), old_success);
        }
    }
}
