//! Independent two-source same-run handoff; not an old named or codegen capability.
use super::*;

mod projection;
use crate::name_resolution::{DeclarationId, SourceUnitId};
use crate::ownership_checking::{
    UnitCallArgumentOwnershipContract, UnitDirectCallLoanEndFact, UnitLoanFact,
};
pub use projection::SplitNamedCallee;
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SplitWrapperPending {
    pub(crate) typed: Arc<()>,
    pub(crate) analysis_owner: Arc<()>,
    pub(crate) entry: UnitItemId,
    pub(crate) callee: UnitItemId,
    pub(crate) target: DeclarationId,
    pub(crate) roots: [UnitSymbolId; 2],
    pub(crate) initializers: [UnitExpressionId; 2],
    pub(crate) call: UnitExpressionId,
    pub(crate) arguments: [UnitExpressionId; 2],
    pub(crate) loans: [UnitLoanFact; 2],
    pub(crate) ends: [UnitDirectCallLoanEndFact; 2],
    pub(crate) contracts: Vec<UnitCallArgumentOwnershipContract>,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SplitSourcePending {
    Wrapper(SplitWrapperPending),
    Before(DualNestedPending, Vec<UnitCallArgumentOwnershipContract>),
    After(AfterChildPending, Vec<UnitCallArgumentOwnershipContract>),
}
impl SplitSourcePending {
    pub(crate) fn source(&self) -> SourceUnitId {
        match self {
            Self::Wrapper(p) => p.entry.source_unit(),
            Self::Before(p, _) => p.callable.source_unit(),
            Self::After(p, _) => p.callable.source_unit(),
        }
    }
}
enum Callee {
    Before(DualNestedPending),
    After(AfterChildPending),
}
/// Complete two-source ownership observation from one real analysis. No public constructor,
/// conversion to an old single-source capability, or executable publication authority.
pub struct BoundSplitNamedEntry {
    owned: ValidatedCompilationUnitOwnership,
    wrapper: SplitWrapperPending,
    callee: Callee,
}
impl BoundSplitNamedEntry {
    /// Original complete final ownership analysis.
    #[must_use]
    pub const fn ownership(&self) -> &ValidatedCompilationUnitOwnership {
        &self.owned
    }
    /// Actual source-qualified zero-parameter entry.
    #[must_use]
    pub const fn entry(&self) -> UnitItemId {
        self.wrapper.entry
    }
    /// Actual source-qualified callee.
    #[must_use]
    pub const fn callee(&self) -> UnitItemId {
        self.wrapper.callee
    }
    /// Actual source wrapper call, not a synthetic entry bridge.
    #[must_use]
    pub const fn call(&self) -> UnitExpressionId {
        self.wrapper.call
    }
    /// Original distinct named Int argument roots.
    #[must_use]
    pub const fn wrapper_roots(&self) -> [UnitSymbolId; 2] {
        self.wrapper.roots
    }
    /// Actual Int source initializer expressions.
    #[must_use]
    pub const fn initializers(&self) -> [UnitExpressionId; 2] {
        self.wrapper.initializers
    }
    /// Actual wrapper arguments in source argument order.
    #[must_use]
    pub const fn arguments(&self) -> [UnitExpressionId; 2] {
        self.wrapper.arguments
    }
    /// Original wrapper Shared loan registrations, without reordering final ownership facts.
    #[must_use]
    pub const fn loans(&self) -> &[UnitLoanFact; 2] {
        &self.wrapper.loans
    }
    /// Actual wrapper direct-call ends, including their original precedence.
    #[must_use]
    pub const fn ends(&self) -> &[UnitDirectCallLoanEndFact; 2] {
        &self.wrapper.ends
    }
    /// Read-only source-qualified callee facts borrowed from this complete split bound.
    /// This projection is not a standalone lowering or single-source capability.
    #[must_use]
    pub const fn callee_facts(&self) -> SplitNamedCallee<'_> {
        SplitNamedCallee {
            callee: &self.callee,
        }
    }
    /// Actual typed declaration target from the same full Unit.
    #[must_use]
    pub const fn target(&self) -> DeclarationId {
        self.wrapper.target
    }
    /// Replacement traversal order from the original callee observation.
    #[must_use]
    pub const fn order(&self) -> NamedEntryOrder {
        match self.callee {
            Callee::Before(_) => NamedEntryOrder::BeforeChild,
            Callee::After(_) => NamedEntryOrder::AfterChild,
        }
    }
    /// Exact complete final ownership instance.
    #[must_use]
    pub fn matches(&self, unit: &super::super::super::OwnedCompilationUnitView<'_, '_>) -> bool {
        self.owned.ownership().is_same_analysis(unit.ownership())
    }
}
/// Analyze the complete two-source Unit once. Any absent, ambiguous, failed or mismatched
/// observation returns no capability; no fallback, source crop or re-analysis is performed.
pub fn analyze_split_named_entry(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
) -> Result<Option<BoundSplitNamedEntry>, OwnershipCheckingError> {
    let (owned, _, _, _, _, _, _, _, pending) = analysis::analyze_with_catalog(
        sources,
        inputs,
        names.names(),
        environment,
        typed.types(),
        false,
    )?;
    Ok(bind(sources, inputs, names, owned, pending))
}
/// Classify shape discovery separately from a failed unique same-run proof.
pub(super) fn select(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    owned: CompilationUnitOwnership,
    pending: Vec<SplitSourcePending>,
) -> Result<BoundSplitNamedEntry, PrepareNamedEntryError> {
    if inputs.len() != 2
        || names.names().index().source_units().len() != 2
        || inputs.iter().any(|input| input.parsed().roots().len() != 1)
    {
        return Err(PrepareNamedEntryError::Unsupported);
    }
    let wrappers = pending
        .iter()
        .filter(|p| matches!(p, SplitSourcePending::Wrapper(_)))
        .count();
    let callees = pending.len() - wrappers;
    if wrappers == 0 || callees == 0 {
        return Err(PrepareNamedEntryError::Unsupported);
    }
    if wrappers != 1 || callees != 1 {
        return Err(PrepareNamedEntryError::Ambiguous);
    }
    bind(sources, inputs, names, owned, pending).ok_or(PrepareNamedEntryError::Unproved)
}

fn bind(
    _sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    owned: CompilationUnitOwnership,
    mut pending: Vec<SplitSourcePending>,
) -> Option<BoundSplitNamedEntry> {
    let [first_source, second_source] = names.names().index().source_units() else {
        return None;
    };
    if inputs.len() != 2
        || pending.len() != 2
        || pending[0].source() != first_source.id()
        || pending[1].source() != second_source.id()
        || inputs.iter().any(|input| input.parsed().roots().len() != 1)
    {
        return None;
    }
    let second = pending.pop()?;
    let first = pending.pop()?;
    let (wrapper, callee, root_order) = match (first, second) {
        (SplitSourcePending::Wrapper(w), other) => (w, other, NamedRootOrder::EntryFirst),
        (other, SplitSourcePending::Wrapper(w)) => (w, other, NamedRootOrder::CalleeFirst),
        _ => return None,
    };
    let (callee, mut contracts) = match callee {
        SplitSourcePending::Before(p, c) => (Callee::Before(p), c),
        SplitSourcePending::After(p, c) => (Callee::After(p), c),
        SplitSourcePending::Wrapper(_) => return None,
    };
    let (callable, typed, run, fields) = match &callee {
        Callee::Before(p) => (
            p.callable,
            &p.typed,
            &p.analysis_owner,
            super::named_validation::NamedValidation::before(p)?,
        ),
        Callee::After(p) => (
            p.callable,
            &p.typed,
            &p.analysis_owner,
            super::named_validation::NamedValidation::after(p),
        ),
    };
    let target = names
        .names()
        .index()
        .declarations()
        .get(wrapper.target.index())?;
    if wrapper.entry.source_unit() == callable.source_unit()
        || wrapper.callee != callable
        || target.source_unit() != callable.source_unit()
        || target.root() != callable.item()
        || !Arc::ptr_eq(&wrapper.typed, typed)
        || !Arc::ptr_eq(&wrapper.analysis_owner, run)
        || !Arc::ptr_eq(run, &owned.provenance.analysis_owner)
    {
        return None;
    }
    contracts.extend(wrapper.contracts.iter().copied());
    // This is the existing contracts producer's ordering, not planner event or loan ordering.
    // Only sort the independently checked expectation; actual final owned remains untouched.
    contracts.sort_by_key(|c| {
        (
            c.call().source_unit().index(),
            c.call_span().start(),
            c.argument_span().start(),
        )
    });
    if contracts.as_slice() != owned.call_argument_contracts() {
        return None;
    }
    let validated = super::named_validation::WrapperValidation {
        root_order,
        contracts: &contracts,
        roots: wrapper.roots,
        call: wrapper.call,
        arguments: wrapper.arguments,
        loans: &wrapper.loans,
        ends: &wrapper.ends,
    }
    .validate(owned, fields)?;
    Some(BoundSplitNamedEntry {
        owned: validated,
        wrapper,
        callee,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "CI-only cross-analysis split-source control"]
    fn ci_only_split_rejects_same_typed_foreign_run() {
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        use crate::{
            lexer::lex,
            name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
            parser::parse_file,
            type_checking::{check_compilation_unit_types, standard_environments},
        };
        let before = r#"fun run(count: Int, childCount: Int) { var a = "first"; var b = "kept"; var outer = count; while(outer > 0) { val parentOld = replace(&a, "parent"); var inner = childCount; while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }; outer -= 1 }; println(a); println(b) }"#;
        let after = before
            .replace("val parentOld = replace(&a, \"parent\"); ", "")
            .replace(
                "}; outer -= 1",
                "}; val parentOld = replace(&a, \"parent\"); outer -= 1",
            );
        for callee in [before, after.as_str()] {
            for wrapper_first in [false, true] {
                let texts = if wrapper_first {
                    [
                        ("a.ko", "fun entry() { val p = 1; val c = 2; run(p,c) }"),
                        ("z.ko", callee),
                    ]
                } else {
                    [
                        ("z.ko", "fun entry() { val p = 1; val c = 2; run(p,c) }"),
                        ("a.ko", callee),
                    ]
                };
                let mut sources = SourceMap::new();
                let parsed: Vec<_> = texts
                    .iter()
                    .map(|(path, text)| {
                        let source = sources.add_source(*path, *text).unwrap();
                        parse_file(&sources, &lex(&sources, source).unwrap()).unwrap()
                    })
                    .collect();
                let inputs: Vec<_> = texts
                    .iter()
                    .zip(&parsed)
                    .map(|((path, _), p)| SourceUnitInput::new("root", path, p.source_id(), p))
                    .collect();
                let (env, types) = standard_environments();
                let index = index_compilation_unit(&sources, &inputs).unwrap();
                let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
                    .unwrap()
                    .validate()
                    .unwrap();
                let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                    .unwrap()
                    .validate()
                    .unwrap();
                let analyze = || {
                    analysis::analyze_with_catalog(
                        &sources,
                        &inputs,
                        names.names(),
                        &types,
                        typed.types(),
                        false,
                    )
                    .unwrap()
                };
                let (own, _, _, _, _, _, _, _, pending) = analyze();
                let (other, _, _, _, _, _, _, _, _) = analyze();
                assert!(bind(&sources, &inputs, &names, own, pending).is_some());
                let (_, _, _, _, _, _, _, _, foreign) = analyze();
                assert!(matches!(
                    select(&sources, &inputs, &names, other, foreign),
                    Err(PrepareNamedEntryError::Unproved)
                ));
                let (own, _, _, _, _, _, _, _, mut original) = analyze();
                let (_, _, _, _, _, _, _, _, mut other) = analyze();
                original[0] = other.remove(0);
                assert!(matches!(
                    select(&sources, &inputs, &names, own, original),
                    Err(PrepareNamedEntryError::Unproved)
                ));
            }
        }
    }
}
