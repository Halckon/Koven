//! Original complete binding lifecycle, closed against the final whole caller.
use super::*;
use crate::ssa::{
    lowering_support::{callable_instances::SourceToken, string_literal},
    model::{
        Definition, EntityId, EntityType, Function, InstructionId, LoanKind, Operation,
        SsaTypeKind, TerminatorKind,
    },
    refine::{
        RefineBody, RefineInputs,
        binding_receipt::{Plans, Receipt},
    },
};
use lang_frontend::{
    ownership_checking::{UnitLoanActionCoverage, UnitLoanActionId, UnitLoweringFacts},
    parser::{Expression, Item, NameMarker, Statement, VariableKind},
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::rc::Rc;
type Edge = (FunctionId, InstructionId, FunctionId);
type Call = (UnitFunctionInstanceKey, FunctionId);
type Literal = (InstructionId, Vec<u8>, EntityId);
struct Expected {
    token: SourceToken,
    key: UnitFunctionInstanceKey,
    callable: UnitItemId,
    source: UnitExpressionId,
    consumer: UnitExpressionId,
    ids: Vec<UnitLoanActionId>,
    calls: Option<[Call; 2]>,
    bytes: Option<Vec<u8>>,
}
struct Snapshot {
    plans: Plans,
    receipt: Receipt,
    function: Function,
    literals: Vec<Literal>,
}
#[derive(Default)]
struct State {
    expected: Vec<Expected>,
    snapshots: Vec<Snapshot>,
}
type Shared = Rc<RefCell<State>>;
thread_local! {static RUNS:RefCell<Vec<Shared>>=const{RefCell::new(Vec::new())};}
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
    ) -> (BTreeSet<Edge>, Vec<String>) {
        let state = self.state.borrow();
        let mut seen = BTreeSet::new();
        let mut edges = BTreeSet::new();
        let mut gaps = Vec::new();
        for e in &state.expected {
            let UnitCallableTarget::Declaration(declaration) = e.key.target() else {
                gaps.push("binding nonplain callable".into());
                continue;
            };
            if !seen.insert(e.source)
                || e.key != UnitFunctionInstanceKey::for_entry(declaration)
                || self.planned.get(&e.token) != Some(&(e.key.clone(), e.callable))
            {
                gaps.push("binding duplicate/outside-plan candidate".into());
                continue;
            }
            let originals: Vec<_> = owned
                .loan_action_observations()
                .iter()
                .flat_map(|c| {
                    c.binding_regions()
                        .iter()
                        .filter(|r| r.source_call() == e.source)
                        .map(move |r| (c, r))
                })
                .collect();
            let [(catalog, region)] = originals.as_slice() else {
                gaps.push("binding original region missing/duplicate".into());
                continue;
            };
            if !catalog.owns_binding_region(region)
                || region.coverage() != UnitLoanActionCoverage::Complete
                || region.action_ids().collect::<Vec<_>>() != e.ids
                || region.consumer_call() != e.consumer
                || owned.d10_binding_link(e.source) != Ok(e.callable)
            {
                gaps.push("binding original action/callable provenance gap".into());
                continue;
            }
            let matches: Vec<_> = state
                .snapshots
                .iter()
                .filter(|s| s.receipt.source_call == e.source)
                .collect();
            let [snapshot] = matches.as_slice() else {
                gaps.push("missing/duplicate complete binding receipt".into());
                continue;
            };
            let r = &snapshot.receipt;
            if r.ids != e.ids
                || r.callable != e.callable
                || r.consumer_call != e.consumer
                || r.binding != region.binding()
                || r.root != region.root()
                || r.entry_statement != region.entry_statement()
                || r.completion_statement != region.completion_statement()
                || mapping.get(&e.key) != Some(&r.caller)
            {
                gaps.push("binding original identity mismatch".into());
                continue;
            }
            let Some(calls) = &e.calls else {
                gaps.push("binding original dispatch gap".into());
                continue;
            };
            if calls.iter().any(|(key, id)| {
                mapping.get(key) != Some(id)
                    || mapping.values().filter(|f| *f == id).count() != 1
                    || program
                        .modules
                        .iter()
                        .flat_map(|m| &m.functions)
                        .filter(|f| f.id == *id)
                        .count()
                        != 1
            }) || !matches_final(
                snapshot,
                program,
                e.bytes.as_deref(),
                [calls[0].1, calls[1].1],
            ) {
                gaps.push("binding final lifecycle/callee closure mismatch".into());
                continue;
            }
            if !edges.insert((r.caller, r.instructions[2], calls[0].1)) {
                gaps.push("duplicate binding certified edge".into());
            }
        }
        for s in &state.snapshots {
            if !seen.contains(&s.receipt.source_call) {
                gaps.push("binding receipt outside original inputs".into());
            }
        }
        (edges, gaps)
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        RUNS.with(|r| r.borrow_mut().retain(|s| !Rc::ptr_eq(s, &self.state)));
    }
}
pub(in crate::ssa) fn expect(inputs: &RefineInputs<'_>) {
    let Some(item) = inputs.function_item else {
        return;
    };
    let callable = UnitItemId::new(inputs.source_unit, item);
    let Some(key) = inputs.source_plan.source(inputs.source_token) else {
        return;
    };
    for catalog in inputs.owned.loan_action_observations() {
        for region in catalog
            .binding_regions()
            .iter()
            .filter(|r| r.callable() == callable)
        {
            let call = |expression| {
                let site = inputs
                    .source_plan
                    .call_site(inputs.source_token, expression)?;
                if !site.delegation().is_empty() {
                    return None;
                }
                let key = site.dispatch_key()?;
                Some((key.clone(), *inputs.function_ids.get(key)?))
            };
            let expected = Expected {
                token: inputs.source_token,
                key: key.clone(),
                callable,
                source: region.source_call(),
                consumer: region.consumer_call(),
                ids: region.action_ids().collect(),
                calls: call(region.source_call())
                    .zip(call(region.consumer_call()))
                    .map(|(a, b)| [a, b]),
                bytes: original_literal(inputs, region.root()),
            };
            RUNS.with(|r| {
                if let Some(state) = r.borrow().last() {
                    state.borrow_mut().expected.push(expected);
                }
            });
        }
    }
}
pub(super) fn original_literal(
    inputs: &RefineInputs<'_>,
    root: lang_frontend::name_resolution::UnitSymbolId,
) -> Option<Vec<u8>> {
    let RefineBody::Block(body) = inputs.body else {
        return None;
    };
    let ast = inputs.parsed.ast();
    let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
        return None;
    };
    let [first, _, _] = elements.as_slice() else {
        return None;
    };
    let Statement::LocalVariable { declaration } = ast.statements().get(*first).ok()?.payload()
    else {
        return None;
    };
    let Item::Variable {
        kind: VariableKind::Val,
        name: NameMarker::Present(span),
        initializer,
        ..
    } = ast.items().get(*declaration).ok()?.payload()
    else {
        return None;
    };
    let symbol = inputs
        .names
        .names()
        .source_units()
        .get(root.source_unit().index())?
        .resolution()
        .symbols()
        .iter()
        .find(|s| s.id() == root.symbol())?;
    if symbol.span() != *span
        || !matches!(
            ast.expressions().get(*initializer).ok()?.payload(),
            Expression::String { .. }
        )
    {
        return None;
    }
    string_literal::decode_plain(
        inputs.parsed,
        inputs.sources.source_text(inputs.parsed.source_id()).ok()?,
        *initializer,
    )
    .ok()?
}
pub(crate) fn record(plans: &Plans, function: &Function, receipt: &Receipt, literals: &[Literal]) {
    RUNS.with(|r| {
        if let Some(state) = r.borrow().last() {
            state.borrow_mut().snapshots.push(Snapshot {
                plans: plans.clone(),
                function: function.clone(),
                receipt: receipt.clone(),
                literals: literals.to_vec(),
            });
        }
    });
}
fn matches_final(
    s: &Snapshot,
    program: &Program,
    expected_bytes: Option<&[u8]>,
    callees: [FunctionId; 2],
) -> bool {
    validate_final(s, program, expected_bytes, callees).is_some()
}
fn validate_final(
    s: &Snapshot,
    program: &Program,
    expected_bytes: Option<&[u8]>,
    callees: [FunctionId; 2],
) -> Option<()> {
    let r = &s.receipt;
    let functions: Vec<_> = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .filter(|f| f.id == r.caller)
        .collect();
    let [f] = functions.as_slice() else {
        return None;
    };
    if **f != s.function
        || r.validate(&s.plans, f, f.origin.span()).is_err()
        || f.blocks.len() != 1
        || f.instructions.len() != 8
        || f.borrow_return.is_some()
        || f.carrier_return.is_some()
        || f.receiver.is_some()
        || f.never_returns
        || !f.return_types.is_empty()
    {
        return None;
    }
    let block = &f.blocks[0];
    if f.entry_block() != Some(block.id)
        || !block.parameters.is_empty()
        || block.terminator.as_ref()?.abort.is_some()
        || !matches!(&block.terminator.as_ref()?.kind,TerminatorKind::Return{values} if values.is_empty())
    {
        return None;
    }
    let [(literal, bytes, owner)] = s.literals.as_slice() else {
        return None;
    };
    let [
        EntityId::Value(owner_id),
        EntityId::Place(_),
        EntityId::Loan(_),
        EntityId::Loan(_),
    ] = r.entities.as_slice()
    else {
        return None;
    };
    let mut expected = vec![*literal];
    expected.extend_from_slice(&r.instructions);
    if expected.iter().collect::<BTreeSet<_>>().len() != 8
        || block.instructions != expected
        || *owner != EntityId::Value(*owner_id)
        || Some(bytes.as_slice()) != expected_bytes
    {
        return None;
    }
    let literal_data = f.instruction(*literal)?;
    if literal_data.results != [*owner]
        || !matches!(&literal_data.operation,Operation::StringLiteral{bytes:actual,..} if actual==bytes)
    {
        return None;
    }
    if !matches!(f.instruction(r.instructions[2])?.operation,Operation::BorrowCall{callee,..} if callee==callees[0])
        || !matches!(f.instruction(r.instructions[3])?.operation,Operation::DirectCall{callee,..} if callee==callees[1])
    {
        return None;
    }
    let target = match f.entity(*owner)?.ty {
        EntityType::Value(t) => t,
        _ => return None,
    };
    let modules: Vec<_> = program
        .modules
        .iter()
        .filter(|m| m.id == target.module)
        .collect();
    let [module] = modules.as_slice() else {
        return None;
    };
    if target.module != f.id.module
        || module.types.get(target.index) != Some(&SsaTypeKind::StringOwner)
        || f.values.len() != 1
        || f.places.len() != 1
        || f.loans.len() != 2
    {
        return None;
    }
    let mut entities = BTreeSet::new();
    for id in &block.instructions {
        let i = f.instruction(*id)?;
        if i.block != block.id {
            return None;
        }
        for (index, entity) in i.results.iter().enumerate() {
            if !entities.insert(*entity)
                || entity.function() != f.id
                || f.entity(*entity)?.definition
                    != (Definition::InstructionResult {
                        instruction: i.id,
                        index,
                    })
            {
                return None;
            }
        }
    }
    if entities != r.entities.iter().copied().collect() {
        return None;
    }
    for entity in &r.entities {
        let expected = match entity {
            EntityId::Value(_) => EntityType::Value(target),
            EntityId::Place(_) => EntityType::Place(target),
            EntityId::Loan(_) => EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
        };
        if f.entity(*entity)?.ty != expected {
            return None;
        }
    }
    Some(())
}
