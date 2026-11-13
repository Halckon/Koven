//! Concrete closure value construction and indirect invocation.

use inkwell::{
    attributes::{Attribute, AttributeLoc},
    builder::Builder,
    types::FunctionType,
    values::{
        BasicMetadataValueEnum, BasicValueEnum, CallSiteValue, FunctionValue, PointerValue,
        StructValue, ValueKind,
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
    let prepared = prepare(builder, module, types, callable_type, callable, name)?;
    invoke_prepared(builder, &prepared, arguments, name)
}

/// Physical call preparation is independent of logical invocation, so loops can reuse storage.
pub(super) struct PreparedCallable<'ctx> {
    function: PointerValue<'ctx>,
    environment: Option<PointerValue<'ctx>>,
    function_type: inkwell::types::FunctionType<'ctx>,
    returns_void: bool,
    never_returns: bool,
}

pub(super) fn prepare<'ctx>(
    builder: &Builder<'ctx>,
    module: &Module,
    types: &TypeMap<'ctx>,
    callable_type: SsaTypeId,
    callable: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<PreparedCallable<'ctx>, LlvmAdapterError> {
    let signature = module
        .callable_signature(callable_type)
        .or_else(|| module.callable_view_signature(callable_type))
        .ok_or_else(|| {
            LlvmAdapterError::InvalidSsa(
                "indirect call operand has no callable signature".to_owned(),
            )
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
        // ADR-0034: a loaded view descriptor already holds the environment pointer.
        Some(SsaTypeKind::CallableView { .. }) => {
            let descriptor = callable.into_struct_value();
            let function = builder
                .build_extract_value(descriptor, 0, &format!("{name}.code"))?
                .into_pointer_value();
            let environment = builder
                .build_extract_value(descriptor, 1, &format!("{name}.env"))?
                .into_pointer_value();
            (function, Some(environment))
        }
        _ => {
            return Err(LlvmAdapterError::InvalidSsa(
                "indirect call operand type is not callable".to_owned(),
            ));
        }
    };
    let function_type = types.callable_function_type(signature, environment.is_some())?;
    Ok(PreparedCallable {
        function,
        environment,
        function_type,
        returns_void: signature.returns.is_empty(),
        never_returns: signature.never_returns,
    })
}

pub(super) fn invoke_prepared<'ctx>(
    builder: &Builder<'ctx>,
    prepared: &PreparedCallable<'ctx>,
    arguments: &[BasicValueEnum<'ctx>],
    name: &str,
) -> Result<Option<BasicValueEnum<'ctx>>, LlvmAdapterError> {
    let PreparedCallable {
        function,
        environment,
        function_type,
        returns_void,
        never_returns,
    } = prepared;
    let mut operands = environment
        .map(BasicMetadataValueEnum::from)
        .into_iter()
        .collect::<Vec<_>>();
    operands.extend(arguments.iter().copied().map(BasicMetadataValueEnum::from));
    let call_name = if *returns_void { "" } else { name };
    let call = builder.build_indirect_call(*function_type, *function, &operands, call_name)?;
    if *never_returns {
        mark_call_noreturn(call, *function_type);
    }
    Ok(match call.try_as_basic_value() {
        ValueKind::Basic(value) => Some(value),
        ValueKind::Instruction(_) => None,
    })
}

pub(super) fn mark_call_noreturn(call: CallSiteValue<'_>, function_type: FunctionType<'_>) {
    let kind = Attribute::get_named_enum_kind_id("noreturn");
    if kind != 0 {
        call.add_attribute(
            AttributeLoc::Function,
            function_type.get_context().create_enum_attribute(kind, 0),
        );
    }
}

pub(crate) mod utf8 {
    //! Shared strict UTF-8 scanner for argv preflight and standard text decoding.
    use super::super::LlvmAdapterError;
    use inkwell::{
        AddressSpace, IntPredicate,
        context::Context,
        module::{Linkage, Module as LlvmModule},
        values::{FunctionValue, IntValue},
    };

    /// Return target-usize MAX for valid input, otherwise the first invalid sequence start.
    /// The caller provides a readable byte range; a zero length never dereferences its pointer.
    pub(crate) fn define_error_offset<'ctx>(
        context: &'ctx Context,
        module: &LlvmModule<'ctx>,
        size: inkwell::types::IntType<'ctx>,
    ) -> Result<FunctionValue<'ctx>, LlvmAdapterError> {
        if let Some(function) = module.get_function("koven.utf8.error_offset") {
            return Ok(function);
        }
        let pointer = context.ptr_type(AddressSpace::default());
        let function = module.add_function(
            "koven.utf8.error_offset",
            size.fn_type(&[pointer.into(), size.into()], false),
            Some(Linkage::Internal),
        );
        let bytes = function
            .get_nth_param(0)
            .expect("bytes")
            .into_pointer_value();
        let length = function.get_nth_param(1).expect("length").into_int_value();
        let builder = context.create_builder();
        let entry = context.append_basic_block(function, "entry");
        let header = context.append_basic_block(function, "scan");
        let body = context.append_basic_block(function, "byte");
        let advance = context.append_basic_block(function, "advance");
        let invalid = context.append_basic_block(function, "invalid");
        let done = context.append_basic_block(function, "done");
        builder.position_at_end(entry);
        builder.build_unconditional_branch(header)?;
        builder.position_at_end(header);
        let index_phi = builder.build_phi(size, "index")?;
        let start_phi = builder.build_phi(size, "sequence.start")?;
        let remaining_phi = builder.build_phi(context.i8_type(), "remaining")?;
        let minimum_phi = builder.build_phi(context.i8_type(), "minimum")?;
        let maximum_phi = builder.build_phi(context.i8_type(), "maximum")?;
        let zero_size = size.const_zero();
        let zero8 = context.i8_type().const_zero();
        let min_cont = context.i8_type().const_int(0x80, false);
        let max_cont = context.i8_type().const_int(0xbf, false);
        index_phi.add_incoming(&[(&zero_size, entry)]);
        start_phi.add_incoming(&[(&zero_size, entry)]);
        remaining_phi.add_incoming(&[(&zero8, entry)]);
        minimum_phi.add_incoming(&[(&min_cont, entry)]);
        maximum_phi.add_incoming(&[(&max_cont, entry)]);
        let index = index_phi.as_basic_value().into_int_value();
        let start = start_phi.as_basic_value().into_int_value();
        let remaining = remaining_phi.as_basic_value().into_int_value();
        let minimum = minimum_phi.as_basic_value().into_int_value();
        let maximum = maximum_phi.as_basic_value().into_int_value();
        let remains = builder.build_int_compare(IntPredicate::ULT, index, length, "remains")?;
        builder.build_conditional_branch(remains, body, done)?;
        builder.position_at_end(done);
        let complete = builder.build_int_compare(IntPredicate::EQ, remaining, zero8, "complete")?;
        let result = builder
            .build_select(complete, size.const_all_ones(), start, "result")?
            .into_int_value();
        builder.build_return(Some(&result))?;

        builder.position_at_end(body);
        // SAFETY: body is reachable only for index < length.
        let address =
            unsafe { builder.build_in_bounds_gep(context.i8_type(), bytes, &[index], "address")? };
        let byte = builder
            .build_load(context.i8_type(), address, "byte")?
            .into_int_value();
        let continuing =
            builder.build_int_compare(IntPredicate::NE, remaining, zero8, "continuing")?;
        let at_least_min =
            builder.build_int_compare(IntPredicate::UGE, byte, minimum, "at_least_min")?;
        let at_most_max =
            builder.build_int_compare(IntPredicate::ULE, byte, maximum, "at_most_max")?;
        let valid_cont = builder.build_and(at_least_min, at_most_max, "valid_cont")?;
        let ascii = builder.build_int_compare(
            IntPredicate::ULT,
            byte,
            context.i8_type().const_int(0x80, false),
            "ascii",
        )?;
        let lead2 = int_range(&builder, byte, 0xc2, 0xdf, "lead2")?;
        let lead3 = int_range(&builder, byte, 0xe0, 0xef, "lead3")?;
        let lead4 = int_range(&builder, byte, 0xf0, 0xf4, "lead4")?;
        let valid_lead = builder.build_or(
            ascii,
            builder.build_or(lead2, builder.build_or(lead3, lead4, "lead34")?, "lead234")?,
            "valid_lead",
        )?;
        let valid = builder
            .build_select(continuing, valid_cont, valid_lead, "valid")?
            .into_int_value();
        builder.build_conditional_branch(valid, advance, invalid)?;
        builder.position_at_end(invalid);
        let error_start = builder
            .build_select(continuing, start, index, "error.start")?
            .into_int_value();
        builder.build_return(Some(&error_start))?;

        builder.position_at_end(advance);
        let continued_remaining = builder.build_int_sub(
            remaining,
            context.i8_type().const_int(1, false),
            "continued_remaining",
        )?;
        let lead2_remaining = builder
            .build_select(
                lead2,
                context.i8_type().const_int(1, false),
                zero8,
                "lead2.remaining",
            )?
            .into_int_value();
        let lead3_remaining = builder
            .build_select(
                lead3,
                context.i8_type().const_int(2, false),
                lead2_remaining,
                "lead3.remaining",
            )?
            .into_int_value();
        let fresh_remaining = builder
            .build_select(
                lead4,
                context.i8_type().const_int(3, false),
                lead3_remaining,
                "lead4.remaining",
            )?
            .into_int_value();
        let next_remaining = builder
            .build_select(
                continuing,
                continued_remaining,
                fresh_remaining,
                "next.remaining",
            )?
            .into_int_value();
        let e0 = builder.build_int_compare(
            IntPredicate::EQ,
            byte,
            context.i8_type().const_int(0xe0, false),
            "e0",
        )?;
        let ed = builder.build_int_compare(
            IntPredicate::EQ,
            byte,
            context.i8_type().const_int(0xed, false),
            "ed",
        )?;
        let f0 = builder.build_int_compare(
            IntPredicate::EQ,
            byte,
            context.i8_type().const_int(0xf0, false),
            "f0",
        )?;
        let f4 = builder.build_int_compare(
            IntPredicate::EQ,
            byte,
            context.i8_type().const_int(0xf4, false),
            "f4",
        )?;
        let f0_min = builder
            .build_select(
                f0,
                context.i8_type().const_int(0x90, false),
                min_cont,
                "f0.min",
            )?
            .into_int_value();
        let fresh_min = builder
            .build_select(
                e0,
                context.i8_type().const_int(0xa0, false),
                f0_min,
                "e0.min",
            )?
            .into_int_value();
        let f4_max = builder
            .build_select(
                f4,
                context.i8_type().const_int(0x8f, false),
                max_cont,
                "f4.max",
            )?
            .into_int_value();
        let fresh_max = builder
            .build_select(
                ed,
                context.i8_type().const_int(0x9f, false),
                f4_max,
                "ed.max",
            )?
            .into_int_value();
        let next_start = builder
            .build_select(continuing, start, index, "next.start")?
            .into_int_value();
        let next_index = builder.build_int_add(index, size.const_int(1, false), "next.index")?;
        builder.build_unconditional_branch(header)?;
        index_phi.add_incoming(&[(&next_index, advance)]);
        start_phi.add_incoming(&[(&next_start, advance)]);
        remaining_phi.add_incoming(&[(&next_remaining, advance)]);
        minimum_phi.add_incoming(&[(&fresh_min, advance)]);
        maximum_phi.add_incoming(&[(&fresh_max, advance)]);
        Ok(function)
    }

    fn int_range<'ctx>(
        builder: &inkwell::builder::Builder<'ctx>,
        value: IntValue<'ctx>,
        minimum: u64,
        maximum: u64,
        name: &str,
    ) -> Result<IntValue<'ctx>, LlvmAdapterError> {
        let ty = value.get_type();
        let lower = builder.build_int_compare(
            IntPredicate::UGE,
            value,
            ty.const_int(minimum, false),
            &format!("{name}.lower"),
        )?;
        let upper = builder.build_int_compare(
            IntPredicate::ULE,
            value,
            ty.const_int(maximum, false),
            &format!("{name}.upper"),
        )?;
        Ok(builder.build_and(lower, upper, name)?)
    }
}

pub(crate) mod debug {
    //! ADR-0011 的首个 DWARF source line 映射边界。

    use std::collections::BTreeMap;

    use inkwell::{
        builder::Builder,
        context::Context,
        debug_info::{
            AsDIScope, DIFile, DIFlags, DIFlagsConstants, DISubprogram, DWARFEmissionKind,
            DWARFSourceLanguage, DebugInfoBuilder, debug_metadata_version,
        },
        module::{FlagBehavior, Module as LlvmModule},
        values::FunctionValue,
    };
    use lang_frontend::source::{SourceId, SourceMap};

    use crate::ssa::model::{Function, FunctionId, Module, Origin};

    use super::super::LlvmAdapterError;

    pub(crate) struct DebugPlan<'sources> {
        sources: &'sources SourceMap,
        primary_source: String,
        primary_file_name: String,
        source_files: BTreeMap<String, String>,
    }

    impl<'sources> DebugPlan<'sources> {
        pub(crate) fn build(
            sources: &'sources SourceMap,
            module: &Module,
            native_entry: FunctionId,
        ) -> Result<Self, LlvmAdapterError> {
            let entry = module.function(native_entry).ok_or_else(|| {
                LlvmAdapterError::Debug("debug native entry function 不存在".to_owned())
            })?;
            let position = resolve_position(sources, &entry.origin)?;
            let primary_file_name = sources
                .source_file_name(position.source_id)
                .map_err(|error| LlvmAdapterError::Debug(error.to_string()))?
                .to_owned();
            let primary_source = position.source_name;
            let mut plan = Self {
                sources,
                primary_source,
                primary_file_name,
                source_files: BTreeMap::new(),
            };

            for function in &module.functions {
                plan.validate_origin(&function.origin)?;
                for block in &function.blocks {
                    plan.validate_origin(&block.origin)?;
                    for entity in &block.parameters {
                        let entity = function.entity(*entity).ok_or_else(|| {
                            LlvmAdapterError::Debug(
                                "debug preflight 遇到未知 block parameter".to_owned(),
                            )
                        })?;
                        plan.validate_origin(&entity.origin)?;
                    }
                    for instruction in &block.instructions {
                        let instruction = function.instruction(*instruction).ok_or_else(|| {
                            LlvmAdapterError::Debug(
                                "debug preflight 遇到未知 instruction".to_owned(),
                            )
                        })?;
                        plan.validate_origin(&instruction.origin)?;
                        for result in &instruction.results {
                            let entity = function.entity(*result).ok_or_else(|| {
                                LlvmAdapterError::Debug(
                                    "debug preflight 遇到未知 instruction result".to_owned(),
                                )
                            })?;
                            plan.validate_origin(&entity.origin)?;
                        }
                    }
                    let terminator = block.terminator.as_ref().ok_or_else(|| {
                        LlvmAdapterError::Debug("debug preflight 遇到缺失 terminator".to_owned())
                    })?;
                    plan.validate_origin(&terminator.origin)?;
                }
            }
            Ok(plan)
        }

        fn validate_origin(&mut self, origin: &Origin) -> Result<(), LlvmAdapterError> {
            let position = resolve_position(self.sources, origin)?;
            let file_name = self
                .sources
                .source_file_name(position.source_id)
                .map_err(|error| LlvmAdapterError::Debug(error.to_string()))?;
            self.source_files
                .entry(position.source_name)
                .or_insert_with(|| file_name.to_owned());
            Ok(())
        }

        fn position(&self, origin: &Origin) -> Result<DebugPosition, LlvmAdapterError> {
            resolve_position(self.sources, origin)
        }

        #[cfg(test)]
        pub(crate) fn ordered_source_names(&self) -> Vec<&str> {
            self.source_files.keys().map(String::as_str).collect()
        }
    }

    pub(crate) struct DebugEmitter<'ctx, 'sources> {
        builder: DebugInfoBuilder<'ctx>,
        files: BTreeMap<String, DIFile<'ctx>>,
        subprograms: BTreeMap<FunctionId, DISubprogram<'ctx>>,
        plan: DebugPlan<'sources>,
    }

    impl<'ctx, 'sources> DebugEmitter<'ctx, 'sources> {
        pub(crate) fn new(
            context: &'ctx Context,
            llvm: &LlvmModule<'ctx>,
            plan: DebugPlan<'sources>,
        ) -> Self {
            let version = context
                .i32_type()
                .const_int(u64::from(debug_metadata_version()), false);
            llvm.add_basic_value_flag("Debug Info Version", FlagBehavior::Warning, version);
            let (builder, compile_unit) = llvm.create_debug_info_builder(
                false,
                DWARFSourceLanguage::C,
                &plan.primary_file_name,
                "",
                "kovenc",
                false,
                "",
                0,
                "",
                DWARFEmissionKind::LineTablesOnly,
                0,
                false,
                false,
                "",
                "",
            );
            let mut files = BTreeMap::new();
            files.insert(plan.primary_source.clone(), compile_unit.get_file());
            for (source_name, file_name) in &plan.source_files {
                if source_name != &plan.primary_source {
                    files.insert(source_name.clone(), builder.create_file(file_name, ""));
                }
            }
            Self {
                builder,
                files,
                subprograms: BTreeMap::new(),
                plan,
            }
        }

        pub(crate) fn attach_function(
            &mut self,
            function: &Function,
            llvm_function: FunctionValue<'ctx>,
        ) -> Result<(), LlvmAdapterError> {
            let position = self.plan.position(&function.origin)?;
            let file = self.file(&position.source_name)?;
            let subroutine = self
                .builder
                .create_subroutine_type(file, None, &[], DIFlags::ZERO);
            let subprogram = self.builder.create_function(
                file.as_debug_info_scope(),
                &function.name,
                llvm_function.get_name().to_str().ok(),
                file,
                position.line,
                subroutine,
                true,
                true,
                position.line,
                DIFlags::ZERO,
                false,
            );
            llvm_function.set_subprogram(subprogram);
            self.subprograms.insert(function.id, subprogram);
            Ok(())
        }

        pub(crate) fn set_location(
            &self,
            context: &'ctx Context,
            builder: &Builder<'ctx>,
            function: FunctionId,
            origin: &Origin,
        ) -> Result<(), LlvmAdapterError> {
            let position = self.plan.position(origin)?;
            let subprogram = self.subprograms.get(&function).copied().ok_or_else(|| {
                LlvmAdapterError::Debug("Koven function 缺少 debug subprogram".to_owned())
            })?;
            let location = self.builder.create_debug_location(
                context,
                position.line,
                position.column,
                subprogram.as_debug_info_scope(),
                None,
            );
            builder.set_current_debug_location(location);
            Ok(())
        }

        /// `  at <logical-path>:<line>:<column>\n` for the Guide v0.46 abort output.
        pub(crate) fn abort_location(&self, origin: &Origin) -> Result<String, LlvmAdapterError> {
            let position = self.plan.position(origin)?;
            Ok(format!(
                "  at {}:{}:{}\n",
                position.source_name, position.line, position.column
            ))
        }

        pub(crate) fn finalize(&self) {
            self.builder.finalize();
        }

        fn file(&self, source_name: &str) -> Result<DIFile<'ctx>, LlvmAdapterError> {
            self.files
                .get(source_name)
                .copied()
                .ok_or_else(|| LlvmAdapterError::Debug("origin source 缺少 DIFile".to_owned()))
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct DebugPosition {
        source_id: SourceId,
        source_name: String,
        line: u32,
        column: u32,
    }

    fn resolve_position(
        sources: &SourceMap,
        origin: &Origin,
    ) -> Result<DebugPosition, LlvmAdapterError> {
        let span = origin.span();
        sources
            .slice(span)
            .map_err(|error| LlvmAdapterError::Debug(error.to_string()))?;
        let source_id = span.source_id();
        let source_name = sources
            .source_name(source_id)
            .map_err(|error| LlvmAdapterError::Debug(error.to_string()))?
            .to_owned();
        let position = sources
            .position(source_id, span.start())
            .map_err(|error| LlvmAdapterError::Debug(error.to_string()))?;
        let line = u32::try_from(position.line())
            .map_err(|_| LlvmAdapterError::Debug("DWARF line 超过 u32".to_owned()))?;
        let column = u32::try_from(position.column())
            .map_err(|_| LlvmAdapterError::Debug("DWARF column 超过 u32".to_owned()))?;
        Ok(DebugPosition {
            source_id,
            source_name,
            line,
            column,
        })
    }
}
