use std::path::Path;

use inkwell::{
    context::Context,
    module::Module as LlvmModule,
    targets::{FileType, TargetMachine, TargetTriple},
};
use lang_frontend::source::SourceMap;

use crate::ssa::{model::Program, verify::verify_program};

use super::super::entry::NativeEntryPlan;
use super::super::{LlvmAdapterError, configure_module, debug, entry, first_target_machine};
use super::ModuleLowerer;

pub(crate) fn render_verified_program(
    program: &Program,
    native_entry: Option<NativeEntryPlan>,
) -> Result<String, LlvmAdapterError> {
    let (triple, target_machine) = first_target_machine()?;
    let context = Context::create();
    let llvm_module = lower_verified_module(
        &context,
        program,
        native_entry,
        None,
        &triple,
        &target_machine,
    )?;
    Ok(llvm_module.print_to_string().to_string())
}

pub(crate) fn render_verified_program_with_debug(
    program: &Program,
    sources: &SourceMap,
    native_entry: NativeEntryPlan,
) -> Result<String, LlvmAdapterError> {
    let (triple, target_machine) = first_target_machine()?;
    let context = Context::create();
    let llvm_module = lower_verified_module(
        &context,
        program,
        Some(native_entry),
        Some(sources),
        &triple,
        &target_machine,
    )?;
    Ok(llvm_module.print_to_string().to_string())
}

pub(crate) fn emit_verified_object(
    program: &Program,
    sources: &SourceMap,
    native_entry: NativeEntryPlan,
    path: &Path,
) -> Result<(), LlvmAdapterError> {
    let (triple, target_machine) = first_target_machine()?;
    let context = Context::create();
    let llvm_module = lower_verified_module(
        &context,
        program,
        Some(native_entry),
        Some(sources),
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
    native_entry: Option<NativeEntryPlan>,
    debug_sources: Option<&SourceMap>,
    triple: &TargetTriple,
    target_machine: &TargetMachine,
) -> Result<LlvmModule<'ctx>, LlvmAdapterError> {
    verify_program(program).map_err(|error| LlvmAdapterError::InvalidSsa(error.to_string()))?;
    let [ssa_module] = program.modules.as_slice() else {
        return Err(LlvmAdapterError::Unsupported(
            "当前 LLVM adapter 只接受一个 SSA module".to_owned(),
        ));
    };
    if let Some(native_entry) = native_entry {
        entry::validate(ssa_module, native_entry)?;
    }
    let debug_plan = match debug_sources {
        Some(sources) => {
            let native_entry = native_entry.ok_or_else(|| {
                LlvmAdapterError::Debug("debug lowering 缺少 native entry".to_owned())
            })?;
            Some(debug::DebugPlan::build(
                sources,
                ssa_module,
                native_entry.function(),
            )?)
        }
        None => None,
    };
    let llvm_module = context.create_module(&ssa_module.name);
    configure_module(&llvm_module, triple, target_machine);
    let debug = debug_plan.map(|plan| debug::DebugEmitter::new(context, &llvm_module, plan));
    ModuleLowerer::new(
        context,
        &llvm_module,
        ssa_module,
        &target_machine.get_target_data(),
        debug,
        native_entry,
    )?
    .lower(native_entry)?;
    llvm_module
        .verify()
        .map_err(|error| LlvmAdapterError::Verify(error.to_string()))?;
    Ok(llvm_module)
}
