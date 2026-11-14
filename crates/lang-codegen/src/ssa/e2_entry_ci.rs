//! Full entry publisher faults; compile/list locally, execute only in CI.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::two_source_experiment::analyze_two_source_experiment,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
#[test]
#[ignore = "CI-only complete entry publication faults; local compile/list only"]
fn entry_semantic_and_final_gate_failures_publish_nothing() {
    assert_eq!(std::env::var("CI").as_deref(), Ok("true"));
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let source = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b)};remaining=remaining-1}}\nfun main():Unit {val rounds=3;val choose=true;run(\"left\",\"right\",rounds,choose)}";
    let mut sources = SourceMap::new();
    let id = sources.add_source("entry-ci.ko", source).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "entry-ci.ko", id, &parsed)];
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
    let [entry] = experiment.entries(&typed).unwrap() else {
        panic!("real complete entry")
    };
    let [fact] = experiment.facts() else {
        panic!("one complete fact")
    };
    use super::e2_entry_publication::{CiFault, Error, publish, publish_ci};
    assert!(publish(&experiment, entry, fact, &[fact], &typed).is_ok());
    for (fault, error) in [
        (CiFault::WrongStringArguments, Error::Replay),
        (CiFault::WrongRounds, Error::Replay),
        (CiFault::MissingTerminator, Error::Verify),
    ] {
        // Error contains no Program/receipt. Every retry uses fresh local emission.
        assert!(
            matches!(publish_ci(&experiment,entry,fact,&typed,fault),Err(actual) if actual==error)
        );
        assert!(publish(&experiment, entry, fact, &[fact], &typed).is_ok());
    }
}
