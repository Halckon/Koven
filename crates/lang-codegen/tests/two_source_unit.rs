//! Public opt-in capability exercised against the non-test codegen library.
#![cfg(feature = "two-source-unit")]

#[path = "../src/ssa/native_bounded.rs"]
mod native_bounded;

use lang_codegen::{emit_native_two_source_unit_object, prepare_two_source_unit};
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "koven-two-source-public-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn public_capability_emits_and_executes_without_test_observers() {
    let text = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {println(item)}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b)};remaining=remaining-1}}\nfun main():Unit {val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)}";
    let mut sources = SourceMap::new();
    let id = sources.add_source("public.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "public.ko", id, &parsed)];
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
    let unit = prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry).unwrap();
    let directory = Directory::new();
    let object = directory.0.join("public.o");
    emit_native_two_source_unit_object(&unit, &object).unwrap();
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
    let executable = directory.0.join("public");
    let clang = if cfg!(target_os = "macos") {
        std::path::PathBuf::from("/usr/bin/clang")
    } else {
        std::env::var_os("LLVM_SYS_211_PREFIX")
            .map(std::path::PathBuf::from)
            .map(|prefix| prefix.join("bin/clang"))
            .filter(|path| path.is_file())
            .unwrap_or_else(|| std::path::PathBuf::from("clang"))
    };
    let linked = std::process::Command::new(clang)
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    native_bounded::execute_binary(&executable, b"right\nright\nright\n");
}

#[test]
fn public_multiple_entries_execute_one_common_program() {
    let text = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {println(item)}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b)};remaining=remaining-1}}\nfun main():Unit {val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)}";
    let second = text.replace("run", "run_second").replace("main", "main_second")
        .replace("val choose=false", "val choose=true")
        .replace("fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {println(item)}\n", "");
    let text = format!("{text}\n{second}");
    let mut sources = SourceMap::new();
    let id = sources.add_source("public.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "public.ko", id, &parsed)];
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
    let other = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "main_second")
        .unwrap()
        .id();
    let units = lang_codegen::prepare_two_source_units(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &[other, entry],
    )
    .unwrap();
    for (selected, expected) in [
        (entry, b"right\nright\nright\n".as_slice()),
        (other, b"left\nleft\nleft\n".as_slice()),
    ] {
        let directory = Directory::new();
        let object = directory.0.join("public.o");
        lang_codegen::emit_native_two_source_units_object(&units, selected, &object).unwrap();
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
        let executable = directory.0.join("public");
        let clang = if cfg!(target_os = "macos") {
            std::path::PathBuf::from("/usr/bin/clang")
        } else {
            std::env::var_os("LLVM_SYS_211_PREFIX")
                .map(std::path::PathBuf::from)
                .map(|prefix| prefix.join("bin/clang"))
                .filter(|path| path.is_file())
                .unwrap_or_else(|| std::path::PathBuf::from("clang"))
        };
        let linked = std::process::Command::new(clang)
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        native_bounded::execute_binary(&executable, expected);
    }
}

#[test]
fn public_named_shared_owner_executes_exact_source() {
    let text = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {println(item)}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b)};remaining=remaining-1}}\nfun main():Unit {val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)}";
    for (rounds, choose) in [(0, false), (0, true), (3, false), (3, true)] {
        let text = text.replace("val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)",
        &format!("val text=\"same\";val rounds={rounds};val choose={choose};run(text,text,rounds,choose)"));
        let mut sources = SourceMap::new();
        let id = sources.add_source("public.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "public.ko", id, &parsed)];
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
        let directory = Directory::new();
        let object = directory.0.join("public.o");
        emit_native_two_source_unit_object(&unit, &object).unwrap();
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
        let executable = directory.0.join("public");
        let clang = if cfg!(target_os = "macos") {
            std::path::PathBuf::from("/usr/bin/clang")
        } else {
            std::env::var_os("LLVM_SYS_211_PREFIX")
                .map(std::path::PathBuf::from)
                .map(|prefix| prefix.join("bin/clang"))
                .filter(|path| path.is_file())
                .unwrap_or_else(|| std::path::PathBuf::from("clang"))
        };
        let linked = std::process::Command::new(clang)
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        native_bounded::execute_binary(&executable, "same\n".repeat(rounds).as_bytes());
    }
}
