//! 拥有完整 const 分析链及两条只读交接适配；行为断言留在 contract tests。

use std::path::Path;

use lang_frontend::{
    name_resolution::{
        DeclarationId, SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        ConstEnabledOwnedUnit, ConstOwnedCompilationUnitView, ConstantMaterializationKind,
        OwnedCompilationUnitViewError, UnitShortCircuitRhs,
        check_compilation_unit_constant_ownership, const_owned_compilation_unit_view,
    },
    parser::ParsedFile,
    source::{SourceId, SourceMap},
    type_checking::{
        ConstEnabledTypedUnit, ConstValue, TypeEnvironment, check_compilation_unit_types,
        standard_environments,
    },
};

use super::super::super::parsed;
use crate::{
    NativeObjectError, NativeUnitEntry, emit_native_const_owned_unit_object,
    emit_native_constant_unit_object,
    ssa::{
        LoweringError,
        model::{FunctionId, Program},
        render_program,
        unit_lower::constant::{lower_const_owned_unit_with_entry, lower_constant_unit_with_entry},
        verify::verify_program,
    },
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Pathway {
    Legacy,
    View,
}

pub(super) const PATHWAYS: [Pathway; 2] = [Pathway::Legacy, Pathway::View];

pub(super) struct ConstAnalysis {
    pub(super) sources: SourceMap,
    pub(super) provider_source: SourceId,
    pub(super) provider: ParsedFile,
    pub(super) consumer_source: SourceId,
    pub(super) consumer: ParsedFile,
    pub(super) names: ValidatedCompilationUnitNames,
    pub(super) environment: TypeEnvironment,
    pub(super) typed: ConstEnabledTypedUnit,
    pub(super) owned: ConstEnabledOwnedUnit,
}

impl ConstAnalysis {
    pub(super) fn inputs(&self) -> [SourceUnitInput<'_>; 2] {
        [
            SourceUnitInput::new(
                "root",
                "p/provider.ko",
                self.provider_source,
                &self.provider,
            ),
            SourceUnitInput::new(
                "root",
                "q/consumer.ko",
                self.consumer_source,
                &self.consumer,
            ),
        ]
    }

    pub(super) fn declaration(&self, name: &str) -> DeclarationId {
        self.names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == name)
            .expect("fixture declaration exists")
            .id()
    }

    pub(super) fn entries(&self) -> [NativeUnitEntry; 2] {
        [
            self.declaration("entry").into(),
            self.declaration("invalid").into(),
        ]
    }
}

#[derive(Clone, Copy)]
pub(super) struct Handoff<'a> {
    pub(super) sources: &'a SourceMap,
    pub(super) inputs: &'a [SourceUnitInput<'a>],
    pub(super) names: &'a ValidatedCompilationUnitNames,
    pub(super) environment: &'a TypeEnvironment,
    pub(super) typed: &'a ConstEnabledTypedUnit,
    pub(super) owned: &'a ConstEnabledOwnedUnit,
}

impl<'a> Handoff<'a> {
    pub(super) fn new(analysis: &'a ConstAnalysis, inputs: &'a [SourceUnitInput<'a>]) -> Self {
        Self {
            sources: &analysis.sources,
            inputs,
            names: &analysis.names,
            environment: &analysis.environment,
            typed: &analysis.typed,
            owned: &analysis.owned,
        }
    }

    pub(super) fn view(
        self,
    ) -> Result<ConstOwnedCompilationUnitView<'a, 'a>, OwnedCompilationUnitViewError> {
        const_owned_compilation_unit_view(
            self.sources,
            self.inputs,
            self.names,
            self.environment,
            self.typed,
            self.owned,
        )
    }

    pub(super) fn emit(
        self,
        pathway: Pathway,
        entry: NativeUnitEntry,
        output: &Path,
    ) -> Result<(), NativeObjectError> {
        match pathway {
            Pathway::Legacy => self.emit_legacy(entry, output),
            Pathway::View => {
                let view = self.view().map_err(NativeObjectError::from)?;
                emit_native_const_owned_unit_object(&view, entry, output)
            }
        }
    }

    pub(super) fn emit_legacy(
        self,
        entry: impl Into<NativeUnitEntry>,
        output: &Path,
    ) -> Result<(), NativeObjectError> {
        emit_native_constant_unit_object(
            self.sources,
            self.inputs,
            self.names,
            self.environment,
            self.typed,
            self.owned,
            entry,
            output,
        )
    }

    pub(super) fn lower_legacy(
        self,
        entry: NativeUnitEntry,
    ) -> Result<(Program, FunctionId), LoweringError> {
        lower_constant_unit_with_entry(
            self.sources,
            self.inputs,
            self.names,
            self.environment,
            self.typed,
            self.owned,
            entry.declaration(),
        )
    }

    pub(super) fn verified_ssa(self, pathway: Pathway, entry: NativeUnitEntry) -> String {
        let (program, function) = match pathway {
            Pathway::Legacy => self.lower_legacy(entry),
            Pathway::View => {
                let view = self
                    .view()
                    .expect("matching const handoff constructs a view");
                assert!(std::ptr::eq(view.sources(), self.sources));
                assert!(std::ptr::eq(view.inputs(), self.inputs));
                assert!(std::ptr::eq(view.names(), self.names));
                assert!(std::ptr::eq(view.types(), self.typed.types()));
                assert!(std::ptr::eq(view.ownership(), self.owned.ownership()));
                assert!(std::ptr::eq(view.constant_ownership(), self.owned));
                assert_eq!(
                    view.constant_ownership().materializations(),
                    self.owned.materializations()
                );
                assert_eq!(
                    view.constant_ownership().short_circuits(),
                    self.owned.short_circuits()
                );
                lower_const_owned_unit_with_entry(&view, entry.declaration())
            }
        }
        .expect("matching const handoff lowers");
        verify_program(&program).expect("lowered const program verifies");
        format!("{function:?}\n{}", render_program(&program))
    }
}

pub(super) fn fixture() -> ConstAnalysis {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nconst val TEXT = \"handoff-界\"\nconst val FLAG = true",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun probe(text: String): Boolean { println(text)\nreturn true }\n\
         fun entry(): Unit {\n\
             if (p.FLAG || probe(p.TEXT)) { println(p.TEXT) }\n\
             if (p.FLAG && probe(p.TEXT)) {}\n\
         }\n\
         fun invalid(number: Int): Unit {}",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("valid input index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &environment, &typed)
            .unwrap()
            .validate()
            .unwrap();
    assert_eq!(typed.constants().declarations().len(), 2);
    assert_eq!(typed.constants().uses().len(), 5);
    assert_eq!(owned.materializations().len(), 4);
    assert_eq!(
        owned
            .materializations()
            .iter()
            .filter(|plan| {
                plan.kind() == ConstantMaterializationKind::StringTemporary
                    && matches!(plan.descriptor().value(), ConstValue::String(_))
            })
            .count(),
        2
    );
    assert_eq!(
        owned
            .short_circuits()
            .iter()
            .map(|plan| plan.rhs())
            .collect::<Vec<_>>(),
        [UnitShortCircuitRhs::Never, UnitShortCircuitRhs::Always]
    );
    ConstAnalysis {
        sources,
        provider_source,
        provider,
        consumer_source,
        consumer,
        names,
        environment,
        typed,
        owned,
    }
}
