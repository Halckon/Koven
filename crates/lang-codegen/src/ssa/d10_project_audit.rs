//! Read-only scoped observations of ordinary whole-unit lowering. Not an admission capability.
//! Only the original successful final verifier publishes an observation. Object publication
//! remains a separate result; this module neither changes lowering nor executes generated code.
use crate::ssa::{
    model::{FunctionId, Operation, Program, SsaTypeKind, TerminatorKind},
    unit_plan::{UnitFunctionInstanceKey, UnitInstancePlan},
};
use lang_frontend::{
    name_resolution::{DeclarationId, SourceUnitId, ValidatedCompilationUnitNames},
    source::SourceId,
    type_checking::UnitCallableTarget,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

/// A restricted direct-call graph observation, not a general callable or runtime certificate.
#[derive(Debug)]
pub struct ProjectAudit {
    /// Number of canonical source units in the original names.
    pub source_count: usize,
    /// Canonical complete source-unit and source identities, copied from the original names.
    pub sources: Vec<(SourceUnitId, SourceId)>,
    /// Original declaration IDs of the complete restricted instance plan.
    pub declarations: Vec<DeclarationId>,
    /// Every actual direct call, retaining multiplicity.
    pub direct_edges: Vec<(DeclarationId, DeclarationId)>,
    /// Explicit SSA runtime-bearing operations, including allocation/drop/abort. These are
    /// observed obligations, not proof of their LLVM/runtime implementations or termination.
    pub runtime_operations: BTreeMap<&'static str, usize>,
    /// Empty only when plan/mapping/Program close within this restricted observation domain.
    pub unsupported: Vec<String>,
}
thread_local! { static OBSERVATIONS: RefCell<Option<Vec<ProjectAudit>>> = const { RefCell::new(None) }; }
/// Observe the caller's original emission exactly once, without replacing its return value.
/// Nested observation is prohibited; unwinding clears the thread-local observation slot.
pub fn observe<T>(run: impl FnOnce() -> T) -> (T, Vec<ProjectAudit>) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            OBSERVATIONS.with(|slot| {
                slot.borrow_mut().take();
            });
        }
    }
    OBSERVATIONS.with(|slot| {
        assert!(slot.borrow().is_none(), "nested project audit");
        *slot.borrow_mut() = Some(Vec::new());
    });
    let _reset = Reset;
    let result = run();
    let observations = OBSERVATIONS.with(|slot| slot.borrow_mut().take().unwrap());
    (result, observations)
}

pub(crate) struct Pending {
    keys: Vec<UnitFunctionInstanceKey>,
    roots: Vec<DeclarationId>,
    sources: Vec<(SourceUnitId, SourceId)>,
}
impl Pending {
    pub(crate) fn capture(
        plan: &UnitInstancePlan,
        names: &ValidatedCompilationUnitNames,
        roots: &[DeclarationId],
    ) -> Option<Self> {
        OBSERVATIONS
            .with(|slot| slot.borrow().is_some())
            .then(|| Self {
                keys: plan
                    .instances()
                    .iter()
                    .map(|instance| instance.key().clone())
                    .collect(),
                roots: roots.to_vec(),
                sources: names
                    .names()
                    .index()
                    .source_units()
                    .iter()
                    .map(|s| (s.id(), s.source_id()))
                    .collect(),
            })
    }
    pub(crate) fn finish(
        self,
        program: &Program,
        mapping: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    ) {
        let mut audit = ProjectAudit {
            source_count: self.sources.len(),
            sources: self.sources,
            declarations: Vec::new(),
            direct_edges: Vec::new(),
            runtime_operations: BTreeMap::new(),
            unsupported: Vec::new(),
        };
        let planned: BTreeSet<_> = self.keys.iter().collect();
        if planned.len() != self.keys.len() || planned != mapping.keys().collect() {
            audit
                .unsupported
                .push("plan/mapping identity mismatch".into());
        }
        let mut declarations = BTreeMap::new();
        for (key, id) in mapping {
            match key.target() {
                UnitCallableTarget::Declaration(d)
                    if *key == UnitFunctionInstanceKey::for_entry(d) =>
                {
                    declarations.insert(*id, d);
                    audit.declarations.push(d);
                }
                _ => audit.unsupported.push(
                    "specialized/member/deinit/callable instance outside direct-only domain".into(),
                ),
            }
        }
        let defined: BTreeSet<_> = program
            .modules
            .iter()
            .flat_map(|module| module.functions.iter())
            .map(|f| f.id)
            .collect();
        let mapped: BTreeSet<_> = mapping.values().copied().collect();
        let definitions = program
            .modules
            .iter()
            .map(|m| m.functions.len())
            .sum::<usize>();
        if definitions != mapping.len() || defined != mapped || mapped.len() != mapping.len() {
            audit
                .unsupported
                .push("Program/mapping definitions mismatch".into());
        }
        for module in &program.modules {
            if !module.deinits.is_empty()
                || !module.map_results.is_empty()
                || !module.nullable_maps.is_empty()
                || !module.integer_ranges.is_empty()
            {
                audit
                    .unsupported
                    .push("runtime descriptor table outside scalar/String domain".into());
            }
            if module.types.iter().any(|ty| {
                !matches!(
                    ty,
                    SsaTypeKind::Unit
                        | SsaTypeKind::Boolean
                        | SsaTypeKind::Integer { .. }
                        | SsaTypeKind::StringOwner
                )
            }) {
                audit
                    .unsupported
                    .push("type outside scalar/String domain".into());
            }
        }
        for function in program.modules.iter().flat_map(|module| &module.functions) {
            for instruction in &function.instructions {
                let runtime = match &instruction.operation {
                    Operation::DirectCall {
                        callee,
                        receiver: None,
                        ..
                    } => {
                        match (declarations.get(&function.id), declarations.get(callee)) {
                            (Some(from), Some(to)) => audit.direct_edges.push((*from, *to)),
                            _ => audit.unsupported.push("unmapped direct call".into()),
                        }
                        None
                    }
                    Operation::StringLiteral { .. } => Some("StringLiteral"),
                    Operation::PrintString { .. } => Some("PrintString"),
                    Operation::Drop { .. } => Some("Drop"),
                    Operation::CheckedArithmetic { .. } => Some("CheckedArithmetic"),
                    Operation::Constant(_)
                    | Operation::Compare { .. }
                    | Operation::Copy { .. }
                    | Operation::Consume { .. }
                    | Operation::RootPlace { .. }
                    | Operation::RootPlaceTake { .. }
                    | Operation::RootReplace { .. }
                    | Operation::BorrowBegin { .. }
                    | Operation::BorrowEnd { .. }
                    | Operation::Read { .. }
                    | Operation::Mutate { .. } => None,
                    other => {
                        audit
                            .unsupported
                            .push(format!("unclassified operation: {other:?}"));
                        None
                    }
                };
                if let Some(runtime) = runtime {
                    *audit.runtime_operations.entry(runtime).or_default() += 1;
                }
            }
            for block in &function.blocks {
                match block.terminator.as_ref().map(|t| &t.kind) {
                    Some(TerminatorKind::Abort) => {
                        *audit.runtime_operations.entry("Abort").or_default() += 1;
                    }
                    Some(
                        TerminatorKind::Branch(_)
                        | TerminatorKind::Conditional { .. }
                        | TerminatorKind::Return { .. },
                    ) => (),
                    other => audit
                        .unsupported
                        .push(format!("unclassified terminator: {other:?}")),
                }
            }
        }
        let mut reached: BTreeSet<_> = self.roots.into_iter().collect();
        loop {
            let before = reached.len();
            for (from, to) in &audit.direct_edges {
                if reached.contains(from) {
                    reached.insert(*to);
                }
            }
            if before == reached.len() {
                break;
            }
        }
        if reached != audit.declarations.iter().copied().collect() {
            audit
                .unsupported
                .push("entry/direct-edge closure differs from complete plan".into());
        }
        audit.declarations.sort();
        OBSERVATIONS.with(|slot| {
            if let Some(observations) = slot.borrow_mut().as_mut() {
                observations.push(audit);
            }
        });
    }
}
