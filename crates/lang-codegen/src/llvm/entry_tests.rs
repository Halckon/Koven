use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    EntityType, LoanKind, Origin, Program, SequentialContainerKind, SsaTypeKind, TerminatorKind,
};

use super::{
    LlvmAdapterError, entry::NativeEntryPlan, render_verified_program,
    render_verified_program_with_entry, render_verified_program_with_entry_plan,
};

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

#[test]
fn borrowed_arguments_plan_builds_checked_argv_owner_wrapper() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("entry");
    let module = program.module_mut(module_id).expect("module");
    let string = module.add_string_owner_type();
    let arguments = module
        .add_sequential_container_type(SequentialContainerKind::Array, string)
        .expect("Array<String>");
    let entry = module
        .add_function("app", vec![], origin.clone())
        .expect("function");
    let block = module
        .function_mut(entry)
        .expect("function")
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: arguments,
            }],
            origin.clone(),
        )
        .expect("block");
    module
        .function_mut(entry)
        .expect("function")
        .set_terminator(block, TerminatorKind::Return { values: vec![] }, origin)
        .expect("return");

    let llvm = render_verified_program_with_entry_plan(
        &program,
        NativeEntryPlan::BorrowedArguments {
            function: entry,
            arguments,
            string,
        },
    )
    .expect("verified borrowed argv plan");
    assert!(llvm.contains("define i32 @main(i32 %0, ptr %1)"), "{llvm}");
    assert!(llvm.contains("@koven.entry.valid_utf8"), "{llvm}");
    assert!(llvm.contains("argv.preflight"), "{llvm}");
    assert!(llvm.contains("argv.construct"), "{llvm}");
    assert!(llvm.contains("call void @f0.app(ptr"), "{llvm}");
    assert!(llvm.contains("call void @koven.drop.t"), "{llvm}");
    assert!(
        llvm.contains(
            "br i1 %argv.preflight.remains, label %argv.preflight.body, label %argv.allocate"
        ),
        "allocation must be reachable only after the full preflight loop: {llvm}"
    );
    let main = llvm.split("define i32 @main").nth(1).expect("main body");
    let before_allocate = main
        .split("argv.allocate:")
        .next()
        .expect("pre-allocation CFG");
    assert!(
        !before_allocate.contains("call ptr @malloc"),
        "pre-allocation CFG must not allocate: {before_allocate}"
    );
    let allocation = main.find("call ptr @malloc").expect("owner allocation");
    let owner_store = main
        .find("argv.owner.with_buffer")
        .expect("Array owner store");
    assert!(
        allocation < owner_store,
        "Array owner exists only after allocation"
    );
}

#[test]
fn borrowed_arguments_plan_rejects_wrong_mode_kind_and_string_identity() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("entry");
    let module = program.module_mut(module_id).expect("module");
    let string = module.add_string_owner_type();
    let array = module
        .add_sequential_container_type(SequentialContainerKind::Array, string)
        .expect("Array<String>");
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, string)
        .expect("List<String>");
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
        .add_block(vec![EntityType::Value(array)], origin.clone())
        .expect("block");
    module
        .function_mut(entry)
        .expect("function")
        .set_terminator(block, TerminatorKind::Abort, origin)
        .expect("abort");

    for plan in [
        NativeEntryPlan::BorrowedArguments {
            function: entry,
            arguments: array,
            string,
        },
        NativeEntryPlan::BorrowedArguments {
            function: entry,
            arguments: list,
            string,
        },
        NativeEntryPlan::BorrowedArguments {
            function: entry,
            arguments: array,
            string: integer,
        },
    ] {
        assert!(matches!(
            render_verified_program_with_entry_plan(&program, plan),
            Err(LlvmAdapterError::InvalidEntry(message)) if message.contains("Array<String>")
        ));
    }
}
