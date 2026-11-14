//! Real observable restricted source consumers, through the joint plan and native transaction.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::two_source_experiment::analyze_two_source_experiment,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
#[test]
fn e2_printing_parameter_observes_original_branch_and_iteration_count() {
    for rounds in [0, 3] {
        for choose in [false, true] {
            let first = format!(
                "fun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {{var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(left);consume(a)}}else{{borrow val b=view(right);consume(b)}};remaining=remaining-1}}}}\nfun main():Unit {{val rounds={rounds};val choose={choose};run(\"left\",\"right\",rounds,choose)}}"
            );
            let second = first
                .replace("run", "run_second")
                .replace("main", "main_second");
            let source = format!(
                "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{println(item)}}\n{first}\n{second}"
            );
            let mut sources = SourceMap::new();
            let id = sources.add_source("print.ko", source).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
            let inputs = [SourceUnitInput::new("root", "print.ko", id, &parsed)];
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
                analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .unwrap();
            let handoff = experiment
                .handoff(&sources, &inputs, &names, &types, &typed)
                .unwrap();
            let facts: Vec<_> = experiment.facts().iter().collect();
            let entries: Vec<_> = experiment.entries(&typed).unwrap().iter().rev().collect();
            assert_eq!(facts.len(), 2);
            assert_eq!(entries.len(), 2);
            let package =
                super::e2_joint_plan::JointPlan::build(&handoff, &experiment, &facts, &entries)
                    .unwrap()
                    .publish()
                    .unwrap();
            let expected = if choose { "left\n" } else { "right\n" }.repeat(rounds);
            for entry in entries {
                super::e2_native_tests::execute_published(
                    &package,
                    &handoff,
                    entry,
                    expected.as_bytes(),
                );
            }
        }
    }
}

#[test]
fn e2_ordinary_forward_call_closure_uses_real_helper_identity() {
    for choose in [false, true] {
        let source = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{helper(item)}}\nfun helper(value:String):Unit {{println(value)}}\nfun silent(value:String):Unit {{}}\nfun consume_silent(item:String):Unit {{silent(item)}}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {{var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(left);consume(a)}}else{{borrow val b=view(right);consume_silent(b)}};remaining=remaining-1}}}}\nfun main():Unit {{val rounds=3;val choose={choose};run(\"left\",\"right\",rounds,choose)}}"
        );
        let mut sources = SourceMap::new();
        let id = sources.add_source("forward.ko", source).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "forward.ko", id, &parsed)];
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
        let handoff = experiment
            .handoff(&sources, &inputs, &names, &types, &typed)
            .unwrap();
        let facts: Vec<_> = experiment.facts().iter().collect();
        let entries: Vec<_> = experiment.entries(&typed).unwrap().iter().collect();
        let required = super::e2_replay::required_keys(&experiment, facts[0], &typed).unwrap();
        assert_eq!(
            required.len(),
            5,
            "view, two wrappers and two distinct helpers"
        );
        let package =
            super::e2_joint_plan::JointPlan::build(&handoff, &experiment, &facts, &entries)
                .unwrap()
                .publish()
                .unwrap();
        assert_eq!(package.program().modules[0].functions.len(), 9);
        super::e2_native_tests::execute_published(
            &package,
            &handoff,
            entries[0],
            if choose { b"left\nleft\nleft\n" } else { b"" },
        );
        let scoped =
            super::e2_entry_publication::publish(&experiment, entries[0], facts[0], &facts, &typed)
                .unwrap();
        super::verify::verify_program(scoped.program()).unwrap();
    }
}
