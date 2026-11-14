//! Test-only ABI trampoline. Original replayed SSA and LLVM function bodies are unchanged.
//! This is not a source entry contract, allocator witness, or production entry mode.
use crate::ssa::model::*;
use std::{
    fs,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn independent_nested_counters_reach_native_final_root_values() {
    crate::ssa::d10_dual_nested_loop_tests::native_postuse_program(check_program);
}

#[test]
fn independent_nested_counter_trampoline_compiles_to_real_objects() {
    crate::ssa::d10_dual_nested_loop_tests::native_postuse_program(check_compiled_program);
}

fn check_compiled_program(program: &Program, entry: FunctionId) {
    check_artifact(program, entry, false);
}
fn check_program(program: &Program, entry: FunctionId) {
    check_artifact(program, entry, true);
}
fn check_artifact(program: &Program, entry: FunctionId, execute: bool) {
    crate::ssa::verify::verify_program(program).unwrap();
    let [module] = program.modules.as_slice() else {
        panic!("single real module")
    };
    let [function] = module.functions.as_slice() else {
        panic!("single real function")
    };
    assert_eq!(function.id(), entry);
    assert!(
        function
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_'))
    );
    assert!(function.return_types.is_empty());
    let block = function.block(function.entry_block().unwrap()).unwrap();
    let [EntityId::Loan(parent), EntityId::Loan(child)] = block.parameters.as_slice() else {
        panic!("two actual Shared Int parameters")
    };
    assert_ne!(parent, child);
    for loan in [parent, child] {
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = function.entity(EntityId::Loan(*loan)).unwrap().ty
        else {
            panic!("Shared parameter ABI")
        };
        assert_eq!(
            module.types[target.index()],
            SsaTypeKind::Integer {
                bits: 32,
                signed: true
            }
        );
    }
    let original = crate::llvm::render_verified_program(program).unwrap();
    let symbol = format!("f{}.{}", entry.index(), function.name);
    assert!(!original.contains("define i32 @main("));
    // Append a harness only. No symbol replacement, linkage rewrite, SSA edits or receipts.
    let mut llvm = original.clone();
    llvm.push_str("\ndefine i32 @main() {\nentry:\n  %parent = alloca i32, align 4\n  %child = alloca i32, align 4\n");
    let mut expected = String::new();
    for (parent, child, value) in [
        (0, 0, "first"),
        (0, 2, "first"),
        (1, 0, "parent"),
        (2, 0, "parent"),
        (1, 2, "next"),
        (2, 2, "next"),
    ] {
        llvm.push_str(&format!("  store i32 {parent}, ptr %parent, align 4\n  store i32 {child}, ptr %child, align 4\n  call void @\"{symbol}\"(ptr %parent, ptr %child)\n"));
        expected.push_str(value);
        expected.push_str("\nkept\n");
    }
    llvm.push_str("  ret i32 0\n}\n");
    assert!(llvm.starts_with(&original));
    let directory = std::env::temp_dir().join(format!(
        "koven-d10-counter-native-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let ir = directory.join("program.ll");
    let object = directory.join("program.o");
    let executable = directory.join("program");
    fs::write(&ir, llvm).unwrap();
    fs::write(directory.join("expected.stdout"), &expected).unwrap();
    for (label, command) in [
        (
            "compile",
            Command::new(crate::test_support::ir_clang())
                .arg("-O0")
                .arg("-c")
                .arg(&ir)
                .arg("-o")
                .arg(&object),
        ),
        (
            "link",
            Command::new(crate::test_support::ir_clang())
                .arg(&object)
                .arg("-o")
                .arg(&executable),
        ),
    ] {
        fs::write(
            directory.join(format!("{label}.command")),
            format!("{command:?}\n"),
        )
        .unwrap();
        let output = command.output().expect("Clang must launch");
        fs::write(directory.join(format!("{label}.stdout")), &output.stdout).unwrap();
        fs::write(directory.join(format!("{label}.stderr")), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{label}: {output:?}; artifacts={}",
            directory.display()
        );
    }
    assert!(fs::metadata(&object).unwrap().len() > 0);
    if execute {
        execute_binary(&executable, expected.as_bytes(), &directory);
    }
    fs::remove_dir_all(&directory).unwrap();
}

// Same 10-second/10-ms bounded execution contract as the existing E2 test helper.
fn execute_binary(executable: &std::path::Path, expected: &[u8], directory: &std::path::Path) {
    let mut child = Command::new(executable)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("native D10 must launch");
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            fs::write(directory.join("run.stdout"), &output.stdout).unwrap();
            fs::write(directory.join("run.stderr"), &output.stderr).unwrap();
            panic!(
                "bounded D10 execution exceeded original 10s deadline: {output:?}; artifacts={}",
                directory.display()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    fs::write(directory.join("run.stdout"), &output.stdout).unwrap();
    fs::write(directory.join("run.stderr"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{output:?}; artifacts={}",
        directory.display()
    );
    assert_eq!(output.stdout, expected, "artifacts={}", directory.display());
    assert!(
        output.stderr.is_empty(),
        "{output:?}; artifacts={}",
        directory.display()
    );
}

#[path = "native_d10_named_entry_tests.rs"]
mod named_entry;

#[path = "native_d10_named_after_entry_tests.rs"]
mod named_after_entry;
