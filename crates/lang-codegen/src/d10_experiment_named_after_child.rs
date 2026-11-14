//! Separate child-then-parent publication; never interchangeable with BeforeChild.
use super::*;
/// Immutable two-function publication, borrowing the same final named-wrapper analysis.
/// Its explicit process-object method revalidates the real zero-argument source entry.
/// No mutable/consuming Program escape, linking, or execution is exposed.
pub struct NamedAfterChildProgram<'view, 'parsed: 'view> {
    unit: OwnedCompilationUnitView<'view, 'parsed>,
    bound: &'view lang_frontend::ownership_checking::d10_catalog_experiment::BoundNamedAfterChildWrapper,
    program: Program,
    entry: FunctionId,
}
impl NamedAfterChildProgram<'_, '_> {
    /// Atomically publish a relocatable process object with the existing LLVM main glue.
    ///
    /// Rechecks the same source declaration, zero-argument Unit signature and immutable graph
    /// before reserving output. Emission uses a sibling temporary and commits only on success;
    /// an existing target is not truncated in advance. This never links or executes native code.
    /// The ordinary native entry gates and the library-object method remain unchanged.
    pub fn emit_process_object(
        &self,
        output: &std::path::Path,
    ) -> Result<(), crate::NativeObjectError> {
        crate::native::d10_named_after_child::emit(self, output)
    }

    pub(crate) fn process_source(
        &self,
    ) -> (
        &OwnedCompilationUnitView<'_, '_>,
        &lang_frontend::ownership_checking::d10_catalog_experiment::BoundNamedAfterChildWrapper,
    ) {
        (&self.unit, self.bound)
    }

    pub(crate) fn process_graph(&self) -> (&Program, FunctionId) {
        (&self.program, self.entry)
    }

    /// Original wrapper declaration, not a synthesized process entry.
    #[must_use]
    pub fn callable(&self) -> UnitItemId {
        self.bound.entry()
    }
    /// Number of original source inputs kept alive by this publication.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.unit.inputs().len()
    }
    /// Verify immutable SSA and LLVM; no entry glue, filesystem write, linking or execution.
    pub fn verified_llvm_ir(&self) -> Result<String, Error> {
        crate::llvm::render_verified_program(&self.program)
            .map_err(|e| Error::new(Stage::Backend, format!("{e:?}")))
    }
    /// Emit a verified host relocatable library object in memory, with both original functions.
    /// No main/debug glue or stable external ABI is promised. Failure returns no bytes.
    pub fn verified_library_object_bytes(&self) -> Result<Vec<u8>, Error> {
        crate::llvm::verified_library_object_bytes(&self.program)
            .map_err(|e| Error::new(Stage::Backend, format!("{e:?}")))
    }
    /// Verified wrapper name in the immutable Program.
    #[must_use]
    pub fn function_name(&self) -> Option<&str> {
        self.program
            .module(self.entry.module())?
            .function(self.entry)
            .map(|f| f.name.as_str())
    }
    #[cfg(test)]
    pub(crate) fn replay_input(&self) -> (&Program, FunctionId) {
        (&self.program, self.entry)
    }
}

/// Lower the actual named wrapper and its reachable callee once through ordinary Unit gates.
/// The independent complete-Program graph check must succeed before any artifact is published.
pub fn lower_named_after_child_wrapper<'view, 'parsed: 'view>(
    unit: OwnedCompilationUnitView<'view, 'parsed>,
    bound: &'view lang_frontend::ownership_checking::d10_catalog_experiment::BoundNamedAfterChildWrapper,
) -> Result<NamedAfterChildProgram<'view, 'parsed>, Error> {
    if !bound.matches(&unit) {
        return Err(Error::new(Stage::Identity, "final owned analysis differs"));
    }
    let callable = bound.entry();
    let mut entries = unit
        .names()
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|d| d.source_unit() == callable.source_unit() && d.root() == callable.item());
    let entry = entries
        .next()
        .ok_or_else(|| Error::new(Stage::Entry, "missing wrapper"))?;
    if entries.next().is_some() {
        return Err(Error::new(Stage::Entry, "ambiguous wrapper"));
    }
    let (program, function) = unit_lower::lower_owned_unit_with_entry(&unit, entry.id())
        .map_err(|e| Error::new(Stage::Lowering, format!("{e:?}")))?;
    crate::ssa::d10_named_wrapper_graph::check_after_child(&unit, bound, &program, function)
        .map_err(|_| {
            Error::new(
                Stage::Graph,
                "unproved wrapper/callee graph or source relation",
            )
        })?;
    Ok(NamedAfterChildProgram {
        unit,
        bound,
        program,
        entry: function,
    })
}
