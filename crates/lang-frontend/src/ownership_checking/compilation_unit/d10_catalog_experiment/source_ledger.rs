//! Original source traversal batches, not callable ownership or reachability permission.
use super::*;
use crate::name_resolution::SourceUnitId;

/// One real planner invocation boundary, not a callable or CFG certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropTraversal {
    /// Original parsed root; a classifier/companion root may contain several members.
    SourceRoot(UnitItemId),
    /// Visited lambda planned in the original second pass.
    Lambda(UnitExpressionId),
}
impl DropTraversal {
    /// Source traversed by the planner, not necessarily every target's declaration source.
    #[must_use]
    pub const fn source(self) -> SourceUnitId {
        match self {
            Self::SourceRoot(root) => root.source_unit(),
            Self::Lambda(lambda) => lambda.source_unit(),
        }
    }
}
/// Newly appended unconditional flat drops after one successful traversal, including empty.
/// The planner normalizes and globally deduplicates before append: this is not a record of
/// every proposed drop, independent per-root output, conditional cleanup, or execution order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropTraversalRecord {
    traversal: DropTraversal,
    drops: Vec<UnitDropFact>,
}
impl DropTraversalRecord {
    pub(crate) fn capture(traversal: DropTraversal, drops: Vec<UnitDropFact>) -> Self {
        Self { traversal, drops }
    }
    /// Actual source-root or visited-lambda invocation.
    #[must_use]
    pub const fn traversal(&self) -> DropTraversal {
        self.traversal
    }
    /// Exact normalized appended facts, retaining point, target and value origin.
    #[must_use]
    pub fn drops(&self) -> &[UnitDropFact] {
        &self.drops
    }
}

/// One actual source traversal's ordered output in five dataflow channels.
/// The source denotes the traversal context, not a proof that every target is local.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceCollectionRecord {
    source: SourceUnitId,
    loans: Vec<UnitLoanFact>,
    receivers: Vec<UnitReceiverOwnershipFact>,
    constructions: Vec<UnitConstructionOwnershipPlan>,
    drops: Vec<UnitDropFact>,
    drop_traversals: Vec<DropTraversalRecord>,
    conditional_receiver_drops: Vec<UnitConditionalReceiverDropFact>,
}
impl SourceCollectionRecord {
    pub(crate) fn capture(
        source: SourceUnitId,
        loans: &[UnitLoanFact],
        receivers: &[UnitReceiverOwnershipFact],
        constructions: &[UnitConstructionOwnershipPlan],
        drops: &[UnitDropFact],
        drop_traversals: Vec<DropTraversalRecord>,
        conditional_receiver_drops: &[UnitConditionalReceiverDropFact],
    ) -> Self {
        Self {
            source,
            loans: loans.to_vec(),
            receivers: receivers.to_vec(),
            constructions: constructions.to_vec(),
            drops: drops.to_vec(),
            drop_traversals,
            conditional_receiver_drops: conditional_receiver_drops.to_vec(),
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
    /// Complete original conditional receiver obligations, retaining the flat Drop ordinal.
    /// Not executed cleanup: concrete specialization and iteration ordering remain downstream.
    #[must_use]
    pub fn conditional_receiver_drops(&self) -> &[UnitConditionalReceiverDropFact] {
        &self.conditional_receiver_drops
    }
    /// Original planner root invocations followed by visited lambdas, including empty batches.
    /// Not callable attribution or proof of the complete reachable closure.
    #[must_use]
    pub fn drop_traversals(&self) -> &[DropTraversalRecord] {
        &self.drop_traversals
    }
    /// Original Drop planner batch; does not certify callable or CFG-point ownership.
    #[must_use]
    pub fn drops(&self) -> &[UnitDropFact] {
        &self.drops
    }
}

/// Sealed original-run source batches and complete static call contracts.
/// Final filtered closure facts are sealed only at final ownership construction.
/// Remaining channels still require independent accounting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceCollectionLedger {
    analysis: Arc<()>,
    records: Vec<SourceCollectionRecord>,
    arguments: Vec<UnitCallArgumentOwnershipContract>,
    receivers: Vec<UnitCallReceiverOwnershipContract>,
    final_closures: Option<FinalClosures>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct FinalClosures {
    captures: Vec<UnitClosureCaptureDescriptor>,
    closures: Vec<UnitClosureDescriptor>,
}
impl SourceCollectionLedger {
    pub(crate) fn new(
        analysis: Arc<()>,
        records: Vec<SourceCollectionRecord>,
        arguments: &[UnitCallArgumentOwnershipContract],
        receivers: &[UnitCallReceiverOwnershipContract],
    ) -> Self {
        Self {
            analysis,
            records,
            arguments: arguments.to_vec(),
            receivers: receivers.to_vec(),
            final_closures: None,
        }
    }
    pub(in crate::ownership_checking::compilation_unit) fn with_final_closures(
        mut self,
        captures: &[UnitClosureCaptureDescriptor],
        closures: &[UnitClosureDescriptor],
    ) -> Self {
        self.final_closures = Some(FinalClosures {
            captures: captures.to_vec(),
            closures: closures.to_vec(),
        });
        self
    }
    /// Exact final filtered capture/closure arrays, never the pre-filter candidate table.
    /// None denotes an unsealed internal observation; public validated retrieval rejects it.
    #[must_use]
    pub fn closure_facts(
        &self,
    ) -> Option<(&[UnitClosureCaptureDescriptor], &[UnitClosureDescriptor])> {
        self.final_closures
            .as_ref()
            .map(|facts| (facts.captures.as_slice(), facts.closures.as_slice()))
    }
    /// Complete original static argument contracts in collector order.
    /// These may describe calls never visited by dataflow; not execution observations.
    #[must_use]
    pub fn argument_contracts(&self) -> &[UnitCallArgumentOwnershipContract] {
        &self.arguments
    }
    /// Complete original static receiver contracts, not receiver execution facts.
    #[must_use]
    pub fn receiver_contracts(&self) -> &[UnitCallReceiverOwnershipContract] {
        &self.receivers
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
        let Some(closures) = &self.final_closures else {
            return false;
        };
        Arc::ptr_eq(&self.analysis, &owned.provenance.analysis_owner)
            && closures.captures == owned.captures()
            && closures.closures == owned.closures()
            && closures
                .captures
                .iter()
                .map(|c| c.lambda().source_unit())
                .chain(
                    closures
                        .closures
                        .iter()
                        .map(|c| c.expression().source_unit()),
                )
                .all(|source| self.records.iter().any(|record| record.source == source))
            && self.arguments == owned.call_argument_contracts()
            && self.receivers == owned.call_receiver_contracts()
            && self
                .arguments
                .iter()
                .map(|c| c.call().source_unit())
                .chain(self.receivers.iter().map(|c| c.call().source_unit()))
                .all(|source| self.records.iter().any(|record| record.source == source))
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
            && self.records.iter().all(|record| {
                record.drop_traversals.iter().all(|batch| {
                    batch.traversal.source() == record.source
                        && batch.drops.iter().all(|fact| {
                            super::super::owner_transport_validation::point_source(fact.point())
                                == record.source
                        })
                }) && record
                    .drop_traversals
                    .iter()
                    .flat_map(|batch| &batch.drops)
                    .eq(&record.drops)
            })
            && self.records.iter().flat_map(|r| &r.drops).eq(owned.drops())
            && self.records.iter().all(|record| {
                record.conditional_receiver_drops.iter().all(|fact| {
                    super::super::owner_transport_validation::point_source(fact.point())
                        == record.source
                })
            })
            && self
                .records
                .iter()
                .flat_map(|r| &r.conditional_receiver_drops)
                .eq(owned.conditional_receiver_drops())
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
        for (text, success, error, arguments, receivers) in [
            ("fun run() { val a = \"x\"; println(a) }", true, false, 1, 0),
            (
                "fun run() { val a = \"x\"; val b = a; println(a); println(b) }",
                false,
                true,
                0,
                0,
            ),
            (
                "class Resource {}\nclass Holder(var payload: Resource)\nfun run(holders: List<Holder>): Unit { val projected = holders[0].payload }",
                false,
                false,
                0,
                0,
            ),
            (
                "class Worker(val count: Int) { fun read(): Int = count }\nfun run(own value: Worker): Unit { val n = value.read() }",
                true,
                false,
                0,
                1,
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
                assert_eq!(
                    ledger.records()[0].loans().len(),
                    usize::from(receivers == 0)
                );
                assert_eq!(ledger.argument_contracts().len(), arguments);
                assert_eq!(ledger.receiver_contracts().len(), receivers);
                assert_eq!(
                    ledger.argument_contracts(),
                    owned.ownership().call_argument_contracts()
                );
                assert_eq!(
                    ledger.receiver_contracts(),
                    owned.ownership().call_receiver_contracts()
                );
                assert!(!ledger.records()[0].drops().is_empty());
                assert!(ledger.matches(&owned.clone()));
            } else {
                assert!(owned.validate().is_err());
            }
        }
    }
    #[test]
    fn closure_ledger_seals_final_filter_not_unvisited_exclusive_candidates() {
        for (text, count) in [
            (
                "fun run(): Unit { var number = 1; val write: () -> Unit = { number = 2 }; write() }",
                1,
            ),
            (
                "fun run(): Unit { var number = 1; return; val write: () -> Unit = { number = 2 } }",
                0,
            ),
        ] {
            let mut sources = SourceMap::new();
            let source = sources.add_source("closure-ledger.ko", text).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
            let inputs = [SourceUnitInput::new(
                "root",
                "closure-ledger.ko",
                source,
                &parsed,
            )];
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
            .unwrap()
            .validate()
            .unwrap();
            let ledger = source_collection_ledger(&owned).unwrap();
            let (captures, closures) = ledger.closure_facts().unwrap();
            assert_eq!(captures, owned.ownership().captures());
            assert_eq!(closures, owned.ownership().closures());
            assert_eq!(captures.len(), count);
            assert_eq!(closures.len(), count);
            let lambda_batches: Vec<_> = ledger.records()[0]
                .drop_traversals()
                .iter()
                .filter(|batch| matches!(batch.traversal(), DropTraversal::Lambda(_)))
                .collect();
            assert_eq!(lambda_batches.len(), count);
            assert!(lambda_batches.iter().all(|batch| batch.drops().is_empty()));
            assert!(ledger.matches(&owned));
        }
    }
    #[test]
    fn drop_traversals_preserve_roots_then_visited_lambdas_including_empty() {
        for text in [
            "fun empty(): Unit {}\nfun run(): Unit { val read: () -> Unit = { val a = \"lambda\"; println(a) }; read() }",
            "fun run(): Unit { val read: () -> Unit = { val a = \"lambda\"; println(a) }; read() }\nfun empty(): Unit {}",
            "class Worker { fun read(): Unit { val a = \"member\"; println(a) } }\nfun empty(): Unit {}",
        ] {
            let mut sources = SourceMap::new();
            let source = sources.add_source("drop-traversals.ko", text).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
            let inputs = [SourceUnitInput::new(
                "root",
                "drop-traversals.ko",
                source,
                &parsed,
            )];
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
            .unwrap()
            .validate()
            .unwrap();
            let ledger = source_collection_ledger(&owned).unwrap();
            let record = &ledger.records()[0];
            let batches = record.drop_traversals();
            assert_eq!(
                batches[..parsed.roots().len()]
                    .iter()
                    .map(|batch| batch.traversal())
                    .collect::<Vec<_>>(),
                parsed
                    .roots()
                    .iter()
                    .map(|&root| DropTraversal::SourceRoot(UnitItemId::new(record.source(), root)))
                    .collect::<Vec<_>>()
            );
            assert!(
                batches[..parsed.roots().len()]
                    .iter()
                    .any(|batch| batch.drops().is_empty())
            );
            let lambdas = &batches[parsed.roots().len()..];
            assert_eq!(lambdas.len(), usize::from(text.contains("lambda")));
            for batch in lambdas {
                let DropTraversal::Lambda(lambda) = batch.traversal() else {
                    panic!("lambda second pass")
                };
                assert!(matches!(
                    parsed
                        .ast()
                        .expressions()
                        .get(lambda.expression())
                        .unwrap()
                        .payload(),
                    crate::parser::Expression::Lambda { .. }
                ));
                assert!(!batch.drops().is_empty());
            }
            assert!(
                batches
                    .iter()
                    .flat_map(|batch| batch.drops())
                    .eq(record.drops())
            );
            assert!(
                batches
                    .iter()
                    .flat_map(|batch| batch.drops())
                    .eq(owned.ownership().drops())
            );
            assert!(ledger.matches(&owned));
        }
    }
}

#[cfg(test)]
mod conditional_tests;
