//! LLVM 21 / Inkwell 兼容边界。

mod adapter;
#[cfg(test)]
mod aggregate_tests;
mod runtime;
#[cfg(test)]
mod runtime_tests;
mod type_map;

use inkwell::OptimizationLevel;
use inkwell::builder::BuilderError;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple,
};

use crate::ssa::model::Program;

const FIRST_TARGET: &str = "aarch64-apple-darwin";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LlvmAdapterError {
    InvalidSsa(String),
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

/// 把已验证的 target-independent SSA 映射为首个 target 的 LLVM IR 文本。
pub(crate) fn render_verified_program(program: &Program) -> Result<String, LlvmAdapterError> {
    adapter::render_verified_program(program)
}

/// 构造最小标量模块，以验证固定 LLVM 工具链、target backend 和 verifier 边界。
pub(crate) fn render_scalar_smoke_module() -> Result<String, LlvmAdapterError> {
    let (triple, target_machine) = first_target_machine()?;

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

fn first_target_machine() -> Result<(TargetTriple, TargetMachine), LlvmAdapterError> {
    Target::initialize_aarch64(&InitializationConfig::default());
    let triple = TargetTriple::create(FIRST_TARGET);
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
        .ok_or_else(|| LlvmAdapterError::Target("无法创建 AArch64 target machine".to_owned()))?;
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
        FIRST_TARGET, LlvmAdapterError, render_scalar_smoke_module, render_verified_program,
    };

    #[test]
    fn renders_deterministic_verified_aarch64_scalar_module() {
        let first = render_scalar_smoke_module().expect("固定 LLVM 21 矩阵应生成合法模块");
        let second = render_scalar_smoke_module().expect("重复生成应保持确定");

        assert_eq!(first, second);
        assert!(first.contains(&format!("target triple = \"{FIRST_TARGET}\"")));
        assert!(first.contains("define i32 @add(i32 %0, i32 %1)"));
        assert!(first.contains("%sum = add i32 %0, %1"));
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
