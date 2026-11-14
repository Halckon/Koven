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
    fixture(false, false, false);
}
fn fixture(checked: bool, joint: bool, object_commit: bool) {
    let source = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b)};remaining=remaining-1}}\nfun main():Unit {val rounds=3;val choose=true;run(\"left\",\"right\",rounds,choose)}";
    let source = if joint {
        let second = source
            .lines()
            .skip(2)
            .collect::<Vec<_>>()
            .join("\n")
            .replace("run", "run_second")
            .replace("main", "main_second");
        format!("{source}\n{second}")
    } else {
        source.to_owned()
    };
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
    if joint {
        use super::e2_arm_fragments::{JointCiFault, emit_for_facts};
        let handoff = experiment
            .handoff(&sources, &inputs, &names, &types, &typed)
            .unwrap();
        let facts: Vec<_> = experiment.facts().iter().collect();
        let entries: Vec<_> = experiment.entries(&typed).unwrap().iter().collect();
        assert_eq!(facts.len(), 2);
        assert_eq!(entries.len(), 2);
        let rebuild = || {
            emit_for_facts(&handoff, &experiment, &facts)
                .unwrap()
                .append_callers(&typed)
                .unwrap()
        };
        if object_commit {
            let package = rebuild().publish_entries(&entries, &typed).unwrap();
            let directory = crate::native_tests::TestDirectory::create();
            let output = directory.join("blocked.o");
            std::fs::create_dir(&output).unwrap();
            let sentinel = output.join("keep");
            std::fs::write(&sentinel, b"original directory content").unwrap();
            for entry in &entries {
                let error =
                    crate::native::emit_experimental_e2_object(&package, &handoff, entry, &output)
                        .unwrap_err();
                assert_eq!(error.kind(), crate::native::NativeObjectErrorKind::Backend);
                assert!(
                    error.to_string().contains("commit sibling object"),
                    "{error}"
                );
                assert_eq!(
                    std::fs::read(&sentinel).unwrap(),
                    b"original directory content"
                );
                assert_eq!(std::fs::read_dir(&output).unwrap().count(), 1);
                let names: Vec<_> = std::fs::read_dir(output.parent().unwrap())
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect();
                assert_eq!(names, vec![std::ffi::OsString::from("blocked.o")]);
            }
            std::fs::remove_file(&sentinel).unwrap();
            std::fs::remove_dir(&output).unwrap();
            for entry in &entries {
                crate::native::emit_experimental_e2_object(&package, &handoff, entry, &output)
                    .unwrap();
                crate::test_support::assert_native_object(&std::fs::read(&output).unwrap());
                assert_eq!(
                    std::fs::read_dir(output.parent().unwrap()).unwrap().count(),
                    1
                );
            }
            return;
        }
        assert!(rebuild().publish_entries(&entries, &typed).is_some());
        for fault in [
            JointCiFault::WrongSecondCaller,
            JointCiFault::DuplicateEntryLocator,
            JointCiFault::PartialEntrySelection,
        ] {
            assert!(
                rebuild()
                    .publish_entries_ci(&entries, &typed, fault)
                    .is_none()
            );
            let retry = rebuild().publish_entries(&entries, &typed).unwrap();
            super::verify::verify_program(retry.program()).unwrap();
            for entry in &entries {
                assert!(retry.entry(entry, &typed).is_some());
            }
        }
        return;
    }
    let [entry] = experiment.entries(&typed).unwrap() else {
        panic!("real complete entry")
    };
    let [fact] = experiment.facts() else {
        panic!("one complete fact")
    };
    use super::e2_entry_publication::{
        CheckedError, CiFault, Error, publish, publish_checked, publish_checked_ci, publish_ci,
    };
    if checked {
        let handoff = experiment
            .handoff(&sources, &inputs, &names, &types, &typed)
            .unwrap();
        assert!(publish_checked(&handoff, &experiment, entry, fact).is_ok());
        for (fault, error) in [
            (CiFault::WrongStringArguments, Error::Replay),
            (CiFault::WrongRounds, Error::Replay),
            (CiFault::MissingTerminator, Error::Verify),
        ] {
            assert!(matches!(
                publish_checked_ci(&handoff, &experiment, entry, fact, fault),
                Err(CheckedError::Publication(actual)) if actual == error
            ));
            // No checked package escaped; retry traverses preflight and fresh emission again.
            let retry = publish_checked(&handoff, &experiment, entry, fact).unwrap();
            super::verify::verify_program(retry.publication().program()).unwrap();
        }
        return;
    }
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

#[test]
#[ignore = "CI-only checked entry publication faults; local compile/list only"]
fn checked_entry_failures_publish_nothing() {
    assert_eq!(std::env::var("CI").as_deref(), Ok("true"));
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    fixture(true, false, false);
}

#[test]
#[ignore = "CI-only joint entry identity/coverage failures; local compile/list only"]
fn joint_entry_mapping_and_partial_failures_publish_nothing() {
    assert_eq!(std::env::var("CI").as_deref(), Ok("true"));
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    fixture(false, true, false);
}

#[test]
#[ignore = "CI-only real object commit failure; local compile/list only"]
fn joint_object_commit_failure_cleans_temporary_and_retries() {
    assert_eq!(std::env::var("CI").as_deref(), Ok("true"));
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    fixture(false, true, true);
}
