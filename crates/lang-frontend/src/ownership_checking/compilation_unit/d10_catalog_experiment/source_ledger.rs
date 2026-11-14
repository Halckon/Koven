//! Original source traversal batches, not callable ownership or reachability permission.
use super::*;
use crate::name_resolution::SourceUnitId;

/// One actual source traversal's ordered output in four dataflow channels.
/// The source denotes the traversal context, not a proof that every target is local.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceCollectionRecord {
    source: SourceUnitId,
    loans: Vec<UnitLoanFact>,
    receivers: Vec<UnitReceiverOwnershipFact>,
    constructions: Vec<UnitConstructionOwnershipPlan>,
    drops: Vec<UnitDropFact>,
}
impl SourceCollectionRecord {
    pub(crate) fn capture(
        source: SourceUnitId,
        loans: &[UnitLoanFact],
        receivers: &[UnitReceiverOwnershipFact],
        constructions: &[UnitConstructionOwnershipPlan],
        drops: &[UnitDropFact],
    ) -> Self {
        Self {
            source,
            loans: loans.to_vec(),
            receivers: receivers.to_vec(),
            constructions: constructions.to_vec(),
            drops: drops.to_vec(),
        }
    }
    /// Actual canonical source traversed by the original checker/planner.
    #[must_use]
    pub const fn source(&self) -> SourceUnitId {
        self.source
    }
    /// Original ordered loan batch, after local trial rollback.
    #[must_use]
    pub fn loans(&self) -> &[UnitLoanFact] {
        &self.loans
    }
    /// Original receiver batch.
    #[must_use]
    pub fn receivers(&self) -> &[UnitReceiverOwnershipFact] {
        &self.receivers
    }
    /// Original construction batch.
    #[must_use]
    pub fn constructions(&self) -> &[UnitConstructionOwnershipPlan] {
        &self.constructions
    }
    /// Original Drop planner batch; does not certify callable or CFG-point ownership.
    #[must_use]
    pub fn drops(&self) -> &[UnitDropFact] {
        &self.drops
    }
}

/// Sealed original-run source batches. Only four channels are accounted for;
/// contracts, captures and all other channels still require independent accounting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceCollectionLedger {
    analysis: Arc<()>,
    records: Vec<SourceCollectionRecord>,
}
impl SourceCollectionLedger {
    pub(crate) fn new(analysis: Arc<()>, records: Vec<SourceCollectionRecord>) -> Self {
        Self { analysis, records }
    }
    /// Original source visitation order, including sources producing no facts.
    #[must_use]
    pub fn records(&self) -> &[SourceCollectionRecord] {
        &self.records
    }
    /// Full, ordered equality with the same final ownership run. Not a lowering gate.
    #[must_use]
    pub fn matches(&self, owned: &ValidatedCompilationUnitOwnership) -> bool {
        let owned = owned.ownership();
        Arc::ptr_eq(&self.analysis, &owned.provenance.analysis_owner)
            && self.records.iter().flat_map(|r| &r.loans).eq(owned.loans())
            && self
                .records
                .iter()
                .flat_map(|r| &r.receivers)
                .eq(owned.receiver_facts())
            && self
                .records
                .iter()
                .flat_map(|r| &r.constructions)
                .eq(owned.construction_plans())
            && self.records.iter().flat_map(|r| &r.drops).eq(owned.drops())
    }
}
/// Returns the original-run ledger only after final validation and full ordered matching.
/// No reanalysis, source filtering or conversion into a project capability occurs here.
#[must_use]
pub fn source_collection_ledger(
    owned: &ValidatedCompilationUnitOwnership,
) -> Option<&SourceCollectionLedger> {
    let ledger = owned.ownership().source_collection_ledger.as_ref()?;
    ledger.matches(owned).then_some(ledger)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lexer::lex,
        name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
        parser::parse_file,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    #[test]
    fn original_success_and_language_failure_have_distinct_ledger_publication() {
        for (text, success, error) in [
            ("fun run() { val a = \"x\"; println(a) }", true, false),
            (
                "fun run() { val a = \"x\"; val b = a; println(a); println(b) }",
                false,
                true,
            ),
            (
                "class Resource {}\nclass Holder(var payload: Resource)\nfun run(holders: List<Holder>): Unit { val projected = holders[0].payload }",
                false,
                false,
            ),
        ] {
            let mut sources = SourceMap::new();
            let source = sources.add_source("ledger.ko", text).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
            let inputs = [SourceUnitInput::new("root", "ledger.ko", source, &parsed)];
            let (environment, types) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let owned = analysis::analyze(
                &sources,
                &inputs,
                names.names(),
                &types,
                typed.types(),
                false,
            )
            .unwrap();
            assert_eq!(!owned.diagnostics().is_empty(), error);
            assert_eq!(!owned.deferred().is_empty(), !success && !error);
            assert_eq!(owned.source_collection_ledger.is_some(), success);
            if success {
                let owned = owned.validate().unwrap();
                let ledger = source_collection_ledger(&owned).unwrap();
                assert_eq!(ledger.records().len(), 1);
                assert_eq!(ledger.records()[0].loans().len(), 1);
                assert!(!ledger.records()[0].drops().is_empty());
                assert!(ledger.matches(&owned.clone()));
            } else {
                assert!(owned.validate().is_err());
            }
        }
    }
}
