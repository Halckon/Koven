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
        assert!(experiment.callee_leaves().is_none());
        assert!(super::e2_callee_leaves::emit(&experiment, &typed).is_none());
        assert!(
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}
