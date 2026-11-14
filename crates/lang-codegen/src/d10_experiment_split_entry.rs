//! Source-qualified split publication. No default driver routing or capability conversion.
use super::*;
use lang_frontend::ownership_checking::d10_catalog_experiment::{
    BoundSplitNamedEntry, NamedEntryOrder, PreparedSplitNamedEntry,
};

/// Immutable original full-Unit graph, published only after independent cross-source replay.
/// This is not a normal driver capability and cannot be constructed from function-local facts.
pub struct SplitNamedEntryProgram<'view, 'parsed: 'view> {
    unit: OwnedCompilationUnitView<'view, 'parsed>,
    bound: &'view BoundSplitNamedEntry,
    program: Program,
    entry: FunctionId,
}
impl SplitNamedEntryProgram<'_, '_> {
    /// Original source wrapper, not a fabricated process main.
    #[must_use]
    pub fn callable(&self) -> UnitItemId {
        self.bound.entry()
    }
    /// Restricted source replacement order.
    #[must_use]
    pub fn order(&self) -> NamedEntryOrder {
        self.bound.order()
    }
    /// All original inputs remain attached to this publication.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.unit.inputs().len()
    }
    /// Verify LLVM from the unchanged complete graph; no executable entry glue.
    pub fn verified_llvm_ir(&self) -> Result<String, Error> {
        self.recheck()?;
        crate::llvm::render_verified_program(&self.program)
            .map_err(|e| Error::new(Stage::Backend, format!("{e:?}")))
    }
    /// Verified host relocatable library object only. No process main, stable ABI, disk write,
    /// linking or execution is provided. Failure returns no bytes.
    pub fn verified_library_object_bytes(&self) -> Result<Vec<u8>, Error> {
        self.recheck()?;
        crate::llvm::verified_library_object_bytes(&self.program)
            .map_err(|e| Error::new(Stage::Backend, format!("{e:?}")))
    }
    fn recheck(&self) -> Result<(), Error> {
        crate::ssa::d10_named_wrapper_graph::check_split(
            &self.unit,
            self.bound,
            &self.program,
            self.entry,
        )
        .map_err(|_| Error::new(Stage::Graph, "unproved split source graph"))
    }
    #[cfg(test)]
    pub(crate) fn replay_input(&self) -> (&Program, FunctionId) {
        (&self.program, self.entry)
    }
}
/// Lower once through ordinary full-Unit gates, then independently check all source functions.
/// A rejected graph never yields a publication; there is no fallback or ownership reanalysis.
pub fn lower_split_named_entry<'view, 'parsed: 'view>(
    unit: OwnedCompilationUnitView<'view, 'parsed>,
    bound: &'view BoundSplitNamedEntry,
) -> Result<SplitNamedEntryProgram<'view, 'parsed>, Error> {
    if !bound.matches(&unit) {
        return Err(Error::new(
            Stage::Identity,
            "split final owned analysis differs",
        ));
    }
    let callable = bound.entry();
    let mut entries = unit
        .names()
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|d| d.source_unit() == callable.source_unit() && d.root() == callable.item());
    let declaration = entries
        .next()
        .ok_or_else(|| Error::new(Stage::Entry, "missing split wrapper"))?;
    if entries.next().is_some() {
        return Err(Error::new(Stage::Entry, "ambiguous split wrapper"));
    }
    let (program, entry) = unit_lower::lower_owned_unit_with_entry(&unit, declaration.id())
        .map_err(|e| Error::new(Stage::Lowering, format!("{e:?}")))?;
    let publication = SplitNamedEntryProgram {
        unit,
        bound,
        program,
        entry,
    };
    publication.recheck()?;
    Ok(publication)
}
/// Borrow the original final-admitted split analysis without reconstructing its identity.
pub fn lower_prepared_split_named_entry<'view, 'parsed: 'view>(
    sources: &'view lang_frontend::source::SourceMap,
    inputs: &'view [lang_frontend::name_resolution::SourceUnitInput<'parsed>],
    names: &'view lang_frontend::name_resolution::ValidatedCompilationUnitNames,
    environment: &'view lang_frontend::type_checking::TypeEnvironment,
    prepared: &'view PreparedSplitNamedEntry,
) -> Result<SplitNamedEntryProgram<'view, 'parsed>, Error> {
    let unit = lang_frontend::ownership_checking::owned_compilation_unit_view(
        sources,
        inputs,
        names,
        environment,
        prepared.typed(),
        prepared.bound().ownership(),
    )
    .map_err(|e| Error::new(Stage::Identity, format!("{e:?}")))?;
    lower_split_named_entry(unit, prepared.bound())
}
