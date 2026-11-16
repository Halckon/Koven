use crate::test_support::diagnostics::assert_diagnostic;
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

pub(super) struct TestDirectory(PathBuf);

impl TestDirectory {
    pub(super) fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-object-test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("test directory must be unique and creatable");
        Self(path)
    }

    pub(super) fn join(&self, name: &str) -> PathBuf {
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

pub(super) fn link(object: &Path, executable: &Path) {
    let output = Command::new(crate::test_support::clang())
        .arg(object)
        .arg("-o")
        .arg(executable)
        .output()
        .expect("system clang must launch");
    assert_diagnostic!(
        output.status.success(),
        "link failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn target_machine_emits_native_object_with_one_external_main() {
    let (sources, program, entry) = unit_entry_program();
    let directory = TestDirectory::create();
    let object = directory.join("entry.o");
    emit_verified_object(&program, &sources, entry, &object).expect("object emission must succeed");

    let bytes = fs::read(&object).expect("object must be readable");
    crate::test_support::assert_native_object(&bytes);

    let output = Command::new("/usr/bin/nm")
        .args(if cfg!(target_os = "macos") {
            ["-g", "-U"]
        } else {
            ["-g", "--defined-only"]
        })
        .arg(&object)
        .output()
        .expect("system nm must run");
    assert!(output.status.success());
    let symbols = String::from_utf8(output.stdout).expect("nm output must be UTF-8");
    assert_eq!(
        symbols
            .lines()
            .filter(
                |line| line.split_whitespace().last() == Some(crate::test_support::main_symbol())
            )
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
    assert!(abort_llvm.contains("call void @koven.abort.report"));
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

pub(crate) mod borrowed_generate_tests {
    use super::super::render_verified_program;
    use crate::ssa::borrowed_generate_tests::{Layout, dynamic_fixture, fixture};
    use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};

    #[test]
    fn borrowed_generation_prepares_fixed_storage_once_outside_the_loop() {
        for layout in [Layout::Pointer, Layout::Shared, Layout::Owned] {
            for size in [0, 1, 3] {
                let fixture = fixture(layout, size, false);
                let llvm = render_verified_program(&fixture.program)
                    .expect("borrowed generator LLVM must verify");
                let make = llvm
                    .split("@f1.make()")
                    .nth(1)
                    .expect("make")
                    .split("\n}")
                    .next()
                    .expect("body");
                let body = make
                    .split(".body:")
                    .nth(1)
                    .expect("loop body")
                    .split(".done:")
                    .next()
                    .expect("done");
                assert_diagnostic!(
                    !body.contains("alloca"),
                    "loop storage must remain bounded: {llvm}"
                );
                assert_eq_diagnostic!(
                    make.matches(".index.storage = alloca i32").count(),
                    1,
                    "{llvm}"
                );
                assert_eq_diagnostic!(
                    make.matches(".environment.storage = alloca").count(),
                    usize::from(!matches!(layout, Layout::Pointer)),
                    "{llvm}"
                );
                assert_diagnostic!(
                    body.contains("call i32"),
                    "callback stays in guarded loop: {llvm}"
                );
                assert!(make.contains("icmp ult i64"));

                assert!(make.contains("llvm.umul.with.overflow"));
            }
        }
    }

    #[test]
    fn borrowed_unit_void_generation_calls_each_logical_index_without_buffer_addressing() {
        let fixture = fixture(Layout::Pointer, 3, true);
        let llvm = render_verified_program(&fixture.program).expect("Unit LLVM must verify");
        let make = llvm
            .split("@f1.make()")
            .nth(1)
            .expect("make")
            .split("\n}")
            .next()
            .expect("body");
        assert_diagnostic!(
            make.contains("call void %"),
            "logical Unit callback must be invoked: {llvm}"
        );
        assert!(!make.contains("getelementptr"));
        assert!(!make.contains("call ptr @malloc"));
    }

    #[test]
    fn borrowed_generation_dynamic_length_keeps_checked_length_and_bytes_guards() {
        let fixture = dynamic_fixture(Layout::Pointer);
        let llvm =
            render_verified_program(&fixture.program).expect("dynamic borrowed generator LLVM");
        let make = llvm
            .split("@f1.make(")
            .nth(1)
            .expect("make")
            .split("\n}")
            .next()
            .expect("body");
        assert_diagnostic!(
            make.contains("icmp slt i32"),
            "negative length rejected before conversion: {llvm}"
        );
        assert_diagnostic!(
            make.contains("zext i32"),
            "nonnegative logical length becomes size_t: {llvm}"
        );
        assert_diagnostic!(
            make.contains("llvm.umul.with.overflow"),
            "physical bytes remain checked: {llvm}"
        );
        let preheader = make.split(".loop:").next().expect("preheader");
        assert_diagnostic!(
            !preheader.contains("call i32 %"),
            "no callback before allocation succeeds: {llvm}"
        );
    }
}

pub(crate) mod nullable_tests {
    use crate::test_support::diagnostics::assert_diagnostic;
    use lang_frontend::source::SourceMap;

    use crate::ssa::model::{
        Edge, EntityId, EntityType, LoanKind, Operation, Origin, Program, TerminatorKind, ValueId,
    };

    use super::super::render_verified_program;

    fn value(entity: EntityId) -> ValueId {
        let EntityId::Value(value) = entity else {
            panic!("expected value");
        };
        value
    }

    #[test]
    fn nullable_handle_uses_one_pointer_non_null_phi_and_conditional_drop() {
        let mut sources = SourceMap::default();
        let source = sources
            .add_source("nullable-llvm.ko", "if (node != null) node")
            .expect("source");
        let origin = Origin::Source(sources.span(source, 0, 2).expect("span"));
        let mut program = Program::default();
        let module_id = program.add_module("nullable_llvm");
        let module = program.module_mut(module_id).expect("module");
        let payload = module
            .add_aggregate_type("Node.payload", Vec::new())
            .expect("payload");
        let inner = module.declare_heap_owner("Node").expect("owner");
        module
            .define_heap_owner(inner, payload)
            .expect("definition");
        let nullable = module.add_nullable_handle_type(inner).expect("nullable");

        let function_id = module
            .add_function("inspect", Vec::new(), origin.clone())
            .expect("function");
        let function = module.function_mut(function_id).expect("function");
        let entry = function
            .add_block(vec![EntityType::Value(nullable)], origin.clone())
            .expect("entry");
        let owner = value(function.block(entry).expect("block").parameters[0]);
        let null_block = function
            .add_block(vec![EntityType::Value(nullable)], origin.clone())
            .expect("null block");
        let non_null_block = function
            .add_block(
                vec![
                    EntityType::Value(nullable),
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: inner,
                    },
                ],
                origin.clone(),
            )
            .expect("non-null block");
        let null_owner = value(function.block(null_block).expect("block").parameters[0]);
        let parameters = function
            .block(non_null_block)
            .expect("block")
            .parameters
            .clone();
        let non_null_owner = value(parameters[0]);
        let EntityId::Loan(view) = parameters[1] else {
            panic!("expected non-null view");
        };
        function
            .set_terminator(
                entry,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null: Edge {
                        target: null_block,
                        arguments: vec![EntityId::Value(owner)],
                    },
                    when_non_null: Edge {
                        target: non_null_block,
                        arguments: vec![EntityId::Value(owner)],
                    },
                    view,
                },
                origin.clone(),
            )
            .expect("branch");
        function
            .append_instruction(
                null_block,
                Operation::Drop { owner: null_owner },
                Vec::new(),
                origin.clone(),
            )
            .expect("nullable drop");
        function
            .set_terminator(
                null_block,
                TerminatorKind::Return { values: Vec::new() },
                origin.clone(),
            )
            .expect("return");
        let taken = value(
            function
                .append_instruction(
                    non_null_block,
                    Operation::NullableTake {
                        owner: non_null_owner,
                        proof: view,
                    },
                    vec![EntityType::Value(inner)],
                    origin.clone(),
                )
                .expect("take")
                .1[0],
        );
        function
            .append_instruction(
                non_null_block,
                Operation::Drop { owner: taken },
                Vec::new(),
                origin.clone(),
            )
            .expect("inner drop");
        function
            .set_terminator(
                non_null_block,
                TerminatorKind::Return { values: Vec::new() },
                origin,
            )
            .expect("return");

        let ir = render_verified_program(&program).expect("nullable LLVM lowering");
        assert_diagnostic!(ir.contains("phi ptr"), "{ir}");
        assert_diagnostic!(ir.contains("icmp eq ptr"), "{ir}");
        assert_diagnostic!(
            ir.contains("define internal void @koven.drop.t2(ptr"),
            "{ir}"
        );
        assert_diagnostic!(ir.contains("call void @koven.drop.t1(ptr"), "{ir}");
        assert_diagnostic!(
            !ir.contains("koven.enum"),
            "nullable pointer must not gain a tag: {ir}"
        );
        assert_diagnostic!(
            !ir.contains("malloc"),
            "nullable wrapping must not allocate: {ir}"
        );
    }
}

pub(crate) mod synthetic_zst_tests {
    //! Internal test SSA only: no source-level MoveOnly ZST capability.
    use super::super::object_tests::{TestDirectory, link};
    use crate::ssa::{
        model::{
            EntityId, EntityType, Operation, Origin, Ownership, Program, ScalarConstant,
            SequentialContainerKind, SsaTypeKind, TerminatorKind,
        },
        verify::verify_program,
    };
    use lang_frontend::source::SourceMap;
    use std::{fs, process::Command};

    fn instrument_entry(ir: &str, name: &str, hook: &str) -> String {
        let definition = ir
            .lines()
            .find(|line| line.starts_with("define ") && line.contains(&format!("@{name}(")))
            .expect("real drop definition");
        let start = ir.find(definition).unwrap();
        let entry = start + ir[start..].find("entry:\n").unwrap() + "entry:\n".len();
        let mut result = ir.to_owned();
        result.insert_str(entry, &format!("  call void @{hook}()\n"));
        result
    }

    #[test]
    fn synthetic_zst_container_drop_counts_logical_elements_and_storage_separately() {
        for kind in [
            SequentialContainerKind::Array,
            SequentialContainerKind::List,
            SequentialContainerKind::MutableList,
        ] {
            for length in [0, 1, 3] {
                for nonzero_stride in [false, true] {
                    let mut sources = SourceMap::default();
                    let source = sources
                        .add_source("synthetic-zst.ko", "internal test fixture")
                        .unwrap();
                    let origin = Origin::Source(sources.span(source, 0, 8).unwrap());
                    let mut program = Program::default();
                    let module_id = program.add_module("synthetic-zst");
                    let module = program.module_mut(module_id).unwrap();
                    let zero = module.intern_type(SsaTypeKind::ZeroSized {
                        name: "SyntheticMoveOnlyZst".into(),
                        ownership: Ownership::MoveOnly,
                    });
                    let int = module.intern_type(SsaTypeKind::Integer {
                        bits: 32,
                        signed: true,
                    });
                    let element = if nonzero_stride {
                        module
                            .add_aggregate_type("NonzeroWrapper", vec![zero, int])
                            .unwrap()
                    } else {
                        zero
                    };
                    let container = module.add_sequential_container_type(kind, element).unwrap();
                    let entry = module
                        .add_function("entry", vec![], origin.clone())
                        .unwrap();
                    let function = module.function_mut(entry).unwrap();
                    let block = function.add_block(vec![], origin.clone()).unwrap();
                    let mut elements = Vec::new();
                    for _ in 0..length {
                        let (_, result) = function
                            .append_instruction(
                                block,
                                Operation::Constant(ScalarConstant::SyntheticZero),
                                vec![EntityType::Value(zero)],
                                origin.clone(),
                            )
                            .unwrap();
                        let EntityId::Value(zst) = result[0] else {
                            panic!("zero value")
                        };
                        let value = if nonzero_stride {
                            let (_, result) = function
                                .append_instruction(
                                    block,
                                    Operation::Constant(ScalarConstant::Integer(7)),
                                    vec![EntityType::Value(int)],
                                    origin.clone(),
                                )
                                .unwrap();
                            let EntityId::Value(number) = result[0] else {
                                panic!("integer")
                            };
                            let (_, result) = function
                                .append_instruction(
                                    block,
                                    Operation::AggregateConstruct {
                                        aggregate: element,
                                        fields: vec![zst, number],
                                    },
                                    vec![EntityType::Value(element)],
                                    origin.clone(),
                                )
                                .unwrap();
                            let EntityId::Value(value) = result[0] else {
                                panic!("element")
                            };
                            value
                        } else {
                            zst
                        };
                        elements.push(value);
                    }
                    let (_, result) = function
                        .append_instruction(
                            block,
                            Operation::ContainerConstruct {
                                container,
                                elements,
                            },
                            vec![EntityType::Value(container)],
                            origin.clone(),
                        )
                        .unwrap();
                    let EntityId::Value(owner) = result[0] else {
                        panic!("container")
                    };
                    function
                        .append_instruction(
                            block,
                            Operation::Drop { owner },
                            vec![],
                            origin.clone(),
                        )
                        .unwrap();
                    function
                        .set_terminator(block, TerminatorKind::Return { values: vec![] }, origin)
                        .unwrap();
                    verify_program(&program).unwrap();
                    let directory = TestDirectory::create();
                    let object = directory.join("real.o");
                    super::super::emit_verified_object(&program, &sources, entry, &object).unwrap();
                    let executable = directory.join("real");
                    link(&object, &executable);
                    assert!(Command::new(&executable).output().unwrap().status.success());
                    let ir =
                        super::super::render_verified_program_with_entry(&program, entry).unwrap();
                    let ir = instrument_entry(
                        &ir,
                        &format!("koven.drop.t{}", zero.index()),
                        "count_element",
                    );
                    let ir = instrument_entry(
                        &ir,
                        &format!("koven.drop.t{}", container.index()),
                        "count_container",
                    );
                    let ir = ir
                        .replace("@malloc(", "@counted_malloc(")
                        .replace("@free(", "@counted_free(");
                    let ir = format!(
                        "{ir}\ndeclare void @count_element()\ndeclare void @count_container()\n"
                    );
                    let llvm = directory.join("counted.ll");
                    fs::write(&llvm, ir).unwrap();
                    let counter = directory.join("counter.c");
                    let allocations = usize::from(nonzero_stride && length > 0);
                    fs::write(&counter, format!(r#"
#include <stdlib.h>
#include <assert.h>
static int elements, containers, allocations, frees;
static void *storage;
void count_element(void) {{ ++elements; }}
void count_container(void) {{ ++containers; }}
void *counted_malloc(size_t size) {{ assert(!storage); ++allocations; return storage = malloc(size); }}
void counted_free(void *pointer) {{ assert(pointer && pointer == storage); storage = 0; ++frees; free(pointer); }}
__attribute__((destructor)) static void verify(void) {{ assert(elements == {length}); assert(containers == 1); assert(allocations == {allocations}); assert(frees == {allocations}); assert(!storage); }}
"#)).unwrap();
                    let counted_object = directory.join("counted.o");
                    let compiled = Command::new(crate::test_support::ir_clang())
                        .arg("-c")
                        .arg(&llvm)
                        .arg("-o")
                        .arg(&counted_object)
                        .output()
                        .unwrap();
                    assert!(compiled.status.success(), "{compiled:?}");
                    let counted = directory.join("counted");
                    let linked = Command::new(crate::test_support::clang())
                        .arg(&counted_object)
                        .arg(&counter)
                        .arg("-o")
                        .arg(&counted)
                        .output()
                        .unwrap();
                    assert!(linked.status.success(), "{linked:?}");
                    let run = Command::new(counted).output().unwrap();
                    assert!(
                        run.status.success(),
                        "kind={kind:?} length={length} nonzero={nonzero_stride}: {run:?}"
                    );
                    assert!(run.stdout.is_empty() && run.stderr.is_empty());
                }
            }
        }
    }
}
