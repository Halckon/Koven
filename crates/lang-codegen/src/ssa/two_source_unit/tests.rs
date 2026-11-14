use super::*;
use lang_frontend::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    type_checking::{check_compilation_unit_types, standard_environments},
};

fn source(same_text: bool, extra: &str) -> String {
    let right = if same_text { "\"left\"" } else { "\"right\"" };
    format!(
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{println(item)}}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {{var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(left);consume(a)}}else{{borrow val b=view(right);consume(b)}};remaining=remaining-1}}}}\nfun main():Unit {{val rounds=3;val choose=false;run(\"left\",{right},rounds,choose)}}\n{extra}"
    )
}

#[test]
fn explicit_capability_checks_whole_unit_and_source_identity() {
    for same_text in [false, true] {
        let other = source(false, "").replace("run", "run_second").replace("main", "main_second").replace("fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {println(item)}\n", "");
        for extra in [
            "",
            "fun ordinary(source:String):Unit {borrow val b=view(source);val number=1+1;consume(b)}",
            other.as_str(),
        ] {
            let mut sources = SourceMap::new();
            let id = sources
                .add_source("formal.ko", source(same_text, extra))
                .unwrap();
            let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
            let inputs = [SourceUnitInput::new("root", "formal.ko", id, &parsed)];
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
            // The original source entry, not a synthetic driver or borrowed experiment descriptor.
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "main")
                .unwrap()
                .id();
            assert!(
                lang_frontend::ownership_checking::check_compilation_unit_ownership(
                    &sources, &inputs, &names, &types, &typed
                )
                .unwrap()
                .validate()
                .is_err(),
                "ordinary domain must remain closed"
            );
            let result = prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry);
            if extra.is_empty() {
                let unit = result.expect("bounded real source must prepare");
                assert_eq!(unit.entry, entry);
                super::super::verify::verify_program(&unit.program).unwrap();
                let directory = crate::native_tests::TestDirectory::create();
                let object = directory.join("formal.o");
                crate::emit_native_two_source_unit_object(&unit, &object).unwrap();
                crate::test_support::assert_native_object(&std::fs::read(&object).unwrap());
                assert_eq!(directory.snapshot().len(), 1);
            } else {
                assert!(matches!(result, Err(TwoSourceUnitError::Preflight(_))));
            }
            // Reusing an unrelated input collection cannot mint a capability.
            let empty: [SourceUnitInput<'_>; 0] = [];
            assert!(
                prepare_two_source_unit(&sources, &empty, &names, &types, &typed, entry).is_err()
            );
        }
    }
}

#[test]
fn explicit_capability_executes_original_object() {
    objects(false);
}

#[test]
#[ignore = "CI-only real commit failure; local compile/list only"]
fn explicit_capability_commit_failure_is_atomic() {
    assert_eq!(std::env::var("CI").as_deref(), Ok("true"));
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    objects(true);
}

fn objects(commit_failure: bool) {
    for same_text in [false, true] {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("execute.ko", source(same_text, ""))
            .unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "execute.ko", id, &parsed)];
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
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "main")
            .unwrap()
            .id();
        let unit =
            prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry).unwrap();
        let directory = crate::native_tests::TestDirectory::create();
        let object = directory.join("formal.o");
        if commit_failure {
            std::fs::create_dir(&object).unwrap();
            let sentinel = object.join("keep");
            std::fs::write(&sentinel, b"old content").unwrap();
            let error = crate::emit_native_two_source_unit_object(&unit, &object).unwrap_err();
            assert_eq!(error.kind(), crate::NativeObjectErrorKind::Backend);
            assert!(error.to_string().contains("commit sibling object"));
            assert_eq!(std::fs::read(&sentinel).unwrap(), b"old content");
            assert_eq!(std::fs::read_dir(&object).unwrap().count(), 1);
            assert_eq!(
                std::fs::read_dir(object.parent().unwrap()).unwrap().count(),
                1
            );
            std::fs::remove_file(&sentinel).unwrap();
            std::fs::remove_dir(&object).unwrap();
            crate::emit_native_two_source_unit_object(&unit, &object).unwrap();
            crate::test_support::assert_native_object(&std::fs::read(&object).unwrap());
            assert_eq!(directory.snapshot().len(), 1);
            continue;
        }
        crate::emit_native_two_source_unit_object(&unit, &object).unwrap();
        let executable = directory.join("formal");
        let linked = std::process::Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        super::super::native_bounded::execute_binary(
            &executable,
            if same_text {
                b"left\nleft\nleft\n"
            } else {
                b"right\nright\nright\n"
            },
        );
    }
}
