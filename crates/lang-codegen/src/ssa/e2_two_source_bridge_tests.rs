//! Real cross-crate experimental admission only, not an emitted SSA receipt.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        check_compilation_unit_ownership, two_source_experiment::analyze_two_source_experiment,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn independent_two_source_bridge_does_not_enable_normal_analysis() {
    let source = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b);consume(b)};remaining=remaining-1};consume(left);consume(right)}";
    for reversed in [false, true] {
        let source = if reversed {
            source.replace("left:String,right:String", "right:String,left:String")
        } else {
            source.to_owned()
        };
        let source = if reversed {
            format!(
                "{}\nfun consume_other(item:String):Unit {{}}",
                source.replace("consume(b);consume(b)", "consume_other(b);consume(b)")
            )
        } else {
            source
        };
        for fail in [false, true] {
            let mut sources = SourceMap::new();
            let id = sources.add_source("candidate.ko", source.as_str()).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
            let bad_id = sources
            .add_source(
                "zz-bad.ko",
                "fun take(own s:String):Unit {}\nfun bad():Unit {val s=\"s\";take(s);consume(s)}",
            )
            .unwrap();
            let bad = parse_file(&sources, &lex(&sources, bad_id).unwrap()).unwrap();
            let mut inputs = vec![SourceUnitInput::new("root", "candidate.ko", id, &parsed)];
            if fail {
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
            let normal = || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            };
            assert!(
                normal().validate().is_err(),
                "feature alone never authorizes ordinary analysis"
            );
            let experiment =
                analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed).unwrap();
            assert_eq!(experiment.is_none(), fail);
            if let Some(experiment) = experiment {
                let [fact] = experiment.facts() else {
                    panic!("one real closed fact")
                };
                assert!(experiment.owns(fact));
                let emitted = super::e2_callee_leaves::emit(&experiment, &typed).unwrap();
                crate::llvm::render_verified_program(&emitted.program).unwrap();
                assert_eq!(emitted.functions.len(), if reversed { 3 } else { 2 });
                for leaf in experiment.callee_leaves().unwrap() {
                    let (_, id) = emitted
                        .functions
                        .iter()
                        .find(|(key, _)| key == leaf.instance())
                        .unwrap();
                    let function = emitted.program.modules[0].function(*id).unwrap();
                    assert_eq!(function.blocks.len(), 1);
                    assert!(function.instructions.is_empty());
                    let block = &function.blocks[0];
                    let super::model::EntityId::Loan(parameter) = block.parameters[0] else {
                        panic!("shared parameter")
                    };
                    assert_eq!(
                        block.terminator.as_ref().unwrap().origin,
                        super::model::Origin::Source(leaf.span())
                    );
                    match leaf.body() {
                        lang_frontend::ownership_checking::two_source_experiment::CalleeLeafBody::SharedIdentity => {
                            assert_eq!(function.borrow_return, Some(0));
                            assert!(matches!(block.terminator.as_ref().unwrap().kind, super::model::TerminatorKind::BorrowReturn { loan } if loan == parameter));
                        }
                        lang_frontend::ownership_checking::two_source_experiment::CalleeLeafBody::EmptyUnit => assert!(matches!(&block.terminator.as_ref().unwrap().kind, super::model::TerminatorKind::Return { values } if values.is_empty())),
                    }
                }
                let fragments = super::e2_arm_fragments::emit(&experiment, fact, &typed).unwrap();
                crate::llvm::render_verified_program(&fragments.leaves.program).unwrap();
                assert_eq!(fragments.columns, if reversed { [1, 0] } else { [0, 1] });
                assert_ne!(fragments.arms[0], fragments.arms[1]);
                for (arm_index, id) in fragments.arms.iter().enumerate() {
                    use super::model::{EntityId, Operation, Origin, TerminatorKind};
                    let function = fragments.leaves.program.modules[0].function(*id).unwrap();
                    assert_eq!(function.blocks.len(), 1);
                    let block = &function.blocks[0];
                    assert_eq!(block.parameters.len(), 2);
                    for parameter in &block.parameters {
                        let super::model::EntityType::Loan {
                            kind: super::model::LoanKind::Shared,
                            target,
                        } = function.entity(*parameter).unwrap().ty
                        else {
                            panic!("Shared parameter")
                        };
                        assert!(matches!(
                            fragments.leaves.program.modules[0]
                                .types
                                .get(target.index()),
                            Some(super::model::SsaTypeKind::StringOwner)
                        ));
                    }
                    let expected_origin = Origin::Synthetic {
                        anchor: parsed
                            .ast()
                            .statements()
                            .get(fact.arms()[arm_index].body().statement())
                            .unwrap()
                            .span(),
                        reason: "detached verified E2 arm".to_owned(),
                    };
                    assert_eq!(function.origin, expected_origin);
                    assert_eq!(block.origin, expected_origin);
                    assert_eq!(block.terminator.as_ref().unwrap().origin, expected_origin);
                    let EntityId::Loan(parent) = block.parameters[fragments.columns[arm_index]]
                    else {
                        panic!("shared root")
                    };
                    let instructions = block
                        .instructions
                        .iter()
                        .map(|id| function.instruction(*id).unwrap())
                        .collect::<Vec<_>>();
                    let calls = &fact.calls()[arm_index];
                    assert_eq!(instructions.len(), calls.len() + 1);
                    let [EntityId::Loan(child)] = instructions[0].results.as_slice() else {
                        panic!("child")
                    };
                    assert!(!block.parameters.contains(&EntityId::Loan(*child)));
                    for (position, ((expression, key), instruction)) in
                        calls.iter().zip(&instructions).enumerate()
                    {
                        let target = fragments
                            .leaves
                            .functions
                            .iter()
                            .find(|(instance, _)| instance == key)
                            .unwrap()
                            .1;
                        assert_eq!(
                            instruction.origin,
                            Origin::Source(
                                parsed
                                    .ast()
                                    .expressions()
                                    .get(expression.expression())
                                    .unwrap()
                                    .span()
                            )
                        );
                        if position == 0 {
                            assert!(
                                matches!(&instruction.operation, Operation::BorrowCall { callee, arguments, source } if *callee==target && *source==parent && arguments==&vec![EntityId::Loan(parent)])
                            );
                        } else {
                            assert!(instruction.results.is_empty());
                            assert!(
                                matches!(&instruction.operation, Operation::DirectCall { callee, arguments, receiver: None } if *callee==target && arguments==&vec![EntityId::Loan(*child)])
                            );
                        }
                    }
                    let end = instructions.last().unwrap();
                    assert!(
                        matches!(end.operation, Operation::BorrowEnd { loan } if loan == *child)
                    );
                    let last = fact.arms()[arm_index].consumers().last().unwrap().0;
                    assert_eq!(
                        end.origin,
                        Origin::Source(
                            parsed
                                .ast()
                                .statements()
                                .get(last.statement())
                                .unwrap()
                                .span()
                        )
                    );
                    assert!(
                        matches!(&block.terminator.as_ref().unwrap().kind, TerminatorKind::Return { values } if values.is_empty())
                    );
                }
                let plan = super::e2_two_source_plan::Plan::new(&experiment, fact, &typed).unwrap();
                assert!(experiment.owns(plan.fact()));
                assert_eq!(plan.source_roles(), if reversed { [1, 0] } else { [0, 1] });
                let declaration = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.id() == plan.callable())
                    .unwrap();
                assert_eq!(
                    declaration.source_unit(),
                    fact.arms()[0].callable().source_unit()
                );
                assert_eq!(declaration.root(), fact.arms()[0].callable().item());
                let retyped = check_compilation_unit_types(&sources, &inputs, &names, &types)
                    .unwrap()
                    .validate()
                    .unwrap();
                assert!(!experiment.matches_types(&retyped));
                assert!(super::e2_callee_leaves::emit(&experiment, &retyped).is_none());
                assert!(super::e2_arm_fragments::emit(&experiment, fact, &retyped).is_none());
                assert!(
                    super::e2_two_source_plan::Plan::new(&experiment, fact, &retyped).is_none()
                );
                assert!(experiment.clone().owns(fact));
                let other =
                    analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                        .unwrap();
                assert!(
                    !other.owns(fact),
                    "another real analysis is not the issuing domain"
                );
                assert!(super::e2_two_source_plan::Plan::new(&other, fact, &typed).is_none());
                assert!(super::e2_arm_fragments::emit(&other, fact, &typed).is_none());
                assert!(!other.owns_leaf(&experiment.callee_leaves().unwrap()[0]));
                assert_ne!(fact.arms()[0].source(), fact.arms()[1].source());
                assert_eq!(
                    fact.branch().source_unit(),
                    fact.arms()[0].callable().source_unit()
                );
                let legacy = normal();
                for (arm, calls) in fact.arms().iter().zip(fact.calls()) {
                    assert!(
                        !legacy.owns_no_carry_loop(arm),
                        "metadata access is not legacy arm authority"
                    );
                    let expected = std::iter::once(arm.initializer())
                        .chain(arm.consumers().iter().map(|(_, call, _)| *call))
                        .collect::<Vec<_>>();
                    assert_eq!(calls.len(), expected.len());
                    for ((expression, key), expected) in calls.iter().zip(expected) {
                        assert_eq!(*expression, expected);
                        assert_eq!(key, typed.types().call(*expression).unwrap().instance());
                    }
                }
            }
            assert!(
                normal().validate().is_err(),
                "dedicated analysis cannot leave activation behind"
            );
        }
    }
}

#[test]
fn unsupported_actual_leaf_bodies_do_not_become_placeholders() {
    let base = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b)};remaining=remaining-1}}";
    for source in [
        base.replace(
            "consume(item:String):Unit {}",
            "consume(item:String):Unit {println(item)}",
        ),
        base.replace("from source=source", "from source=(source)"),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("leaf.ko", &source).unwrap();
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
        let experiment = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .unwrap();
        assert_eq!(
            experiment.facts().len(),
            1,
            "valid facts remain independent of leaf lowering support"
        );
        assert!(
            experiment
                .function_shell(&experiment.facts()[0], &typed)
                .is_some(),
            "caller shell does not promise callee body support"
        );
        assert!(super::e2_function::emit(&experiment, &experiment.facts()[0], &typed).is_none());
        assert_eq!(
            super::e2_publication::publish(
                &experiment,
                &experiment.facts()[0],
                &[&experiment.facts()[0]],
                &typed
            )
            .err(),
            Some(super::e2_publication::Error::Emission)
        );
        assert!(
            experiment
                .handoff(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .for_fact(&experiment.facts()[0])
                .is_none()
        );
        assert!(experiment.callee_leaves().is_none());
        assert!(super::e2_callee_leaves::emit(&experiment, &typed).is_none());
        assert!(
            super::e2_arm_fragments::emit(&experiment, &experiment.facts()[0], &typed).is_none()
        );
        assert!(
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}

#[test]
fn e2_two_source_complete_function_shell_rejects_uncovered_body_and_abi() {
    let base = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b)};remaining=remaining-1}}";
    let cases = [
        (base.to_owned(), Some([0, 1, 2, 3])),
        (
            base.replace(
                "left:String,right:String,rounds:Int,choose:Boolean",
                "choose:Boolean,right:String,rounds:Int,left:String",
            ),
            Some([3, 1, 2, 0]),
        ),
        (
            base.replace("{var remaining", "{consume(left);var remaining"),
            None,
        ),
        (
            base.replace("remaining-1}}", "remaining-1};consume(right)}"),
            None,
        ),
        (
            base.replace("choose:Boolean):Unit", "choose:Boolean,extra:Int):Unit"),
            None,
        ),
        (base.replace("rounds:Int", "own rounds:Int"), None),
    ];
    for (source, expected) in cases {
        let mut sources = SourceMap::new();
        let id = sources.add_source("shell.ko", &source).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "shell.ko", id, &parsed)];
        let (env, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
            .unwrap()
            .validate()
            .unwrap();
        let typecheck = || {
            check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap()
        };
        let typed = typecheck();
        let analyze = || {
            analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .unwrap()
        };
        let experiment = analyze();
        let [fact] = experiment.facts() else {
            panic!("one local closed fact retained")
        };
        let published = super::e2_publication::publish(&experiment, fact, &[fact], &typed);
        assert_eq!(published.is_ok(), expected.is_some());
        if let Ok(published) = published {
            assert_eq!(published.callable(), experiment.callable(fact).unwrap());
            assert!(
                published.program().modules[0]
                    .function(published.caller())
                    .is_some()
            );
            crate::llvm::render_verified_program(published.program()).unwrap();
            assert!(super::e2_publication::publish(&experiment, fact, &[], &typed).is_err());
            assert!(
                super::e2_publication::publish(&experiment, fact, &[fact, fact], &typed).is_err()
            );
        }
        let emitted = super::e2_function::emit(&experiment, fact, &typed);
        assert_eq!(emitted.is_some(), expected.is_some(), "{source}");
        if let Some(emitted) = emitted {
            super::e2_function_tests::assert_caller(
                &emitted,
                experiment.function_shell(fact, &typed).unwrap(),
            );
            crate::llvm::render_verified_program(&emitted.fragments.leaves.program).unwrap();
            assert_eq!(
                emitted.fragments.leaves.program.modules[0]
                    .function(emitted.caller)
                    .unwrap()
                    .blocks
                    .len(),
                9
            );
        }
        let shell = experiment.function_shell(fact, &typed);
        assert_eq!(shell.map(|s| s.roles()), expected, "{source}");
        let handoff = experiment
            .handoff(&sources, &inputs, &names, &types, &typed)
            .unwrap();
        assert_eq!(
            handoff.for_fact(fact).is_some(),
            expected.is_some(),
            "only complete source shells can select binding controls"
        );
        if let Some(shell) = shell {
            assert_eq!(shell.callable(), experiment.callable(fact).unwrap());
            let signature = typed
                .types()
                .signatures()
                .declaration(shell.callable())
                .unwrap()
                .callable()
                .unwrap();
            assert_eq!(signature.parameters().len(), 4);
            for (role, symbol) in shell.roles().into_iter().zip(shell.symbols()) {
                assert_eq!(signature.parameters()[role].symbol(), Some(symbol));
            }
            assert!(!shell.symbols().contains(&shell.counter()));
            assert_eq!(shell.body().source_id(), id);
            for control in shell.controls() {
                assert_eq!(control.source_unit(), fact.branch().source_unit());
                let span = parsed
                    .ast()
                    .expressions()
                    .get(control.expression())
                    .unwrap()
                    .span();
                assert!(span.start() >= shell.body().start() && span.end() <= shell.body().end());
            }
        }
        assert!(super::e2_function::emit(&experiment, fact, &typecheck()).is_none());
        assert!(experiment.function_shell(fact, &typecheck()).is_none());
        let other = analyze();
        if expected.is_some() {
            let emitted = super::e2_function::emit(&experiment, fact, &typed).unwrap();
            assert!(
                super::e2_replay::replay(&emitted, &other, &other.facts()[0], &typed).is_none(),
                "equivalent new analysis is not the issuing chain"
            );
        }
        assert!(super::e2_function::emit(&other, fact, &typed).is_none());
        assert!(other.function_shell(fact, &typed).is_none());
        assert!(
            super::e2_publication::publish(&experiment, fact, &[&other.facts()[0]], &typed)
                .is_err()
        );
        assert!(super::e2_publication::publish(&experiment, fact, &[fact], &typecheck()).is_err());
        assert!(experiment.owns(fact));
        assert!(
            super::e2_arm_fragments::emit(&experiment, fact, &typed).is_some(),
            "shell refusal must not revoke local helpers"
        );
        assert!(
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}

#[test]
fn e2_real_source_entry_publication_executes_without_synthetic_driver() {
    for (rounds, choose, distinct, multiple, unsupported, ordinary) in [
        (0, false, false, false, false, false),
        (3, false, false, false, false, false),
        (3, true, false, false, false, false),
        (3, true, true, false, false, false),
        (3, false, false, true, false, false),
        (3, false, false, true, true, false),
        (3, false, false, false, false, true),
    ] {
        let source = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {{var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(left);consume(a)}}else{{borrow val b=view(right);consume(b)}};remaining=remaining-1}}}}\nfun main():Unit {{val rounds={rounds};val choose={choose};run(\"left\",\"right\",rounds,choose)}}"
        );
        let source = if distinct {
            format!(
                "{}\nfun other(item:String):Unit {{}}",
                source.replace("consume(b)", "other(b)")
            )
        } else {
            source
        };
        let source = if multiple {
            let second = source
                .replace("view", "view_second")
                .replace("consume", "consume_second")
                .replace("run", "run_second")
                .replace("main", "main_second");
            let second = if unsupported {
                second.replace(
                    "fun consume_second(item:String):Unit {}",
                    "fun consume_second(item:String):Unit {val unused=1}",
                )
            } else {
                second
            };
            format!("{source}\n{second}")
        } else {
            source
        };
        let source = if ordinary {
            format!(
                "{source}\nfun ordinary(source:String):Unit {{borrow val b=view(source);val number=1+1;consume(b)}}"
            )
        } else {
            source
        };
        let mut sources = SourceMap::new();
        let id = sources.add_source("entry.ko", source).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "entry.ko", id, &parsed)];
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
        assert_eq!(experiment.facts().len(), if multiple { 2 } else { 1 });
        let fact = &experiment.facts()[0];
        let entries = experiment.entries(&typed).unwrap();
        assert_eq!(entries.len(), if multiple { 2 } else { 1 });
        let entry = &entries[0];
        let handoff = experiment
            .handoff(&sources, &inputs, &names, &types, &typed)
            .unwrap();
        assert!(handoff.owns(fact));
        let parsed_sources =
            super::lowering_support::parsed_by_source_unit(handoff.inputs(), handoff.names())
                .unwrap();
        let preflight = super::unit_lower::preflight::validate(
            &parsed_sources,
            handoff.names(),
            handoff.types().types(),
            &handoff,
        );
        assert_eq!(
            preflight.as_ref().unwrap_err().span,
            Some(
                experiment
                    .function_shell(fact, &typed)
                    .unwrap()
                    .control_spans()[1]
            )
        );
        assert!(
            matches!(
                preflight,
                Err(super::LoweringError {
                    kind: super::LoweringErrorKind::UnsupportedNode,
                    ..
                })
            ),
            "existing binding gate must not gain a two-source exemption: {preflight:?}"
        );
        let selected = handoff.for_fact(fact).unwrap();
        let selected_preflight = super::unit_lower::preflight::validate(
            &parsed_sources,
            selected.names(),
            selected.types().types(),
            &selected,
        );
        if multiple || ordinary {
            let rejected = selected_preflight.unwrap_err();
            assert_eq!(rejected.kind, super::LoweringErrorKind::UnsupportedNode);
            let expected_span = if multiple {
                experiment
                    .function_shell(&experiment.facts()[1], &typed)
                    .unwrap()
                    .control_spans()[1]
            } else {
                parsed
                    .ast()
                    .expressions()
                    .iter()
                    .find_map(|(_, node)| {
                        matches!(
                            node.payload(),
                            lang_frontend::parser::Expression::Binary {
                                operator: lang_frontend::parser::BinaryOperator::Add,
                                ..
                            }
                        )
                        .then_some(node.span())
                    })
                    .unwrap()
            };
            assert_eq!(
                rejected.span,
                Some(expected_span),
                "unselected or ordinary unsupported binding retains its exact frontier"
            );
        } else {
            selected_preflight.unwrap();
        }
        assert!(std::ptr::eq(handoff.sources(), &sources));
        assert!(std::ptr::eq(handoff.inputs(), inputs.as_slice()));
        assert!(std::ptr::eq(handoff.names(), &names));
        assert!(std::ptr::eq(handoff.types(), &typed));
        assert!(
            experiment
                .clone()
                .handoff(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .owns(fact)
        );
        let empty_sources = SourceMap::new();
        assert!(
            experiment
                .handoff(&empty_sources, &inputs, &names, &types, &typed)
                .is_err()
        );
        assert!(
            experiment
                .handoff(&sources, &[], &names, &types, &typed)
                .is_err()
        );
        let emitted = super::e2_entry::emit(&experiment, fact, entry, &typed).unwrap();
        super::e2_entry_replay::replay(&emitted, &experiment, fact, &typed).unwrap();
        // Ordinary single-callable publication/replay does not silently widen its coverage.
        assert!(super::e2_replay::replay(&emitted.function, &experiment, fact, &typed).is_none());
        let other = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .unwrap();
        assert!(super::e2_entry::emit(&other, &other.facts()[0], entry, &typed).is_none());
        assert!(!handoff.owns(&other.facts()[0]));
        assert!(handoff.for_fact(&other.facts()[0]).is_none());
        assert!(
            super::e2_entry_replay::replay(&emitted, &other, &other.facts()[0], &typed).is_none()
        );
        use super::e2_entry_publication::{Error, publish};
        assert!(matches!(
            publish(&experiment, entry, fact, &[], &typed),
            Err(Error::Selection)
        ));
        assert!(matches!(
            publish(&experiment, entry, fact, &[fact, fact], &typed),
            Err(Error::Selection)
        ));
        assert!(matches!(
            publish(
                &other,
                entry,
                &other.facts()[0],
                &[&other.facts()[0]],
                &typed
            ),
            Err(Error::Required)
        ));
        assert!(matches!(
            publish(&experiment, entry, fact, &[&other.facts()[0]], &typed),
            Err(Error::Selection)
        ));
        let retyped = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        assert!(matches!(
            publish(&experiment, entry, fact, &[fact], &retyped),
            Err(Error::Required)
        ));
        assert!(
            experiment
                .handoff(&sources, &inputs, &names, &types, &retyped)
                .is_err()
        );
        let published = publish(&experiment, entry, fact, &[fact], &typed).unwrap();
        assert_eq!(published.declaration(), entry.entry());
        let coverage = published.coverage();
        assert_eq!(coverage.root(), (entry.entry(), published.entry()));
        assert_eq!(
            coverage.called().len(),
            3 + usize::from(distinct),
            "run plus actual view/consume instances"
        );
        assert!(
            coverage
                .called()
                .iter()
                .any(|(key, _)| key == entry.instance())
        );
        assert!(
            coverage
                .helpers()
                .iter()
                .all(|id| coverage.called().iter().all(|(_, source)| source != id))
        );
        assert_eq!(
            published.program().modules[0].functions.len(),
            coverage.called().len() + 3
        );

        let llvm =
            crate::llvm::render_verified_program_with_entry(published.program(), published.entry())
                .unwrap();
        super::e2_native_tests::execute_llvm(&llvm, b"");
        if multiple {
            let second = &entries[1];
            let second_fact = experiment
                .facts()
                .iter()
                .find(|f| {
                    Some(second.instance().target())
                        == experiment
                            .callable(f)
                            .map(lang_frontend::type_checking::UnitCallTarget::Declaration)
                })
                .unwrap();
            assert!(publish(&experiment, entry, second_fact, &[second_fact], &typed).is_err());
            assert!(
                experiment
                    .callee_leaves_for(&other.facts()[0], &typed)
                    .is_none()
            );
            assert!(experiment.callee_leaves_for(fact, &retyped).is_none());
            if unsupported {
                assert!(experiment.callee_leaves().is_none());
                assert!(experiment.callee_leaves_for(second_fact, &typed).is_none());
                assert!(handoff.for_fact(second_fact).is_none());
                assert!(super::e2_entry::emit(&experiment, second_fact, second, &typed).is_none());
                assert!(publish(&experiment, second, second_fact, &[second_fact], &typed).is_err());
                continue;
            }
            let second_package =
                publish(&experiment, second, second_fact, &[second_fact], &typed).unwrap();
            assert_eq!(second_package.coverage().called().len(), 3);
            assert!(second_package.coverage().called().iter().all(|(key, _)| {
                !published
                    .coverage()
                    .called()
                    .iter()
                    .any(|(first, _)| first == key)
            }));
            let llvm = crate::llvm::render_verified_program_with_entry(
                second_package.program(),
                second_package.entry(),
            )
            .unwrap();
            super::e2_native_tests::execute_llvm(&llvm, b"");
        }
    }
}
