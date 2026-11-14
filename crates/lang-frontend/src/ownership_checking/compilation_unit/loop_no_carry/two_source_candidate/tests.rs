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
        let base = if reversed {
            format!(
                "{}\nfun consume_other(item:String):Unit {{}}",
                base.replace("consume(b);consume(b)", "consume_other(b);consume(b)")
            )
        } else {
            base
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
                        for (arm_index, (arm, calls)) in
                            fact.arms.iter().zip(&fact.calls).enumerate()
                        {
                            let expressions = std::iter::once(arm.initializer())
                                .chain(arm.consumers().iter().map(|(_, call, _)| *call))
                                .collect::<Vec<_>>();
                            assert_eq!(calls.len(), expressions.len());
                            for (position, ((expression, key), expected)) in
                                calls.iter().zip(expressions).enumerate()
                            {
                                assert_eq!(*expression, expected);
                                assert_eq!(
                                    key,
                                    typed.types().call(*expression).unwrap().instance()
                                );
                                let crate::type_checking::UnitCallTarget::Declaration(id) =
                                    key.target()
                                else {
                                    panic!("actual source callee")
                                };
                                let declaration = names
                                    .names()
                                    .index()
                                    .declarations()
                                    .iter()
                                    .find(|d| d.id() == id)
                                    .unwrap();
                                let expected_name = if position == 0 {
                                    "view"
                                } else if reversed && arm_index == 1 && position == 1 {
                                    "consume_other"
                                } else {
                                    "consume"
                                };
                                assert_eq!(declaration.name(), expected_name);
                            }
                        }
                        assert_eq!(fact.calls[1].len(), 3);
                        assert_ne!(fact.calls[1][1].0, fact.calls[1][2].0);
                        assert_eq!(fact.calls[1][1].1 == fact.calls[1][2].1, !reversed);
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

#[test]
fn process_entry_argument_end_evidence_distinguishes_literals_and_named_scalars() {
    for (named, extra) in [
        (false, 0),
        (true, 0),
        (true, 1),
        (true, 2),
        (true, 3),
        (true, 4),
        (true, 5),
        (true, 6),
    ] {
        let mut sources = SourceMap::new();
        let entry = if named {
            "fun entry():Unit {val rounds=3;val choose=true;run(\"left\",\"right\",rounds,choose)}"
        } else {
            "fun entry():Unit {run(\"left\",\"right\",3,true)}"
        };
        let bounded = SOURCE.replace(";consume(left);consume(right)", "");
        let entry = match extra {
            1 => entry.replace("val rounds=3;", "consume(\"extra\");val rounds=3;"),
            2 => entry.replace("rounds,choose)}", "rounds,choose);consume(\"extra\")}"),
            3 => entry.replace("rounds,choose)", "3,choose)"),
            4 => entry.replace(
                "run(\"left\",\"right\",rounds,choose)",
                "run(choose,\"right\",rounds,\"left\")",
            ),
            5 => entry.replace("left", "left\\n"),
            6 => entry.replace("rounds=3", "rounds=0x3"),
            _ => entry.to_owned(),
        };
        let bounded = if extra == 4 {
            bounded.replace(
                "left:String,right:String,rounds:Int,choose:Boolean",
                "choose:Boolean,right:String,rounds:Int,left:String",
            )
        } else {
            bounded
        };
        let source = format!("{bounded}\n{entry}");
        let id = sources.add_source("entry-evidence.ko", source).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new(
            "root",
            "entry-evidence.ko",
            id,
            &parsed,
        )];
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
        let experiment = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .unwrap();
        let entries = experiment.entries(&typed).unwrap();
        assert_eq!(entries.len(), usize::from(named && extra == 0));
        if named && extra == 0 {
            assert!(experiment.owns_entry(&entries[0].clone()));
            let other = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .unwrap();
            assert!(!other.owns_entry(&entries[0]));
            assert_eq!(entries[0].contracts().len(), 4);
            assert_eq!(entries[0].ends().len(), 4);
            assert_eq!(entries[0].drops().len(), 2);
            let retyped = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            assert!(experiment.entries(&retyped).is_none());
        }
        assert_eq!(experiment.facts().len(), 1);
        if extra != 0 {
            continue;
        }
        let (checked, _) = with_domain(|| {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
        });
        let owned = checked.validate().unwrap().into_ownership();
        assert_eq!(owned.no_carry.two_sources.len(), 1);
        let run = owned.no_carry.two_sources[0].arms()[0].callable();
        let run_declaration = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.source_unit() == run.source_unit() && d.root() == run.item())
            .unwrap()
            .id();
        let contracts: Vec<_> = owned
            .call_argument_contracts()
            .iter()
            .filter(|c| {
                typed.types().call(c.call()).is_some_and(|call| {
                    call.target() == UnitCallTarget::Declaration(run_declaration)
                })
            })
            .collect();
        assert_eq!(contracts.len(), 4);
        let call = contracts[0].call();
        assert!(contracts.iter().all(|c| c.call() == call));
        let descriptors: Vec<_> = owned.loans_ending_at(call).collect();
        assert_eq!(
            descriptors.len(),
            4,
            "descriptors are not observed End events"
        );
        let ends: Vec<_> = owned
            .direct_call_loan_ends()
            .iter()
            .filter(|end| end.loan().call() == call)
            .collect();
        assert_eq!(
            ends.len(),
            if named { 4 } else { 2 },
            "named scalar roots have observed Ends; literal scalars do not"
        );
        use crate::ownership_checking::{UnitDropPoint, UnitDropTarget, UnitLoanTarget};
        for contract in contracts.iter().take(if named { 4 } else { 2 }) {
            assert_eq!(
                ends.iter()
                    .filter(|end| end.loan().argument() == contract.argument())
                    .count(),
                1
            );
        }
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.point() == UnitDropPoint::CallReturn(call))
            .collect();
        assert_eq!(drops.len(), 2);
        for contract in &contracts[..2] {
            assert_eq!(
                drops
                    .iter()
                    .filter(|drop| drop.target() == UnitDropTarget::Temporary(contract.argument()))
                    .count(),
                1
            );
        }
        for end in ends {
            assert_eq!(end.point(), UnitDropPoint::CallReturn(call));
            assert_eq!(
                end.preceding_drops(),
                0,
                "loans end before temporary owners drop"
            );
            if named
                && contracts[2..]
                    .iter()
                    .any(|c| c.argument() == end.loan().argument())
            {
                assert!(
                    matches!(end.loan().target(), UnitLoanTarget::Place(place) if place.is_root())
                );
                continue;
            }
            let loan = end.loan();
            assert!(
                contracts[..2]
                    .iter()
                    .any(|c| c.argument() == loan.argument())
            );
            assert!(
                matches!(loan.target(), crate::ownership_checking::UnitLoanTarget::Temporary(owner)
            if *owner == loan.argument())
            );
        }
        assert!(!enabled());
    }
}

#[test]
fn parameter_print_retains_real_shared_argument_contract_without_ending_input() {
    for (body, extra, accepted) in [
        ("println(item)", "", true),
        ("println(item)", "\nfun println(value:String):Unit {}", true),
        (
            "helper(item)",
            "\nfun helper(value:String):Unit {println(value)}",
            true,
        ),
        (
            "helper(\"literal\")",
            "\nfun helper(value:String):Unit {println(value)}",
            false,
        ),
        (
            "helper(item);helper(item)",
            "\nfun helper(value:String):Unit {println(value)}",
            false,
        ),
        (
            "helper(item)",
            "\nfun helper(value:String):Unit {helper(value)}",
            false,
        ),
        (
            "helper(item)",
            "\nfun helper(value:String):Unit {other(value)}\nfun other(value:String):Unit {println(value)}",
            true,
        ),
        (
            "helper(item)",
            "\nfun helper(value:String):Unit {other(value)}\nfun other(value:String):Unit {helper(value)}",
            false,
        ),
        (
            "helper(item)",
            "\nfun helper(value:String):Unit {other(value)}\nfun other(value:String):Unit {println(\"literal\")}",
            false,
        ),
        ("println(\"literal\")", "", false),
        ("println(item);println(item)", "", false),
        ("val unused=1;println(item)", "", false),
        (
            "println(item)",
            "\nfun println(value:String):Unit {val unused=1}",
            false,
        ),
    ] {
        let source = format!(
            "{}{}",
            SOURCE.replace(
                "consume(item:String):Unit {}",
                &format!("consume(item:String):Unit {{{body}}}")
            ),
            extra
        );
        let mut sources = SourceMap::new();
        let id = sources.add_source("print-probe.ko", source).unwrap();
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
        let experiment =
            super::bridge::analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .unwrap();
        assert_eq!(experiment.facts().len(), 1);
        if !accepted {
            assert!(experiment.callee_leaves().is_none(), "{body}{extra}");
            assert!(
                experiment
                    .callee_leaves_for(&experiment.facts()[0], &typed)
                    .is_none()
            );
            continue;
        }
        let Some(call) = typed.types().calls().iter().find(|c| c.prints_line()) else {
            let leaves = experiment.callee_leaves().unwrap();
            let forward = leaves
                .iter()
                .find(|leaf| leaf.body() == CalleeLeafBody::ForwardSharedParameter)
                .unwrap();
            let (call, key) = forward.source_call().unwrap();
            assert!(!typed.types().call(call).unwrap().prints_line());
            assert!(matches!(
                key.target(),
                crate::type_checking::UnitCallTarget::Declaration(_)
            ));
            assert!(
                leaves
                    .iter()
                    .any(|leaf| leaf.instance() == key && leaf.body() == CalleeLeafBody::EmptyUnit)
            );
            continue;
        };
        let owned = &experiment.retained;
        let loans: Vec<_> = owned
            .loans()
            .iter()
            .filter(|l| l.call() == call.expression())
            .collect();
        assert_eq!(loans.len(), 1);
        assert_eq!(loans[0].kind(), crate::ownership_checking::LoanKind::Shared);
        assert!(matches!(
            call.target(),
            crate::type_checking::UnitCallTarget::External(_)
        ));
        let contracts: Vec<_> = owned
            .call_argument_contracts_for(call.expression())
            .collect();
        assert_eq!(contracts.len(), 1);
        assert_eq!(
            contracts[0].kind(),
            crate::ownership_checking::UnitCallArgumentOwnershipKind::SharedLoan
        );
        let parameter = typed.types().signatures().declarations().iter().filter_map(|d| d.callable()).flat_map(|s| s.parameters()).find(|p| p.symbol().is_some_and(|symbol| matches!(loans[0].target(), crate::ownership_checking::UnitLoanTarget::Place(place) if place.is_root() && place.root()==symbol))).unwrap();
        assert_eq!(
            parameter.mode(),
            crate::type_checking::ParameterMode::Borrow
        );
        assert_eq!(contracts[0].parameter_type(), parameter.ty());
        assert!(
            !owned
                .direct_call_loan_ends()
                .iter()
                .any(|e| e.loan().call() == call.expression())
        );
        assert!(!owned.drops().iter().any(|d| d.point()
            == crate::ownership_checking::UnitDropPoint::CallReturn(call.expression())));
        let leaves = experiment.callee_leaves().unwrap();
        let printing: Vec<_> = leaves
            .iter()
            .filter(|l| l.body() == CalleeLeafBody::PrintSharedParameter)
            .collect();
        assert_eq!(printing.len(), 1);
        assert_eq!(printing[0].input_call_loan(), Some(loans[0]));
        for forward in leaves
            .iter()
            .filter(|leaf| leaf.body() == CalleeLeafBody::ForwardSharedParameter)
        {
            let (source_call, key) = forward.source_call().unwrap();
            assert_eq!(typed.types().call(source_call).unwrap().instance(), key);
            assert!(matches!(
                key.target(),
                crate::type_checking::UnitCallTarget::Declaration(_)
            ));
            let loan = forward.input_call_loan().unwrap();
            assert_eq!(loan.call(), source_call);
            assert_eq!(loan.kind(), crate::ownership_checking::LoanKind::Shared);
            assert!(
                !owned
                    .direct_call_loan_ends()
                    .iter()
                    .any(|e| e.loan().call() == source_call)
            );
        }
    }
}

#[test]
fn named_shared_string_entry_cleanup_evidence() {
    let mut sources = SourceMap::new();
    let source = format!(
        "{}\nfun entry():Unit {{val text=\"same\";val rounds=3;val choose=true;run(text,text,rounds,choose)}}",
        SOURCE.replace(";consume(left);consume(right)", "")
    );
    let id = sources.add_source("named-entry.ko", source).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "named-entry.ko", id, &parsed)];
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
    let experiment = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
        .unwrap()
        .unwrap();
    let entries = experiment.entries(&typed).unwrap();
    assert_eq!(entries.len(), 1);
    let descriptor = &entries[0];
    assert!(descriptor.strings().is_none());
    let (named_root, _, initializer_span, bytes) = descriptor.named_string().unwrap();
    assert_eq!(sources.slice(initializer_span).unwrap(), "\"same\"");
    assert_eq!(bytes, b"same");
    assert_eq!(descriptor.drops().len(), 1);
    let run = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "run")
        .unwrap()
        .id();
    let contracts: Vec<_> = experiment
        .retained
        .call_argument_contracts()
        .iter()
        .filter(|c| {
            typed
                .types()
                .call(c.call())
                .is_some_and(|call| call.target() == UnitCallTarget::Declaration(run))
        })
        .collect();
    assert_eq!(contracts.len(), 4);
    let call = contracts[0].call();
    let ends: Vec<_> = experiment
        .retained
        .direct_call_loan_ends()
        .iter()
        .filter(|e| e.loan().call() == call)
        .collect();
    use crate::ownership_checking::{UnitDropPoint, UnitDropTarget, UnitLoanTarget};
    for (index, end) in ends.iter().enumerate() {
        assert_eq!(end.point(), UnitDropPoint::CallReturn(call));
        assert_eq!(end.preceding_drops(), 0);
        assert_eq!(end.preceding_call_loan_ends(), index);
    }
    assert_ne!(contracts[0].argument(), contracts[1].argument());
    for contract in &contracts[..2] {
        let matches: Vec<_> = ends
            .iter()
            .filter(|e| e.loan().argument() == contract.argument())
            .collect();
        assert_eq!(matches.len(), 1);
        assert!(
            matches!(matches[0].loan().target(),UnitLoanTarget::Place(place) if place.is_root() && place.root()==named_root)
        );
    }
    let drop = descriptor.drops()[0];
    assert_eq!(drop.point(), UnitDropPoint::CallReturn(call));
    assert_eq!(drop.target(), UnitDropTarget::Named(named_root));
    assert_eq!(sources.slice(drop.value_origin()).unwrap(), "text");
    assert_eq!(
        experiment
            .retained
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(named_root))
            .count(),
        1
    );
    assert_eq!(ends.len(), 4);
}
