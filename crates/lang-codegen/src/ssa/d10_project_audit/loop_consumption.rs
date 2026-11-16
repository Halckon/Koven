//! Same-run consumption of an original Partial lineage and complete bounded SSA loop receipt.
//! Partial remains an explicit project obligation; this proves neither termination nor general loops.
use super::*;
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{EntityId, EntityType, Function, InstructionId, LoanKind},
    refine::{
        RefineBody, RefineInputs, RefineTypes,
        loop_receipt::{Receipt, Tag},
    },
};
use lang_frontend::{
    ownership_checking::{
        UnitLoanAction, UnitLoanActionCoverage, UnitLoanLoopInvariantLineage, UnitLoanTarget,
        UnitLoweringFacts,
    },
    parser::Statement,
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};
use std::rc::Rc;
type Edge = (FunctionId, InstructionId, FunctionId);
type Call = (UnitFunctionInstanceKey, FunctionId);
struct Expected {
    token: SourceToken,
    key: UnitFunctionInstanceKey,
    callable: UnitItemId,
    lineage: UnitLoanLoopInvariantLineage,
    calls: Option<Vec<Call>>,
    bytes: Option<Vec<u8>>,
    types: Option<[crate::ssa::model::SsaTypeId; 2]>,
}
struct Snapshot {
    receipt: Receipt,
    function: Function,
}
#[derive(Default)]
struct State {
    expected: Vec<Expected>,
    snapshots: Vec<Snapshot>,
}
type Shared = Rc<RefCell<State>>;
thread_local! { static RUNS: RefCell<Vec<Shared>> = const {RefCell::new(Vec::new())}; }
pub(super) struct Transaction {
    state: Shared,
    planned: BTreeMap<SourceToken, (UnitFunctionInstanceKey, UnitItemId)>,
}
impl Transaction {
    pub(super) fn begin(plan: &UnitInstancePlan) -> Self {
        let state = Rc::new(RefCell::new(State::default()));
        RUNS.with(|r| r.borrow_mut().push(state.clone()));
        Self {
            state,
            planned: plan
                .instances()
                .iter()
                .map(|p| {
                    (
                        p.source_token(),
                        (p.key().clone(), UnitItemId::new(p.source_unit(), p.item())),
                    )
                })
                .collect(),
        }
    }
    pub(super) fn finish(
        &self,
        program: &Program,
        mapping: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
        owned: &dyn UnitLoweringFacts,
    ) -> (BTreeSet<Edge>, Vec<DeclarationId>, Vec<String>) {
        let state = self.state.borrow();
        let mut edges = BTreeSet::new();
        let mut callers = Vec::new();
        let mut gaps = Vec::new();
        let mut seen = BTreeSet::new();
        let mut used = BTreeSet::new();
        for e in &state.expected {
            let result = (|| {
                let UnitCallableTarget::Declaration(declaration) = e.key.target() else {
                    return None;
                };
                if !seen.insert(e.lineage.statement())
                    || e.key != UnitFunctionInstanceKey::for_entry(declaration)
                    || self.planned.get(&e.token) != Some(&(e.key.clone(), e.callable))
                {
                    return None;
                }
                let originals: Vec<_> = owned
                    .loan_action_observations()
                    .iter()
                    .flat_map(|c| {
                        c.loop_invariant_lineages()
                            .iter()
                            .filter(|l| l.statement() == e.lineage.statement())
                            .map(move |l| (c, l))
                    })
                    .collect();
                let [(catalog, lineage)] = originals.as_slice() else {
                    return None;
                };
                if !catalog.owns_loop_invariant_lineage(&e.lineage)
                    || *lineage != &e.lineage
                    || lineage.coverage() != UnitLoanActionCoverage::Partial
                    || catalog.source() != e.callable.source_unit()
                    || owned.d10_loop_link(lineage.statement()).ok() != Some(e.callable)
                {
                    return None;
                }
                let caller = *mapping.get(&e.key)?;
                if mapping.values().filter(|id| **id == caller).count() != 1 {
                    return None;
                }
                let matches: Vec<_> = state
                    .snapshots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.receipt.lineage.statement() == lineage.statement())
                    .collect();
                let [(index, snapshot)] = matches.as_slice() else {
                    return None;
                };
                if !used.insert(*index)
                    || snapshot.receipt.lineage != e.lineage
                    || snapshot.receipt.caller != caller
                {
                    return None;
                }
                let calls = e.calls.as_ref()?;
                for (key, id) in calls {
                    if mapping.get(key) != Some(id)
                        || mapping.values().filter(|v| *v == id).count() != 1
                        || program
                            .modules
                            .iter()
                            .flat_map(|m| &m.functions)
                            .filter(|f| f.id == *id)
                            .count()
                            != 1
                    {
                        return None;
                    }
                }
                let edge = validate_final(snapshot, program, e.bytes.as_deref()?, calls, e.types?)?;
                Some((edge, declaration))
            })();
            if let Some((edge, declaration)) = result {
                if !edges.insert(edge) {
                    gaps.push("overlapping loop receipt".into());
                }
                callers.push(declaration);
                gaps.push("loop lineage remains Partial: no general fixed-point, arbitrary-iteration or termination proof".into());
            } else {
                gaps.push("loop missing/duplicate/outside-plan or final evidence gap".into());
            }
        }
        if used.len() != state.snapshots.len() {
            gaps.push("loop unconsumed receipt".into());
        }
        (edges, callers, gaps)
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        RUNS.with(|r| r.borrow_mut().retain(|s| !Rc::ptr_eq(s, &self.state)));
    }
}
pub(in crate::ssa) fn expect(inputs: &RefineInputs<'_>, types: &RefineTypes<'_>) {
    let (Some(item), RefineBody::Block(body)) = (inputs.function_item, &inputs.body) else {
        return;
    };
    let Some(key) = inputs.source_plan.source(inputs.source_token) else {
        return;
    };
    let Ok(node) = inputs.parsed.ast().statements().get(*body) else {
        return;
    };
    let Statement::Block { elements } = node.payload() else {
        return;
    };
    let statements: BTreeSet<_> = elements
        .iter()
        .map(|s| UnitStatementId::new(inputs.source_unit, *s))
        .collect();
    for catalog in inputs.owned.loan_action_observations() {
        for lineage in catalog
            .loop_invariant_lineages()
            .iter()
            .filter(|l| statements.contains(&l.statement()))
        {
            let source = match catalog.action(&lineage.source_begin()) {
                Some(UnitLoanAction::Begin(loan)) => Some(loan),
                _ => None,
            };
            let call = |expression: UnitExpressionId| {
                let site = inputs
                    .source_plan
                    .call_site(inputs.source_token, expression)?;
                if !site.delegation().is_empty() {
                    return None;
                }
                let key = site.dispatch_key()?;
                Some((key.clone(), *inputs.function_ids.get(key)?))
            };
            let calls = source.and_then(|s| {
                std::iter::once(s.call())
                    .chain(lineage.uses().iter().map(|u| u.loan().call()))
                    .map(call)
                    .collect()
            });
            let bytes = source.and_then(|s| match s.target() {
                UnitLoanTarget::Place(place) if place.is_root() => {
                    super::binding_consumption::original_literal(inputs, place.root())
                }
                _ => None,
            });
            let e = Expected {
                token: inputs.source_token,
                key: key.clone(),
                callable: UnitItemId::new(inputs.source_unit, item),
                lineage: lineage.clone(),
                calls,
                bytes,
                types: source.and_then(|s| {
                    let UnitLoanTarget::Place(place) = s.target() else {
                        return None;
                    };
                    let [flag] = inputs.parameters.as_slice() else {
                        return None;
                    };
                    let string = inputs.typed.symbol_type(place.root())?;
                    let boolean = inputs.typed.symbol_type(*flag)?;
                    use lang_frontend::type_checking::{BuiltinType, UnitTypeKind};
                    if inputs.typed.types().get(string)
                        != Some(&UnitTypeKind::Builtin(BuiltinType::String))
                        || inputs.typed.types().get(boolean)
                            != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                    {
                        return None;
                    }
                    Some([
                        *types.type_ids.get(&string)?,
                        *types.type_ids.get(&boolean)?,
                    ])
                }),
            };
            RUNS.with(|r| {
                if let Some(state) = r.borrow().last() {
                    state.borrow_mut().expected.push(e);
                }
            });
        }
    }
}
pub(crate) fn record(function: &Function, receipt: &Receipt) {
    RUNS.with(|r| {
        if let Some(state) = r.borrow().last() {
            state.borrow_mut().snapshots.push(Snapshot {
                function: function.clone(),
                receipt: receipt.clone(),
            });
        }
    });
}
fn validate_final(
    s: &Snapshot,
    program: &Program,
    bytes: &[u8],
    calls: &[Call],
    expected_types: [crate::ssa::model::SsaTypeId; 2],
) -> Option<Edge> {
    let functions: Vec<_> = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .filter(|f| f.id == s.receipt.caller)
        .collect();
    let [function] = functions.as_slice() else {
        return None;
    };
    if **function != s.function
        || s.receipt.validate(function).is_err()
        || calls.len() != s.receipt.lineage.uses().len() + 1
    {
        return None;
    }
    let module = program
        .modules
        .iter()
        .find(|m| m.id == function.id.module())?;
    let operation = |tag| {
        let ids: Vec<_> = s
            .receipt
            .instructions
            .iter()
            .filter(|(t, _)| *t == tag)
            .map(|(_, id)| *id)
            .collect();
        let [id] = ids.as_slice() else {
            return None;
        };
        function.instruction(*id)
    };
    let owner = operation(Tag::Owner)?;
    let Operation::StringLiteral {
        string: ty,
        bytes: actual,
    } = &owner.operation
    else {
        return None;
    };
    if actual != bytes
        || *ty != expected_types[0]
        || module.type_kind(*ty) != Some(&SsaTypeKind::StringOwner)
    {
        return None;
    }
    let entry = function.block(function.entry_block()?)?;
    let [flag] = entry.parameters.as_slice() else {
        return None;
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = function.entity(*flag)?.ty
    else {
        return None;
    };
    if target != expected_types[1]
        || !matches!(flag, EntityId::Loan(_))
        || module.type_kind(target) != Some(&SsaTypeKind::Boolean)
    {
        return None;
    }
    let source = operation(Tag::Result)?;
    let Operation::BorrowCall { callee, .. } = source.operation else {
        return None;
    };
    if callee != calls[0].1 {
        return None;
    }
    for (index, (_, expected)) in calls.iter().skip(1).enumerate() {
        if !matches!(operation(Tag::Consumer(index))?.operation,Operation::DirectCall {callee,receiver:None,..} if callee==*expected)
        {
            return None;
        }
    }
    Some((function.id, source.id, callee))
}
