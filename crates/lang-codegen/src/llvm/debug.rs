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

use super::LlvmAdapterError;

pub(super) struct DebugPlan<'sources> {
    sources: &'sources SourceMap,
    primary_source: String,
    source_ids: BTreeMap<String, SourceId>,
}

impl<'sources> DebugPlan<'sources> {
    pub(super) fn build(
        sources: &'sources SourceMap,
        module: &Module,
        native_entry: FunctionId,
    ) -> Result<Self, LlvmAdapterError> {
        let entry = module.function(native_entry).ok_or_else(|| {
            LlvmAdapterError::Debug("debug native entry function 不存在".to_owned())
        })?;
        let primary_source = resolve_position(sources, &entry.origin)?.source_name;
        let mut plan = Self {
            sources,
            primary_source,
            source_ids: BTreeMap::new(),
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
                        LlvmAdapterError::Debug("debug preflight 遇到未知 instruction".to_owned())
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
        self.source_ids
            .entry(position.source_name)
            .or_insert(position.source_id);
        Ok(())
    }

    fn position(&self, origin: &Origin) -> Result<DebugPosition, LlvmAdapterError> {
        resolve_position(self.sources, origin)
    }

    #[cfg(test)]
    pub(super) fn ordered_source_names(&self) -> Vec<&str> {
        self.source_ids.keys().map(String::as_str).collect()
    }
}

pub(super) struct DebugEmitter<'ctx, 'sources> {
    builder: DebugInfoBuilder<'ctx>,
    files: BTreeMap<String, DIFile<'ctx>>,
    subprograms: BTreeMap<FunctionId, DISubprogram<'ctx>>,
    plan: DebugPlan<'sources>,
}

impl<'ctx, 'sources> DebugEmitter<'ctx, 'sources> {
    pub(super) fn new(
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
            &plan.primary_source,
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
        for source_name in plan.source_ids.keys() {
            if source_name != &plan.primary_source {
                files.insert(source_name.clone(), builder.create_file(source_name, ""));
            }
        }
        Self {
            builder,
            files,
            subprograms: BTreeMap::new(),
            plan,
        }
    }

    pub(super) fn attach_function(
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

    pub(super) fn set_location(
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

    pub(super) fn finalize(&self) {
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
