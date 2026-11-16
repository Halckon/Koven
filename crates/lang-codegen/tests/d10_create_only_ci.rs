//! CI-only concurrent public publishers. No synthetic SSA or production synchronization hook.
#![cfg(feature = "d10-catalog-experiment")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::d10_catalog_experiment as experiment,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
use std::{fs, path::PathBuf, process::Command, sync::mpsc, thread, time::Duration};
const WAIT: Duration = Duration::from_secs(30);
fn publisher(
    output: PathBuf,
    tag: usize,
    ready: mpsc::Sender<(usize, Vec<u8>)>,
    go: mpsc::Receiver<()>,
) -> Result<(), lang_codegen::NativeObjectErrorKind> {
    let text = format!(
        r#"fun run(count: Int, childCount: Int) {{
var a = "first{tag}"; var b = "kept"; var outer = count;
while(outer > 0) {{ val parentOld = replace(&a, "parent"); var inner = childCount;
while(inner > 0) {{ val old = replace(&a, "next"); inner -= 1 }}; outer -= 1 }};
println(a); println(b) }}
fun entry() {{ val p = 1; val c = 2; run(p, c) }}"#
    );
    let mut sources = SourceMap::new();
    let id = sources.add_source("competition.ko", &text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let inputs = [SourceUnitInput::new("root", "competition.ko", id, &parsed)];
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
    let prepared =
        experiment::prepare_named_entry(&sources, &inputs, &names, &types, typed).unwrap();
    let artifact = lang_codegen::d10_experiment::lower_prepared_named_entry(
        &sources, &inputs, &names, &types, &prepared,
    )
    .unwrap();
    let reference = output.with_file_name(format!("reference-{tag}.o"));
    artifact.emit_new_process_object(&reference).unwrap();
    ready.send((tag, fs::read(reference).unwrap())).unwrap();
    go.recv_timeout(WAIT).expect("bounded release wait");
    artifact
        .emit_new_process_object(&output)
        .map_err(|e| e.kind())
}
#[test]
#[ignore = "CI-only concurrent publication control; never run locally"]
fn ci_only_concurrent_create_only_publishers_preserve_one_winner() {
    assert_eq!(
        std::env::var("GITHUB_ACTIONS").as_deref(),
        Ok("true"),
        "CI-only competition"
    );
    let directory =
        std::env::temp_dir().join(format!("koven-d10-competition-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let output = directory.join("entry.o");
    let (ready_tx, ready_rx) = mpsc::channel();
    let mut workers = Vec::new();
    let mut releases = Vec::new();
    for tag in 0..2 {
        let (go_tx, go_rx) = mpsc::channel();
        releases.push(go_tx);
        let path = output.clone();
        let ready = ready_tx.clone();
        workers.push(thread::spawn(move || publisher(path, tag, ready, go_rx)));
    }
    drop(ready_tx);
    let first = ready_rx
        .recv_timeout(WAIT)
        .expect("first independently validated publisher");
    let second = ready_rx
        .recv_timeout(WAIT)
        .expect("second independently validated publisher");
    assert_ne!(first.0, second.0);
    assert_ne!(
        first.1, second.1,
        "independent artifacts must be observably distinct"
    );
    let references = [first, second];
    for release in releases {
        release.send(()).unwrap();
    }
    let results = workers
        .into_iter()
        .map(|w| w.join().expect("publisher thread"))
        .collect::<Vec<_>>();
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "exactly one public publisher must commit"
    );
    assert_eq!(results.iter().filter(|r| r.is_err()).count(), 1);
    assert!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|kind| *kind == lang_codegen::NativeObjectErrorKind::OutputAlreadyExists)
    );
    let winner = results.iter().position(Result::is_ok).unwrap();
    let expected = &references.iter().find(|(tag, _)| *tag == winner).unwrap().1;
    assert!(
        fs::read(&output).unwrap() == *expected,
        "target must equal winning independent artifact"
    );
    let nm = Command::new("/usr/bin/nm")
        .args(if cfg!(target_os = "macos") {
            ["-j", "-U"]
        } else {
            ["-j", "--defined-only"]
        })
        .arg(&output)
        .output()
        .unwrap();
    assert!(nm.status.success());
    assert_eq!(
        String::from_utf8(nm.stdout)
            .unwrap()
            .lines()
            .filter(|s| *s
                == if cfg!(target_os = "macos") {
                    "_main"
                } else {
                    "main"
                })
            .count(),
        1
    );
    assert_eq!(
        fs::read_dir(&directory).unwrap().count(),
        3,
        "only target and two independent references remain"
    );
    fs::remove_dir_all(directory).unwrap();
}
