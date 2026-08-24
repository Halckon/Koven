use std::path::Path;

use inkwell::{
    context::Context,
    module::Module as LlvmModule,
    targets::{FileType, TargetMachine, TargetTriple},
};

use crate::ssa::{
    model::{FunctionId, Program},
    verify::verify_program,
};

use super::super::{LlvmAdapterError, configure_module, entry, first_target_machine};
use super::ModuleLowerer;

pub(crate) fn render_verified_program(
    program: &Program,
    native_entry: Option<FunctionId>,
) -> Result<String, LlvmAdapterError> {
    let (triple, target_machine) = first_target_machine()?;
    let context = Context::create();
    let llvm_module =
        lower_verified_module(&context, program, native_entry, &triple, &target_machine)?;
    Ok(llvm_module.print_to_string().to_string())
}

pub(crate) fn emit_verified_object(
    program: &Program,
    native_entry: FunctionId,
    path: &Path,
) -> Result<(), LlvmAdapterError> {
    let (triple, target_machine) = first_target_machine()?;
    let context = Context::create();
    let llvm_module = lower_verified_module(
        &context,
        program,
        Some(native_entry),
        &triple,
        &target_machine,
    )?;
    target_machine
        .write_to_file(&llvm_module, FileType::Object, path)
        .map_err(|error| LlvmAdapterError::Object(error.to_string()))
}

fn lower_verified_module<'ctx>(
    context: &'ctx Context,
    program: &Program,
    native_entry: Option<FunctionId>,
    triple: &TargetTriple,
    target_machine: &TargetMachine,
) -> Result<LlvmModule<'ctx>, LlvmAdapterError> {
    verify_program(program).map_err(|error| LlvmAdapterError::InvalidSsa(error.to_string()))?;
    let [ssa_module] = program.modules.as_slice() else {
        return Err(LlvmAdapterError::Unsupported(
            "当前 LLVM adapter 只接受一个 SSA module".to_owned(),
        ));
    };
    let llvm_module = context.create_module(&ssa_module.name);
    configure_module(&llvm_module, triple, target_machine);
    if let Some(native_entry) = native_entry {
        entry::validate(ssa_module, native_entry)?;
    }
    ModuleLowerer::new(
        context,
        &llvm_module,
        ssa_module,
        &target_machine.get_target_data(),
    )?
    .lower(native_entry)?;
    llvm_module
        .verify()
        .map_err(|error| LlvmAdapterError::Verify(error.to_string()))?;
    Ok(llvm_module)
}
