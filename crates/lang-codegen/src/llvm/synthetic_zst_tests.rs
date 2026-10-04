//! Internal test SSA only: no source-level MoveOnly ZST capability.
use super::object_tests::{TestDirectory, link};
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
                    .append_instruction(block, Operation::Drop { owner }, vec![], origin.clone())
                    .unwrap();
                function
                    .set_terminator(block, TerminatorKind::Return { values: vec![] }, origin)
                    .unwrap();
                verify_program(&program).unwrap();
                let directory = TestDirectory::create();
                let object = directory.join("real.o");
                super::emit_verified_object(&program, &sources, entry, &object).unwrap();
                let executable = directory.join("real");
                link(&object, &executable);
                assert!(Command::new(&executable).output().unwrap().status.success());
                let ir = super::render_verified_program_with_entry(&program, entry).unwrap();
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
