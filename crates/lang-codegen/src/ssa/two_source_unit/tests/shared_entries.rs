use super::*;
use crate::ssa::{e2_joint_plan::JointPlan, model::Operation};
use lang_frontend::ownership_checking::two_source_experiment::analyze_two_source_experiment;

#[test]
fn shared_caller_entries_use_exact_fact_set_and_independent_roots() {
    let alias = "fun alias():Unit {val rounds=1;val choose=true;run(\"A\",\"B\",rounds,choose)}";
    let second = source(false, "").replace("run", "run_second").replace("main", "main_second").replace("fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {println(item)}\n", "");
    for (mixed, bad_neighbor) in [(false, false), (true, false), (false, true)] {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source(
                "shared.ko",
                source(
                    false,
                    &format!("{alias}\n{}", if mixed { second.as_str() } else if bad_neighbor { "fun ordinary(source:String):Unit {borrow val b=view(source);val number=1+1;consume(b)}" } else { "" }),
                ),
            )
            .unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "shared.ko", id, &parsed)];
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
        let named = |name| {
            names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == name)
                .unwrap()
                .id()
        };
        let mut requested = vec![named("alias"), named("main")];
        if mixed {
            requested.insert(0, named("main_second"));
        }
        let result =
            prepare_two_source_units(&sources, &inputs, &names, &types, &typed, &requested);
        if bad_neighbor {
            assert!(matches!(result, Err(TwoSourceUnitError::Preflight(_))));
            continue;
        }
        let units = result.unwrap();
        assert_eq!(units.entries.len(), requested.len());
        assert_eq!(units.program.modules.len(), 1);
        assert_eq!(
            units.program.modules[0].functions.len(),
            if mixed { 11 } else { 7 }
        );
        let caller = |entry| {
            let (_, program, id) = units.parts(entry).unwrap();
            let f = program.modules[0].function(id).unwrap();
            let calls: Vec<_> = f
                .instructions
                .iter()
                .filter_map(|i| {
                    if let Operation::DirectCall { callee, .. } = i.operation {
                        Some(callee)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(calls.len(), 1);
            calls[0]
        };
        assert_eq!(caller(named("main")), caller(named("alias")));
        assert_ne!(
            units.parts(named("main")).unwrap().2,
            units.parts(named("alias")).unwrap().2
        );
        if mixed {
            assert_ne!(caller(named("main")), caller(named("main_second")));
        }
        assert!(
            prepare_two_source_units(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                &[named("main"), named("main")]
            )
            .is_err()
        );
        let single =
            prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, named("main"));
        if mixed {
            assert!(matches!(single, Err(TwoSourceUnitError::Preflight(_))));
        } else {
            let single = single.unwrap();
            assert_eq!(single.entry, named("main"));
            assert_eq!(single.program.modules[0].functions.len(), 6);
        }

        let experiment = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .unwrap();
        let handoff = experiment
            .handoff(&sources, &inputs, &names, &types, &typed)
            .unwrap();
        let facts: Vec<_> = experiment.facts().iter().collect();
        let descriptors: Vec<_> = requested
            .iter()
            .map(|id| {
                experiment
                    .entries(&typed)
                    .unwrap()
                    .iter()
                    .find(|e| e.entry() == *id)
                    .unwrap()
            })
            .collect();
        assert!(
            JointPlan::build_shared_entries_checked(
                &handoff,
                &experiment,
                &[facts[0], facts[0]],
                &descriptors
            )
            .is_err()
        );
        let other = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .unwrap();
        let foreign_facts: Vec<_> = other.facts().iter().collect();
        assert!(
            JointPlan::build_shared_entries_checked(
                &handoff,
                &experiment,
                &foreign_facts,
                &descriptors
            )
            .is_err()
        );
        let foreign_entries: Vec<_> = requested
            .iter()
            .map(|id| {
                other
                    .entries(&typed)
                    .unwrap()
                    .iter()
                    .find(|e| e.entry() == *id)
                    .unwrap()
            })
            .collect();
        assert!(
            JointPlan::build_shared_entries_checked(
                &handoff,
                &experiment,
                &facts,
                &foreign_entries
            )
            .is_err()
        );
        if mixed {
            assert!(
                JointPlan::build_shared_entries_checked(
                    &handoff,
                    &experiment,
                    &facts[..1],
                    &descriptors
                )
                .is_err(),
                "missing fact cannot sign complete plan"
            );
            let only_a: Vec<_> = descriptors
                .iter()
                .copied()
                .filter(|d| d.entry() != named("main_second"))
                .collect();
            assert!(
                JointPlan::build_shared_entries_checked(&handoff, &experiment, &facts, &only_a)
                    .is_err(),
                "no surplus authorization fact"
            );
        }
        #[cfg(feature = "e2-two-source-test-support")]
        {
            assert!(JointPlan::build_checked(&handoff, &experiment, &facts, &descriptors).is_err());
            assert!(
                JointPlan::build_shared_entries_checked(
                    &handoff,
                    &experiment,
                    &facts,
                    &descriptors
                )
                .unwrap()
                .publish()
                .is_none(),
                "legacy publisher keeps its original cardinality"
            );
        }
    }
}
