//! CI-only export of MemoryDocument/rooted Unit Resource Map IR; no synthetic damaged IR.
use super::*;
use crate::ssa::map_provider_exits_pipeline_tests::{history_sanitizer_ir, history_sources};

#[test]
fn map_provider_resource_histories_export_real_ir() {
    let export = std::env::var_os("KOVEN_SANITIZER_PROVIDER_ARTIFACTS").map(PathBuf::from);
    let directory = export.clone().unwrap_or_else(|| {
        std::env::temp_dir().join(format!(
            "koven-map-provider-sanitizers-{}-{}",
            std::process::id(),
            NEXT_ARTIFACTS.fetch_add(1, Ordering::Relaxed)
        ))
    });
    fs::create_dir(&directory).expect("provider sanitizer export root must be new");
    let cases = history_sources()
        .into_iter()
        .filter(|case| case.label.ends_with("-resource"))
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 6);
    let mut manifest = String::new();
    for case in cases {
        for unit in [false, true] {
            // Keep the sanitizer artifact labels stable; both inputs now use Unit products.
            let entry = if unit { "unit" } else { "single" };
            let input = if unit {
                "rooted Unit"
            } else {
                "MemoryDocument Unit"
            };
            let label = format!("{}-{entry}", case.label);
            let path = directory.join(&label);
            fs::create_dir(&path).unwrap();
            let original = history_sanitizer_ir(&case, unit);
            fs::write(path.join("map.raw.ll"), &original).unwrap();
            fs::write(path.join("map-program.ko"), &case.source).unwrap();
            fs::write(path.join("map.expected.stdout"), &case.golden).unwrap();
            // Plain execution must satisfy the same independently specified full oracle first.
            let plain = path.join("map-plain");
            let linked = Command::new(crate::test_support::ir_clang())
                .arg(path.join("map.raw.ll"))
                .arg("-o")
                .arg(&plain)
                .output()
                .unwrap();
            fs::write(
                path.join("plain-link.command.txt"),
                format!(
                    "{:?} {:?} -o {:?}\n",
                    crate::test_support::ir_clang(),
                    path.join("map.raw.ll"),
                    plain
                ),
            )
            .unwrap();
            save_plain_output(&path, "plain-link", &linked);
            assert!(linked.status.success(), "{label}: {linked:?}");
            let run = Command::new(&plain).output().unwrap();
            fs::write(path.join("plain-run.command.txt"), format!("{plain:?}\n")).unwrap();
            save_plain_output(&path, "plain-run", &run);
            assert_eq!(run.status.code(), Some(0), "{label}: {run:?}");
            assert_eq!(run.stdout, case.golden.as_bytes(), "{label}: {run:?}");
            assert!(run.stderr.is_empty(), "{label}: {run:?}");
            let context = Context::create();
            let buffer =
                MemoryBuffer::create_from_memory_range_copy(original.as_bytes(), "provider");
            let module = context.create_module_from_ir(buffer).unwrap();
            let functions = mark_address_sanitizer(&context, &module);
            module
                .verify()
                .expect("ASan attributes preserve original valid LLVM");
            fs::write(
                path.join("map.asan.ll"),
                module.print_to_string().to_bytes(),
            )
            .unwrap();
            fs::write(path.join("map.functions"), functions.join("\n")).unwrap();
            manifest.push_str(&label);
            manifest.push('\n');
            println!(
                "SPEC0331 CI PROVIDER EXPORT case={label} input={input} source=original SSA=verified LLVM=verified plain=golden"
            );
        }
    }
    assert_eq!(manifest.lines().count(), 12);
    fs::write(directory.join("manifest.txt"), manifest).unwrap();
    if export.is_none() {
        fs::remove_dir_all(&directory).unwrap();
    }
}

fn save_plain_output(path: &std::path::Path, label: &str, output: &std::process::Output) {
    fs::write(path.join(format!("{label}.stdout")), &output.stdout).unwrap();
    fs::write(path.join(format!("{label}.stderr")), &output.stderr).unwrap();
    fs::write(
        path.join(format!("{label}.status")),
        format!("{:?}\n", output.status),
    )
    .unwrap();
}
