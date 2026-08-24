use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::source::SourceMap;

use crate::ssa::model::{EntityType, Origin, Program, SsaTypeKind, TerminatorKind};

use super::{LlvmAdapterError, emit_verified_object};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-object-test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("test directory must be unique and creatable");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("owned test directory must be removable");
    }
}

fn unit_entry_program() -> (Program, crate::ssa::model::FunctionId) {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("object.ko", "fun app() {}")
        .expect("source");
    let origin = Origin::Source(sources.span(source, 0, 3).expect("span"));
    let mut program = Program::default();
    let module_id = program.add_module("object");
    let module = program.module_mut(module_id).expect("module");
    let entry = module
        .add_function("app", vec![], origin.clone())
        .expect("function");
    let block = module
        .function_mut(entry)
        .expect("function")
        .add_block(vec![], origin.clone())
        .expect("block");
    module
        .function_mut(entry)
        .expect("function")
        .set_terminator(block, TerminatorKind::Return { values: vec![] }, origin)
        .expect("return");
    (program, entry)
}

#[test]
fn target_machine_emits_arm64_mach_object_with_one_external_main() {
    let (program, entry) = unit_entry_program();
    let directory = TestDirectory::create();
    let object = directory.join("entry.o");
    emit_verified_object(&program, entry, &object).expect("object emission must succeed");

    let bytes = fs::read(&object).expect("object must be readable");
    assert_eq!(&bytes[..4], &[0xcf, 0xfa, 0xed, 0xfe]);
    assert_eq!(&bytes[4..8], &[0x0c, 0x00, 0x00, 0x01]);
    assert_eq!(&bytes[12..16], &[0x01, 0x00, 0x00, 0x00]);

    let output = Command::new("/usr/bin/nm")
        .args(["-g", "-U"])
        .arg(&object)
        .output()
        .expect("system nm must run");
    assert!(output.status.success());
    let symbols = String::from_utf8(output.stdout).expect("nm output must be UTF-8");
    assert_eq!(
        symbols
            .lines()
            .filter(|line| line.ends_with(" _main"))
            .count(),
        1
    );
    assert!(!symbols.contains("f0.app"));
}

#[test]
fn invalid_entry_and_unwritable_parent_fail_without_object() {
    let (mut program, _) = unit_entry_program();
    let module = program.modules.first_mut().expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let origin = module.functions[0].origin.clone();
    let invalid = module
        .add_function("invalid", vec![], origin.clone())
        .expect("function");
    let block = module
        .function_mut(invalid)
        .expect("function")
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("block");
    module
        .function_mut(invalid)
        .expect("function")
        .set_terminator(block, TerminatorKind::Abort, origin)
        .expect("abort");
    let directory = TestDirectory::create();
    let invalid_path = directory.join("invalid.o");
    assert!(matches!(
        emit_verified_object(&program, invalid, &invalid_path),
        Err(LlvmAdapterError::InvalidEntry(_))
    ));
    assert!(!invalid_path.exists());

    let missing_parent = directory.join("missing/entry.o");
    let (program, entry) = unit_entry_program();
    assert!(matches!(
        emit_verified_object(&program, entry, &missing_parent),
        Err(LlvmAdapterError::Object(_))
    ));
    assert!(!Path::new(&missing_parent).exists());
}
