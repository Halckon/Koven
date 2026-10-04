//! LLVM 21 / Inkwell 兼容边界。

mod adapter;
mod aggregate;
#[cfg(test)]
mod aggregate_tests;
mod closure;
#[cfg(test)]
mod closure_tests;
mod container;
#[cfg(test)]
mod container_tests;
mod debug;
#[cfg(test)]
mod debug_tests;
#[cfg(test)]
pub(crate) mod emission_failure;
mod entities;
pub(crate) mod entry;
#[cfg(test)]
mod entry_tests;
pub(crate) mod layout;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod nullable_tests;
#[cfg(test)]
mod object_tests;
mod runtime;
#[cfg(test)]
mod runtime_tests;
mod scalar;
mod string;
#[cfg(test)]
mod string_tests;
#[cfg(test)]
mod synthetic_zst_tests;
mod tagged;
mod type_map;
#[cfg(test)]
mod unit_storage_tests;

use inkwell::OptimizationLevel;
use inkwell::builder::BuilderError;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple,
};
use lang_frontend::source::SourceMap;

use crate::ssa::model::Program;

const MACOS_TARGET: &str = "aarch64-apple-darwin";
const LINUX_TARGET: &str = "x86_64-unknown-linux-gnu";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LlvmAdapterError {
    InvalidSsa(String),
    InvalidEntry(String),
    Debug(String),
    InvalidLayout(layout::TargetLayoutError),
    Object(String),
    Target(String),
    Build(String),
    Unsupported(String),
    Verify(String),
}

impl From<BuilderError> for LlvmAdapterError {
    fn from(error: BuilderError) -> Self {
        Self::Build(error.to_string())
    }
}

/// 把已验证的 target-independent SSA 映射为当前受支持宿主的 LLVM IR 文本。
pub(crate) fn render_verified_program(program: &Program) -> Result<String, LlvmAdapterError> {
    adapter::render_verified_program(program, None)
}

#[cfg(test)]
pub(crate) fn render_verified_program_with_entry(
    program: &Program,
    entry: crate::ssa::model::FunctionId,
) -> Result<String, LlvmAdapterError> {
    adapter::render_verified_program(
        program,
        Some(entry::NativeEntryPlan::NoArguments { function: entry }),
    )
}

#[cfg(test)]
pub(crate) fn render_verified_program_with_entry_plan(
    program: &Program,
    entry: entry::NativeEntryPlan,
) -> Result<String, LlvmAdapterError> {
    adapter::render_verified_program(program, Some(entry))
}

pub(crate) fn emit_verified_object(
    program: &Program,
    sources: &SourceMap,
    entry: impl Into<entry::NativeEntryPlan>,
    path: &std::path::Path,
) -> Result<(), LlvmAdapterError> {
    adapter::emit_verified_object(program, sources, entry.into(), path)
}

#[cfg(test)]
pub(crate) fn render_verified_program_with_debug(
    program: &Program,
    sources: &SourceMap,
    entry: crate::ssa::model::FunctionId,
) -> Result<String, LlvmAdapterError> {
    adapter::render_verified_program_with_debug(
        program,
        sources,
        entry::NativeEntryPlan::NoArguments { function: entry },
    )
}

/// 构造最小标量模块，以验证固定 LLVM 工具链、target backend 和 verifier 边界。
pub(crate) fn render_scalar_smoke_module() -> Result<String, LlvmAdapterError> {
    let (triple, target_machine) = native_target_machine()?;

    let context = Context::create();
    let module = context.create_module("koven.scalar-smoke");
    configure_module(&module, &triple, &target_machine);

    let int_type = context.i32_type();
    let function_type = int_type.fn_type(&[int_type.into(), int_type.into()], false);
    let function = module.add_function("add", function_type, None);
    let entry = context.append_basic_block(function, "entry");
    let builder = context.create_builder();
    builder.position_at_end(entry);

    let left = function
        .get_nth_param(0)
        .expect("函数签名固定包含第一个参数")
        .into_int_value();
    let right = function
        .get_nth_param(1)
        .expect("函数签名固定包含第二个参数")
        .into_int_value();
    let sum = builder
        .build_int_add(left, right, "sum")
        .map_err(|error| LlvmAdapterError::Build(error.to_string()))?;
    builder
        .build_return(Some(&sum))
        .map_err(|error| LlvmAdapterError::Build(error.to_string()))?;

    module
        .verify()
        .map_err(|error| LlvmAdapterError::Verify(error.to_string()))?;
    Ok(module.print_to_string().to_string())
}

pub(crate) fn native_target_triple() -> Result<&'static str, LlvmAdapterError> {
    target_for_host(
        std::env::consts::ARCH,
        std::env::consts::OS,
        cfg!(target_env = "gnu"),
        cfg!(target_pointer_width = "64"),
    )
}

fn target_for_host(
    arch: &str,
    os: &str,
    gnu: bool,
    pointer64: bool,
) -> Result<&'static str, LlvmAdapterError> {
    match (arch, os, gnu, pointer64) {
        ("aarch64", "macos", _, true) => Ok(MACOS_TARGET),
        ("x86_64", "linux", true, true) => Ok(LINUX_TARGET),
        _ => Err(LlvmAdapterError::Target(format!(
            "unsupported native host: {arch}-{os}; supported targets: {MACOS_TARGET}, {LINUX_TARGET}"
        ))),
    }
}

fn native_target_machine() -> Result<(TargetTriple, TargetMachine), LlvmAdapterError> {
    target_machine_for_triple(native_target_triple()?)
}

fn target_machine_for_triple(
    target_triple: &str,
) -> Result<(TargetTriple, TargetMachine), LlvmAdapterError> {
    match target_triple {
        MACOS_TARGET => Target::initialize_aarch64(&InitializationConfig::default()),
        LINUX_TARGET => Target::initialize_x86(&InitializationConfig::default()),
        _ => {
            return Err(LlvmAdapterError::Target(format!(
                "unsupported native target: {target_triple}"
            )));
        }
    }
    let triple = TargetTriple::create(target_triple);
    let target = Target::from_triple(&triple)
        .map_err(|error| LlvmAdapterError::Target(error.to_string()))?;
    let target_machine = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::None,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| {
            LlvmAdapterError::Target(format!("无法创建 {target_triple} target machine"))
        })?;
    Ok((triple, target_machine))
}

fn configure_module(module: &Module<'_>, triple: &TargetTriple, machine: &TargetMachine) {
    module.set_triple(triple);
    module.set_data_layout(&machine.get_target_data().get_data_layout());
}

#[cfg(test)]
mod tests {
    use inkwell::context::Context;
    use lang_frontend::source::SourceMap;

    use crate::ssa::model::{
        BinaryOperator, EntityId, EntityType, Operation, Origin, Program, SsaTypeKind,
        TerminatorKind, ValueId,
    };

    use super::{
        LINUX_TARGET, LlvmAdapterError, MACOS_TARGET, native_target_triple,
        render_scalar_smoke_module, render_verified_program, target_for_host,
        target_machine_for_triple,
    };

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    fn linux_host_renders_x86_64_gnu_target() {
        let llvm = render_scalar_smoke_module().expect("Linux target must be available");
        assert!(llvm.contains("target triple = \"x86_64-unknown-linux-gnu\""));
    }

    #[test]
    fn renders_deterministic_verified_native_scalar_module() {
        let first = render_scalar_smoke_module().expect("固定 LLVM 21 矩阵应生成合法模块");
        let second = render_scalar_smoke_module().expect("重复生成应保持确定");

        assert_eq!(first, second);
        let target = native_target_triple().expect("supported native host");
        assert!(first.contains(&format!("target triple = \"{target}\"")));
        assert!(first.contains("define i32 @add(i32 %0, i32 %1)"));
        assert!(first.contains("%sum = add i32 %0, %1"));
    }

    #[test]
    fn native_host_selection_rejects_unsupported_platforms() {
        assert_eq!(
            target_for_host("aarch64", "macos", false, true),
            Ok(MACOS_TARGET)
        );
        assert_eq!(
            target_for_host("x86_64", "linux", true, true),
            Ok(LINUX_TARGET)
        );
        for (arch, os, gnu, pointer64) in [
            ("x86_64", "linux", false, true),
            ("aarch64", "linux", true, true),
            ("x86_64", "macos", false, true),
            ("x86_64", "windows", true, true),
            ("x86_64", "linux", true, false),
        ] {
            assert!(matches!(
                target_for_host(arch, os, gnu, pointer64),
                Err(LlvmAdapterError::Target(_))
            ));
        }
    }

    #[test]
    fn both_supported_backends_keep_a_64_bit_pointer_layout() {
        for target in [MACOS_TARGET, LINUX_TARGET] {
            let (triple, machine) = target_machine_for_triple(target).expect("enabled backend");
            assert_eq!(triple.as_str().to_str().unwrap(), target);
            assert_eq!(machine.get_target_data().get_pointer_byte_size(None), 8);
        }
    }

    #[test]
    fn llvm_verifier_rejects_missing_terminator() {
        let context = Context::create();
        let module = context.create_module("invalid");
        let function = module.add_function("broken", context.void_type().fn_type(&[], false), None);
        context.append_basic_block(function, "entry");

        let error = module
            .verify()
            .expect_err("缺少 terminator 的 basic block 必须被 LLVM verifier 拒绝");
        assert!(error.to_string().contains("does not have terminator"));
    }

    #[test]
    fn adapter_rejects_invalid_ssa_before_constructing_llvm() {
        let mut sources = SourceMap::new();
        let source = sources.add_source("invalid.ko", "fun broken() {}").unwrap();
        let span = sources.span(source, 0, 15).unwrap();
        let mut program = Program::default();
        let module = program.add_module("invalid");
        let module = program.module_mut(module).unwrap();
        let function = module
            .add_function("broken", Vec::new(), Origin::Source(span))
            .unwrap();
        module
            .function_mut(function)
            .unwrap()
            .add_block(Vec::new(), Origin::Source(span))
            .unwrap();

        let error = render_verified_program(&program)
            .expect_err("invalid SSA must not reach LLVM construction");
        assert!(matches!(error, LlvmAdapterError::InvalidSsa(_)));
    }

    #[test]
    fn adapter_maps_verified_unchecked_scalar_primitives() {
        let mut sources = SourceMap::new();
        let source = sources.add_source("primitive.ko", "fun primitive").unwrap();
        let origin = Origin::Source(sources.span(source, 0, 13).unwrap());
        let mut program = Program::default();
        let module = program.add_module("primitive");
        let module = program.module_mut(module).unwrap();
        let integer = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: false,
        });
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let function = module
            .add_function("primitive", vec![boolean], origin.clone())
            .unwrap();
        let function = module.function_mut(function).unwrap();
        let entry = function
            .add_block(
                vec![EntityType::Value(integer), EntityType::Value(integer)],
                origin.clone(),
            )
            .unwrap();
        let parameters = function.block(entry).unwrap().parameters.clone();
        let left = value(parameters[0]);
        let right = value(parameters[1]);
        let (_, sum) = function
            .append_instruction(
                entry,
                Operation::Binary {
                    operator: BinaryOperator::Add,
                    left,
                    right,
                },
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .unwrap();
        let (_, copied) = function
            .append_instruction(
                entry,
                Operation::Copy {
                    source: value(sum[0]),
                },
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .unwrap();
        let (_, comparison) = function
            .append_instruction(
                entry,
                Operation::Binary {
                    operator: BinaryOperator::LessThan,
                    left: value(copied[0]),
                    right,
                },
                vec![EntityType::Value(boolean)],
                origin.clone(),
            )
            .unwrap();
        function
            .set_terminator(
                entry,
                TerminatorKind::Return {
                    values: vec![value(comparison[0])],
                },
                origin,
            )
            .unwrap();

        let llvm = render_verified_program(&program).expect("verified primitives must lower");
        assert!(llvm.contains("add i32 %v0, %v1"));
        assert!(llvm.contains("icmp ult i32"));
    }

    fn value(entity: EntityId) -> ValueId {
        let EntityId::Value(value) = entity else {
            panic!("expected ValueId")
        };
        value
    }
}
