use lang_frontend::source::SourceMap;

use crate::ssa::model::{EntityType, Origin, Program, SsaTypeKind, TerminatorKind};

use super::{LlvmAdapterError, render_verified_program, render_verified_program_with_entry};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("entry.ko", "fun app() {}")
        .expect("source");
    Origin::Source(sources.span(source, 0, 3).expect("span"))
}

#[test]
fn explicit_unit_entry_generates_one_c_main_wrapper() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("entry");
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

    let ordinary = render_verified_program(&program).expect("ordinary module");
    assert!(!ordinary.contains("@main"));
    let llvm = render_verified_program_with_entry(&program, entry).expect("native entry module");
    assert_eq!(llvm.matches("define i32 @main()").count(), 1);
    assert!(llvm.contains("define internal void @f0.app()"));
    assert!(llvm.contains("call void @f0.app()"));
    assert!(llvm.contains("ret i32 0"));
}

#[test]
fn native_entry_rejects_parameters_and_non_unit_return_before_wrapper() {
    let origin = origin();
    let mut parameterized = Program::default();
    let module_id = parameterized.add_module("parameterized");
    let module = parameterized.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let entry = module
        .add_function("app", vec![], origin.clone())
        .expect("function");
    let block = module
        .function_mut(entry)
        .expect("function")
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("block");
    module
        .function_mut(entry)
        .expect("function")
        .set_terminator(block, TerminatorKind::Abort, origin.clone())
        .expect("abort");
    assert!(matches!(
        render_verified_program_with_entry(&parameterized, entry),
        Err(LlvmAdapterError::InvalidEntry(message)) if message.contains("parameters")
    ));

    let mut returning = Program::default();
    let module_id = returning.add_module("returning");
    let module = returning.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let entry = module
        .add_function("app", vec![integer], origin.clone())
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
    assert!(matches!(
        render_verified_program_with_entry(&returning, entry),
        Err(LlvmAdapterError::InvalidEntry(message)) if message.contains("Unit")
    ));
}
