//! Concrete closure value construction and indirect invocation.

use inkwell::{
    builder::Builder,
    values::{
        BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue, StructValue, ValueKind,
    },
};

use crate::ssa::model::{Module, SsaTypeId, SsaTypeKind};

use super::{LlvmAdapterError, type_map::TypeMap};

pub(super) fn function_address(target: FunctionValue<'_>) -> PointerValue<'_> {
    target.as_global_value().as_pointer_value()
}

pub(super) fn construct<'ctx>(
    builder: &Builder<'ctx>,
    types: &TypeMap<'ctx>,
    closure: SsaTypeId,
    target: FunctionValue<'ctx>,
    captures: &[BasicValueEnum<'ctx>],
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let layout = types.closure_layout(closure)?;
    let mut environment = layout.environment.const_zero();
    for (index, capture) in captures.iter().enumerate() {
        environment = builder
            .build_insert_value(
                environment,
                *capture,
                index as u32,
                &format!("{name}.capture{index}"),
            )?
            .into_struct_value();
    }
    let with_function = builder
        .build_insert_value(
            layout.value.const_zero(),
            function_address(target),
            0,
            &format!("{name}.function"),
        )?
        .into_struct_value();
    let value = builder
        .build_insert_value(
            with_function,
            environment,
            1,
            &format!("{name}.environment"),
        )?
        .into_struct_value();
    value.set_name(name);
    Ok(value)
}

pub(super) fn invoke<'ctx>(
    builder: &Builder<'ctx>,
    module: &Module,
    types: &TypeMap<'ctx>,
    callable_type: SsaTypeId,
    callable: BasicValueEnum<'ctx>,
    arguments: &[BasicValueEnum<'ctx>],
    name: &str,
) -> Result<Option<BasicValueEnum<'ctx>>, LlvmAdapterError> {
    let signature = module.callable_signature(callable_type).ok_or_else(|| {
        LlvmAdapterError::InvalidSsa("indirect call operand has no callable signature".to_owned())
    })?;
    let (function, environment) = match module.type_kind(callable_type) {
        Some(SsaTypeKind::FunctionPointer { .. }) => (callable.into_pointer_value(), None),
        Some(SsaTypeKind::ConcreteClosure { .. }) => {
            let callable = callable.into_struct_value();
            let function = builder
                .build_extract_value(callable, 0, &format!("{name}.function"))?
                .into_pointer_value();
            let environment = builder
                .build_extract_value(callable, 1, &format!("{name}.environment"))?
                .into_struct_value();
            let storage = builder.build_alloca(
                environment.get_type(),
                &format!("{name}.environment.storage"),
            )?;
            builder.build_store(storage, environment)?;
            (function, Some(storage))
        }
        _ => {
            return Err(LlvmAdapterError::InvalidSsa(
                "indirect call operand type is not callable".to_owned(),
            ));
        }
    };
    let function_type = types.callable_function_type(signature, environment.is_some())?;
    let mut operands = environment
        .map(BasicMetadataValueEnum::from)
        .into_iter()
        .collect::<Vec<_>>();
    operands.extend(arguments.iter().copied().map(BasicMetadataValueEnum::from));
    let call_name = if signature.returns.is_empty() {
        ""
    } else {
        name
    };
    let call = builder.build_indirect_call(function_type, function, &operands, call_name)?;
    Ok(match call.try_as_basic_value() {
        ValueKind::Basic(value) => Some(value),
        ValueKind::Instruction(_) => None,
    })
}
