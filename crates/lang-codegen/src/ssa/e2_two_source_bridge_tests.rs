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
    for fail in [false, true] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("candidate.ko", source).unwrap();
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
        let normal =
            || check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
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
            assert!(experiment.clone().owns(fact));
            let other = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .unwrap();
            assert!(
                !other.owns(fact),
                "another real analysis is not the issuing domain"
            );
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
