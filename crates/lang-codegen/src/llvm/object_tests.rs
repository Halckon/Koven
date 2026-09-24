use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    ComparisonOperator, Edge, EntityId, EntityType, LoanKind, Operation, Origin, Program,
    ScalarConstant, SequentialContainerKind, SsaTypeKind, TerminatorKind,
};

use super::{LlvmAdapterError, emit_verified_object, render_verified_program_with_entry};

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

fn unit_entry_program() -> (SourceMap, Program, crate::ssa::model::FunctionId) {
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
    (sources, program, entry)
}

fn abort_entry_program() -> (SourceMap, Program, crate::ssa::model::FunctionId) {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("abort.ko", "fun app(): Nothing")
        .expect("source");
    let origin = Origin::Source(sources.span(source, 0, 3).expect("span"));
    let mut program = Program::default();
    let module_id = program.add_module("abort");
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
        .set_terminator(block, TerminatorKind::Abort, origin)
        .expect("abort");
    (sources, program, entry)
}

fn link(object: &Path, executable: &Path) {
    let output = Command::new("/usr/bin/clang")
        .arg(object)
        .arg("-o")
        .arg(executable)
        .output()
        .expect("system clang must launch");
    assert!(
        output.status.success(),
        "link failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn target_machine_emits_arm64_mach_object_with_one_external_main() {
    let (sources, program, entry) = unit_entry_program();
    let directory = TestDirectory::create();
    let object = directory.join("entry.o");
    emit_verified_object(&program, &sources, entry, &object).expect("object emission must succeed");

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
fn borrowed_container_length_builds_links_and_runs_with_int_result() {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("borrowed-length.ko", "fun app() {}")
        .expect("source");
    let origin = Origin::Source(sources.span(source, 0, 3).expect("span"));
    let mut program = Program::default();
    let module_id = program.add_module("borrowed-length");
    let module = program.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let array = module
        .add_sequential_container_type(SequentialContainerKind::Array, integer)
        .expect("array type");
    let entry = module
        .add_function("app", vec![], origin.clone())
        .expect("function");
    let function = module.function_mut(entry).expect("function");
    let block = function
        .add_block(vec![], origin.clone())
        .expect("entry block");
    let success = function
        .add_block(vec![], origin.clone())
        .expect("success block");
    let failure = function
        .add_block(vec![], origin.clone())
        .expect("failure block");
    let mut constant = |number| {
        let (_, results) = function
            .append_instruction(
                block,
                Operation::Constant(ScalarConstant::Integer(number)),
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .expect("constant");
        let [EntityId::Value(value)] = results.as_slice() else {
            panic!("constant value")
        };
        *value
    };
    let first = constant(1);
    let second = constant(2);
    let expected = constant(2);
    let (_, results) = function
        .append_instruction(
            block,
            Operation::ContainerConstruct {
                container: array,
                elements: vec![first, second],
            },
            vec![EntityType::Value(array)],
            origin.clone(),
        )
        .expect("container");
    let [EntityId::Value(owner)] = results.as_slice() else {
        panic!("container owner")
    };
    let owner = *owner;
    let (_, results) = function
        .append_instruction(
            block,
            Operation::RootPlace { owner },
            vec![EntityType::Place(array)],
            origin.clone(),
        )
        .expect("root place");
    let [EntityId::Place(place)] = results.as_slice() else {
        panic!("root place result")
    };
    let (_, results) = function
        .append_instruction(
            block,
            Operation::BorrowBegin {
                place: *place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: array,
            }],
            origin.clone(),
        )
        .expect("shared loan");
    let [EntityId::Loan(loan)] = results.as_slice() else {
        panic!("shared loan result")
    };
    let loan = *loan;
    let (_, results) = function
        .append_instruction(
            block,
            Operation::ContainerLength {
                owner: EntityId::Loan(loan),
            },
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("borrowed length");
    let [EntityId::Value(length)] = results.as_slice() else {
        panic!("length result")
    };
    let length = *length;
    function
        .append_instruction(block, Operation::BorrowEnd { loan }, vec![], origin.clone())
        .expect("loan end");
    function
        .append_instruction(block, Operation::Drop { owner }, vec![], origin.clone())
        .expect("drop");
    let (_, results) = function
        .append_instruction(
            block,
            Operation::Compare {
                operator: ComparisonOperator::Equal,
                left: length,
                right: expected,
            },
            vec![EntityType::Value(boolean)],
            origin.clone(),
        )
        .expect("compare length");
    let [EntityId::Value(matches)] = results.as_slice() else {
        panic!("compare result")
    };
    function
        .set_terminator(
            block,
            TerminatorKind::Conditional {
                condition: *matches,
                when_true: Edge {
                    target: success,
                    arguments: vec![],
                },
                when_false: Edge {
                    target: failure,
                    arguments: vec![],
                },
            },
            origin.clone(),
        )
        .expect("length branch");
    function
        .append_instruction(
            success,
            Operation::PrintLiteral {
                bytes: b"done\n".to_vec(),
            },
            vec![],
            origin.clone(),
        )
        .expect("print");
    function
        .set_terminator(
            success,
            TerminatorKind::Return { values: vec![] },
            origin.clone(),
        )
        .expect("return");
    function
        .set_terminator(failure, TerminatorKind::Abort, origin)
        .expect("abort");

    let directory = TestDirectory::create();
    let object = directory.join("borrowed-length.o");
    let executable = directory.join("borrowed-length");
    emit_verified_object(&program, &sources, entry, &object).expect("borrowed length object");
    link(&object, &executable);
    let run = Command::new(&executable)
        .output()
        .expect("executable must run");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"done\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn invalid_entry_and_unwritable_parent_fail_without_object() {
    let (sources, mut program, _) = unit_entry_program();
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
        emit_verified_object(&program, &sources, invalid, &invalid_path),
        Err(LlvmAdapterError::InvalidEntry(_))
    ));
    assert!(!invalid_path.exists());

    let missing_parent = directory.join("missing/entry.o");
    let (sources, program, entry) = unit_entry_program();
    assert!(matches!(
        emit_verified_object(&program, &sources, entry, &missing_parent),
        Err(LlvmAdapterError::Object(_))
    ));
    assert!(!Path::new(&missing_parent).exists());
}

#[test]
fn emitted_koven_objects_link_and_run_normal_and_abort_entries() {
    let directory = TestDirectory::create();
    let (normal_sources, normal_program, normal_entry) = unit_entry_program();
    let normal_object = directory.join("normal.o");
    let normal_executable = directory.join("normal");
    emit_verified_object(
        &normal_program,
        &normal_sources,
        normal_entry,
        &normal_object,
    )
    .expect("normal object");
    link(&normal_object, &normal_executable);
    assert_eq!(
        Command::new(&normal_executable)
            .output()
            .expect("normal run")
            .status
            .code(),
        Some(0)
    );

    let (abort_sources, abort_program, abort_entry) = abort_entry_program();
    let abort_llvm =
        render_verified_program_with_entry(&abort_program, abort_entry).expect("abort IR");
    assert!(abort_llvm.contains("call void @abort()"));
    assert!(abort_llvm.contains("unreachable"));
    assert!(!abort_llvm.contains("landingpad"));
    assert!(!abort_llvm.contains("personality"));
    let abort_object = directory.join("abort.o");
    let abort_executable = directory.join("abort");
    emit_verified_object(&abort_program, &abort_sources, abort_entry, &abort_object)
        .expect("abort object");
    link(&abort_object, &abort_executable);
    assert!(
        !Command::new(&abort_executable)
            .output()
            .expect("abort run")
            .status
            .success()
    );
}
