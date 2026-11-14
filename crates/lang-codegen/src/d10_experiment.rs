//! Default-off, same-analysis ordered loop publications. Not ordinary entry/Carry capabilities.
//!
//! This route uses ordinary Unit lowering unchanged and publishes only after a separate graph
//! check. It does not select source programs for the normal driver or create native entry glue.
use lang_frontend::{
    ownership_checking::{
        OwnedCompilationUnitView, d10_catalog_experiment::BoundAfterChildNestedLoop,
    },
    type_checking::UnitItemId,
};

use crate::ssa::{
    model::{FunctionId, Program},
    unit_lower,
};

/// The stage that refused a restricted ordered-loop experimental publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// The view does not borrow the bound's exact final ownership analysis.
    Identity,
    /// There is no unique original callable declaration.
    Entry,
    /// Ordinary Unit planning/lowering rejected the source; its gates are not bypassed.
    Lowering,
    /// Actual SSA did not prove the complete restricted graph contract.
    Graph,
    /// LLVM verification/rendering failed; no text was returned.
    Backend,
}

/// An experimental error, without publication of a partial Program or backend text.
#[derive(Debug)]
pub struct Error {
    stage: Stage,
    detail: String,
}
impl Error {
    /// Returns the exact failed stage.
    #[must_use]
    pub const fn stage(&self) -> Stage {
        self.stage
    }
    fn new(stage: Stage, detail: impl Into<String>) -> Self {
        Self {
            stage,
            detail: detail.into(),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "D10 ordered-loop {:?}: {}", self.stage, self.detail)
    }
}
impl std::error::Error for Error {}

/// Private construction and no mutable/consuming SSA escape.
///
/// The source view and sealed bound remain borrowed for the entire publication lifetime.
/// LLVM text is a detached rendering, not another checked Program or native entry capability.
pub struct AfterChildProgram<'view, 'parsed: 'view> {
    unit: OwnedCompilationUnitView<'view, 'parsed>,
    bound: &'view BoundAfterChildNestedLoop,
    program: Program,
    entry: FunctionId,
}
impl AfterChildProgram<'_, '_> {
    /// Original source-qualified callable identity, not a function name guess.
    #[must_use]
    pub fn callable(&self) -> UnitItemId {
        self.bound.controls().0
    }

    /// Render and verify the actual immutable SSA using the existing LLVM adapter.
    /// Does not append an executable entry, write an object, or execute native code.
    pub fn verified_llvm_ir(&self) -> Result<String, Error> {
        crate::llvm::render_verified_program(&self.program)
            .map_err(|e| Error::new(Stage::Backend, format!("{e:?}")))
    }

    /// The artifact keeps the same source/owned view alive; no replacement analysis is accepted.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.unit.inputs().len()
    }

    /// Verified original callable name in this immutable Program.
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

/// Lower through the existing ordinary gate, then check the complete actual restricted graph.
/// No caller-selected entry, existing Program, or prior check result can be supplied.
pub fn lower_after_child<'view, 'parsed: 'view>(
    unit: OwnedCompilationUnitView<'view, 'parsed>,
    bound: &'view BoundAfterChildNestedLoop,
) -> Result<AfterChildProgram<'view, 'parsed>, Error> {
    if !bound.matches(&unit) {
        return Err(Error::new(Stage::Identity, "final owned analysis differs"));
    }
    let callable = bound.controls().0;
    let mut entries = unit
        .names()
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|d| d.source_unit() == callable.source_unit() && d.root() == callable.item());
    let entry = entries
        .next()
        .ok_or_else(|| Error::new(Stage::Entry, "missing callable"))?;
    if entries.next().is_some() {
        return Err(Error::new(Stage::Entry, "ambiguous callable"));
    }
    let (program, function) = unit_lower::lower_owned_unit_with_entry(&unit, entry.id())
        .map_err(|e| Error::new(Stage::Lowering, format!("{e:?}")))?;
    crate::ssa::d10_after_child_graph::check(&unit, bound, &program, function)
        .map_err(|_| Error::new(Stage::Graph, "unproved graph or source relation"))?;
    Ok(AfterChildProgram {
        unit,
        bound,
        program,
        entry: function,
    })
}

/// Separate parent-before-child publication. It does not inherit AfterChild authority.
/// Only two independent counters and real final uses of both roots are accepted.
pub struct BeforeChildProgram<'view, 'parsed: 'view> {
    unit: OwnedCompilationUnitView<'view, 'parsed>,
    bound: &'view lang_frontend::ownership_checking::d10_catalog_experiment::BoundDualNestedLoop,
    program: Program,
    entry: FunctionId,
}
impl BeforeChildProgram<'_, '_> {
    /// Original source-qualified callable identity.
    #[must_use]
    pub fn callable(&self) -> UnitItemId {
        self.bound.controls().0
    }

    /// Verify and render only; no native entry glue, file publication or execution.
    pub fn verified_llvm_ir(&self) -> Result<String, Error> {
        crate::llvm::render_verified_program(&self.program)
            .map_err(|e| Error::new(Stage::Backend, format!("{e:?}")))
    }

    /// Number of source inputs held alive by this publication.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.unit.inputs().len()
    }

    /// Original callable name from this immutable graph.
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

/// Preserve ordinary lowering; require the independent parent-before-child graph proof.
pub fn lower_before_child<'view, 'parsed: 'view>(
    unit: OwnedCompilationUnitView<'view, 'parsed>,
    bound: &'view lang_frontend::ownership_checking::d10_catalog_experiment::BoundDualNestedLoop,
) -> Result<BeforeChildProgram<'view, 'parsed>, Error> {
    if !bound.matches(&unit) {
        return Err(Error::new(Stage::Identity, "final owned analysis differs"));
    }
    let [parent, child] = bound.counter_parameters();
    if parent == child || bound.final_root_use().is_none() {
        return Err(Error::new(
            Stage::Graph,
            "two independent counters and two final uses required",
        ));
    }
    let callable = bound.controls().0;
    let mut entries = unit
        .names()
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|d| d.source_unit() == callable.source_unit() && d.root() == callable.item());
    let entry = entries
        .next()
        .ok_or_else(|| Error::new(Stage::Entry, "missing callable"))?;
    if entries.next().is_some() {
        return Err(Error::new(Stage::Entry, "ambiguous callable"));
    }
    let (program, function) = unit_lower::lower_owned_unit_with_entry(&unit, entry.id())
        .map_err(|e| Error::new(Stage::Lowering, format!("{e:?}")))?;
    crate::ssa::d10_before_child_graph::check(&unit, bound, &program, function)
        .map_err(|_| Error::new(Stage::Graph, "unproved graph or source relation"))?;
    Ok(BeforeChildProgram {
        unit,
        bound,
        program,
        entry: function,
    })
}
