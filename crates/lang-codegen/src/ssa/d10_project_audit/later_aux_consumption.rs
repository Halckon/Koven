//! Original complete auxiliary-binding lifecycle, closed against the final whole caller.
use super::*;
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{
        Definition, EntityId, EntityType, Function, InstructionId, LoanKind, Operation,
        SsaTypeKind, TerminatorKind,
    },
    refine::{
        RefineInputs, RefineTypes,
        later_aux_receipt::{Plans, Receipt},
    },
};
use lang_frontend::{
    ownership_checking::{
        UnitBorrowCallHandoffFact, UnitLoanAction, UnitLoanActionCoverage, UnitLoanActionId,
        UnitLoweringFacts,
    },
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::rc::Rc;
type Edge = (FunctionId, InstructionId, FunctionId);
type Call = (UnitFunctionInstanceKey, FunctionId);
type Literal = (InstructionId, Vec<u8>, EntityId);
/// Minted only after the complete original later-use auxiliary lifecycle passes final validation.
/// Private fields prevent another route from manufacturing a discharge from an edge alone.
pub(super) struct ValidatedHandoff {
    handoff: UnitBorrowCallHandoffFact,
    callable: UnitItemId,
    caller: Call,
    callee: Call,
    instruction: crate::ssa::model::Instruction,
}
impl ValidatedHandoff {
    pub(super) fn call(&self) -> UnitExpressionId {
        self.handoff.source_loan().call()
    }
    pub(super) fn matches(
        &self,
        handoff: &UnitBorrowCallHandoffFact,
        callable: UnitItemId,
        program: &Program,
        mapping: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    ) -> bool {
        let functions: Vec<_> = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .filter(|f| f.id == self.caller.1)
            .collect();
        self.handoff == *handoff
            && self.callable == callable
            && mapping.get(&self.caller.0) == Some(&self.caller.1)
            && mapping.get(&self.callee.0) == Some(&self.callee.1)
            && matches!(functions.as_slice(), [f] if f.instruction(self.instruction.id) == Some(&self.instruction))
            && matches!(self.instruction.operation, Operation::BorrowCall {callee,..} if callee == self.callee.1)
    }
}
struct Expected {
    token: SourceToken,
    key: UnitFunctionInstanceKey,
    callable: UnitItemId,
    source: UnitExpressionId,
    consumer: UnitExpressionId,
    later: Option<UnitExpressionId>,
    ids: Vec<UnitLoanActionId>,
    calls: Option<[Call; 3]>,
    bytes: [Option<Vec<u8>>; 2],
    string: [Option<crate::ssa::model::SsaTypeId>; 2],
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
    ) -> (BTreeSet<Edge>, Vec<ValidatedHandoff>, Vec<String>) {
        let state = self.state.borrow();
        let mut seen = BTreeSet::new();
        let mut edges = BTreeSet::new();
        let mut gaps = Vec::new();
        let mut fulfilled = Vec::new();
        for e in &state.expected {
            let UnitCallableTarget::Declaration(declaration) = e.key.target() else {
                gaps.push("later auxiliary binding nonplain callable".into());
                continue;
            };
            if !seen.insert(e.source)
                || e.key != UnitFunctionInstanceKey::for_entry(declaration)
                || self.planned.get(&e.token) != Some(&(e.key.clone(), e.callable))
            {
                gaps.push("later auxiliary binding duplicate/outside-plan candidate".into());
                continue;
            }
            let originals: Vec<_> = owned
                .loan_action_observations()
                .iter()
                .flat_map(|c| {
                    c.later_auxiliary_binding_regions()
                        .iter()
                        .filter(|r| r.source_call() == e.source)
                        .map(move |r| (c, r))
                })
                .collect();
            let [(catalog, region)] = originals.as_slice() else {
                gaps.push("later auxiliary binding original region missing/duplicate".into());
                continue;
            };
            if !catalog.owns_later_auxiliary_binding_region(region)
                || region.coverage() != UnitLoanActionCoverage::Complete
                || region.action_ids().collect::<Vec<_>>() != e.ids
                || region.consumer_call() != e.consumer
                || region.later_consumer_call() != e.later
                || owned.d10_later_aux_binding_link(e.source) != Ok(e.callable)
            {
                gaps.push("later auxiliary binding original action/callable provenance gap".into());
                continue;
            }
            let matches: Vec<_> = state
                .snapshots
                .iter()
                .filter(|s| s.receipt.source_call == e.source)
                .collect();
            let [snapshot] = matches.as_slice() else {
                gaps.push("missing/duplicate complete later auxiliary binding receipt".into());
                continue;
            };
            let r = &snapshot.receipt;
            if r.ids != e.ids
                || r.callable != e.callable
                || r.consumer_call != e.consumer
                || Some(r.later_call) != e.later
                || r.binding != region.binding()
                || r.root != region.root()
                || Some(r.auxiliary_root) != region.auxiliary_root()
                || r.entry_statement != region.entry_statement()
                || r.completion_statement != region.completion_statement()
                || mapping.get(&e.key) != Some(&r.caller)
                || mapping.values().filter(|id| **id == r.caller).count() != 1
            {
                gaps.push("later auxiliary binding original identity mismatch".into());
                continue;
            }
            let Some(calls) = &e.calls else {
                gaps.push("later auxiliary binding original dispatch gap".into());
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
                [e.bytes[0].as_deref(), e.bytes[1].as_deref()],
                [calls[0].1, calls[1].1, calls[2].1],
                e.string,
            ) {
                gaps.push("later auxiliary binding final lifecycle/callee closure mismatch".into());
                continue;
            }
            let Some(UnitLoanAction::BorrowHandoffCommitted(handoff)) = catalog.action(&e.ids[2])
            else {
                gaps.push("later auxiliary binding original handoff action gap".into());
                continue;
            };
            let originals: Vec<_> = owned
                .borrow_call_handoffs()
                .iter()
                .filter(|h| h.source_loan().call() == e.source)
                .collect();
            if !matches!(originals.as_slice(), [original] if *original == handoff) {
                gaps.push("later auxiliary binding original handoff fact mismatch".into());
                continue;
            }
            let Some(instruction) = snapshot.function.instruction(r.instructions[4]) else {
                gaps.push("later auxiliary binding source instruction missing".into());
                continue;
            };
            if !edges.insert((r.caller, r.instructions[4], calls[0].1)) {
                gaps.push("duplicate later auxiliary binding certified edge".into());
                continue;
            }
            fulfilled.push(ValidatedHandoff {
                handoff: handoff.clone(),
                callable: e.callable,
                caller: (e.key.clone(), r.caller),
                callee: calls[0].clone(),
                instruction: instruction.clone(),
            });
        }
        for s in &state.snapshots {
            if !seen.contains(&s.receipt.source_call) {
                gaps.push("later auxiliary binding receipt outside original inputs".into());
            }
        }
        (edges, fulfilled, gaps)
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        RUNS.with(|r| r.borrow_mut().retain(|s| !Rc::ptr_eq(s, &self.state)));
    }
}
pub(in crate::ssa) fn expect(inputs: &RefineInputs<'_>, types: &RefineTypes<'_>) {
    let Some(item) = inputs.function_item else {
        return;
    };
    let callable = UnitItemId::new(inputs.source_unit, item);
    let Some(key) = inputs.source_plan.source(inputs.source_token) else {
        return;
    };
    for catalog in inputs.owned.loan_action_observations() {
        for region in catalog
            .later_auxiliary_binding_regions()
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
                later: region.later_consumer_call(),
                ids: region.action_ids().collect(),
                calls: call(region.source_call())
                    .zip(call(region.consumer_call()))
                    .zip(region.later_consumer_call().and_then(call))
                    .map(|((a, b), c)| [a, b, c]),
                bytes: [
                    original_literal(inputs, region.root(), 0),
                    region
                        .auxiliary_root()
                        .and_then(|root| original_literal(inputs, root, 1)),
                ],
                string: [Some(region.root()), region.auxiliary_root()].map(|root| {
                    let t = inputs.typed.symbol_type(root?)?;
                    use lang_frontend::type_checking::{BuiltinType, UnitTypeKind};
                    (inputs.typed.types().get(t)
                        == Some(&UnitTypeKind::Builtin(BuiltinType::String)))
                    .then(|| types.type_ids.get(&t).copied())
                    .flatten()
                }),
            };
            RUNS.with(|r| {
                if let Some(state) = r.borrow().last() {
                    state.borrow_mut().expected.push(expected);
                }
            });
        }
    }
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
    expected_bytes: [Option<&[u8]>; 2],
    callees: [FunctionId; 3],
    string: [Option<crate::ssa::model::SsaTypeId>; 2],
) -> bool {
    validate_final(s, program, expected_bytes, callees, string).is_some()
}
fn validate_final(
    s: &Snapshot,
    program: &Program,
    expected_bytes: [Option<&[u8]>; 2],
    callees: [FunctionId; 3],
    string: [Option<crate::ssa::model::SsaTypeId>; 2],
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
        || f.instructions.len() != 17
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
    let [source_literal, aux_literal] = s.literals.as_slice() else {
        return None;
    };
    let [
        source_owner,
        aux_owner,
        EntityId::Place(_),
        EntityId::Loan(_),
        EntityId::Place(_),
        EntityId::Loan(_),
        EntityId::Loan(_),
        EntityId::Place(_),
        EntityId::Loan(_),
    ] = r.entities.as_slice()
    else {
        return None;
    };
    if !matches!(
        (source_owner, aux_owner),
        (EntityId::Value(_), EntityId::Value(_))
    ) || source_owner == aux_owner
    {
        return None;
    }
    let mut expected = vec![source_literal.0, aux_literal.0];
    expected.extend_from_slice(&r.instructions);
    if expected.iter().collect::<BTreeSet<_>>().len() != 17 || block.instructions != expected {
        return None;
    }
    for (index, ((literal, bytes, actual_owner), owner)) in [source_literal, aux_literal]
        .into_iter()
        .zip([source_owner, aux_owner])
        .enumerate()
    {
        let data = f.instruction(*literal)?;
        if actual_owner != owner
            || data.results != [*owner]
            || Some(bytes.as_slice()) != expected_bytes[index]
            || !matches!(&data.operation,Operation::StringLiteral{bytes:actual,string:ty} if actual == bytes && Some(*ty) == string[index])
        {
            return None;
        }
    }
    if !matches!(f.instruction(r.instructions[4])?.operation,Operation::BorrowCall{callee,..} if callee==callees[0])
        || !matches!(f.instruction(r.instructions[6])?.operation,Operation::DirectCall{callee,..} if callee==callees[1])
        || !matches!(f.instruction(r.instructions[12])?.operation,Operation::DirectCall{callee,..} if callee==callees[2])
    {
        return None;
    }
    let target = match f.entity(*source_owner)?.ty {
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
    if string != [Some(target); 2]
        || target.module != f.id.module
        || module.types.get(target.index) != Some(&SsaTypeKind::StringOwner)
        || f.values.len() != 2
        || f.places.len() != 3
        || f.loans.len() != 4
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

fn original_literal(
    inputs: &RefineInputs<'_>,
    root: lang_frontend::name_resolution::UnitSymbolId,
    position: usize,
) -> Option<Vec<u8>> {
    use crate::ssa::{lowering_support::string_literal, refine::RefineBody};
    use lang_frontend::parser::{Expression, Item, NameMarker, Statement, VariableKind};
    let RefineBody::Block(body) = inputs.body else {
        return None;
    };
    let ast = inputs.parsed.ast();
    let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
        return None;
    };
    let [source, auxiliary, _, _, _] = elements.as_slice() else {
        return None;
    };
    let first = match position {
        0 => source,
        1 => auxiliary,
        _ => return None,
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
