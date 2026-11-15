//! Same-run core no-carry proof for one fresh loop-local result lease, not loop termination.
use super::*;
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{EntityId, EntityType, Function, InstructionId, LoanKind, SsaTypeId},
    refine::{
        RefineInputs, RefineTypes,
        fresh_loop_receipt::{Receipt, SourceConsumer, Tag, audit_segments},
    },
};
use lang_frontend::{
    ownership_checking::{UnitLoopNoCarryFact, UnitLoweringFacts},
    type_checking::{ParameterMode, UnitExpressionId, UnitItemId},
};
use std::rc::Rc;
mod sequence;
type Edge = (FunctionId, InstructionId, FunctionId);
type Call = (UnitFunctionInstanceKey, FunctionId);
struct Expected {
    token: SourceToken,
    key: UnitFunctionInstanceKey,
    fact: UnitLoopNoCarryFact,
    calls: Option<Vec<Call>>,
    abi: Option<[SsaTypeId; 2]>,
    source_index: Option<usize>,
    segments: Option<[Vec<SourceConsumer>; 2]>,
    sequence: Option<sequence::Source>,
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
    ) -> (BTreeSet<Edge>, Vec<String>) {
        let state = self.state.borrow();
        let mut edges = BTreeSet::new();
        let mut gaps = Vec::new();
        let mut seen = BTreeSet::new();
        let mut used = BTreeSet::new();
        let mut grouped = BTreeSet::new();
        for e in &state.expected {
            let group: Vec<_> = state
                .expected
                .iter()
                .filter(|other| other.token == e.token)
                .collect();
            if group.len() == 1 || !grouped.insert(e.token) {
                continue;
            }
            let result = sequence::finish(self, &state, &group, program, mapping, owned, &mut used);
            match result {
                Some(pair)
                    if pair[0] != pair[1] && pair.iter().all(|edge| !edges.contains(edge)) =>
                {
                    edges.extend(pair);
                }
                _ => gaps.push("fresh no-carry sequential caller evidence gap".into()),
            }
        }
        for e in &state.expected {
            if grouped.contains(&e.token) {
                continue;
            }
            let result = (|| {
                let UnitCallableTarget::Declaration(declaration) = e.key.target() else {
                    return None;
                };
                if !seen.insert(e.fact.loop_statement())
                    || !matches!(e.fact.consumers().len(), 1 | 2)
                    || e.key != UnitFunctionInstanceKey::for_entry(declaration)
                    || self.planned.get(&e.token) != Some(&(e.key.clone(), e.fact.callable()))
                {
                    return None;
                }
                let originals: Vec<_> = owned
                    .no_carry_loops()
                    .iter()
                    .filter(|f| f.loop_statement() == e.fact.loop_statement())
                    .collect();
                let [original] = originals.as_slice() else {
                    return None;
                };
                if !owned.owns_no_carry_loop(&e.fact)
                    || !original.same_identity(&e.fact)
                    || **original != e.fact
                    || owned.d10_fresh_loop_link(e.fact.loop_statement()).ok()
                        != Some(e.fact.callable())
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
                    .filter(|(_, s)| {
                        s.receipt
                            .loops
                            .iter()
                            .any(|l| l.fact.loop_statement() == e.fact.loop_statement())
                    })
                    .collect();
                let [(index, s)] = matches.as_slice() else {
                    return None;
                };
                if !used.insert(*index) || s.receipt.caller != caller || s.receipt.loops.len() != 1
                {
                    return None;
                }
                let local = &s.receipt.loops[0];
                if !local.fact.same_identity(&e.fact)
                    || local.fact != e.fact
                    || !s.receipt.matches_source_segments(e.segments.as_ref()?)
                {
                    return None;
                }
                let segments = e.segments.as_ref()?;
                let body_count = e.fact.consumers().len();
                if !matches!(
                    (body_count, segments[0].len(), segments[1].len()),
                    (1, 0, 0)
                        | (1, 1, 0)
                        | (1, 0, 1)
                        | (1, 1, 1)
                        | (2, 0, 0)
                        | (2, 1, 0)
                        | (2, 0, 1)
                        | (2, 1, 1)
                ) {
                    return None;
                }
                let calls = e.calls.as_ref()?;
                if calls.len() != 1 + body_count + segments.iter().map(Vec::len).sum::<usize>() {
                    return None;
                }
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
                validate_final(s, program, calls, e.abi?, e.source_index?)
            })();
            match result {
                Some(edge) if edges.insert(edge) => (),
                _ => gaps.push(
                    "fresh no-carry missing/duplicate/outside-domain or final evidence gap".into(),
                ),
            }
        }
        if used.len() != state.snapshots.len() {
            gaps.push("fresh no-carry unconsumed receipt".into());
        }
        (edges, gaps)
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
    let Some(key) = inputs.source_plan.source(inputs.source_token) else {
        return;
    };
    let callable = UnitItemId::new(inputs.source_unit, item);
    for fact in inputs
        .owned
        .no_carry_loops()
        .iter()
        .filter(|f| f.callable() == callable)
    {
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
        let source_index = inputs.parameters.iter().position(|p| *p == fact.source());
        let abi = (|| {
            if inputs.parameters.len() != 2
                || inputs.modes.as_slice() != [ParameterMode::Borrow, ParameterMode::Borrow]
            {
                return None;
            }
            let index = source_index?;
            let mut result = Vec::new();
            for (i, symbol) in inputs.parameters.iter().enumerate() {
                let ty = inputs.typed.symbol_type(*symbol)?;
                use lang_frontend::type_checking::{BuiltinType, UnitTypeKind};
                let expected = if i == index {
                    BuiltinType::String
                } else {
                    BuiltinType::Boolean
                };
                if inputs.typed.types().get(ty) != Some(&UnitTypeKind::Builtin(expected)) {
                    return None;
                }
                result.push(*types.type_ids.get(&ty)?);
            }
            result.try_into().ok()
        })();
        let segments = audit_segments(inputs, fact);
        let e = Expected {
            token: inputs.source_token,
            key: key.clone(),
            fact: fact.clone(),
            abi,
            source_index,
            calls: std::iter::once(fact.initializer())
                .chain(fact.consumers().iter().map(|(_, c, _)| *c))
                .chain(segments.iter().flatten().flatten().map(|c| c.call))
                .map(call)
                .collect(),
            segments,
            sequence: sequence::original_order(inputs, fact, types),
        };
        RUNS.with(|r| {
            if let Some(s) = r.borrow().last() {
                s.borrow_mut().expected.push(e);
            }
        });
    }
}
pub(crate) fn record(function: &Function, receipt: &Receipt) {
    RUNS.with(|r| {
        if let Some(s) = r.borrow().last() {
            s.borrow_mut().snapshots.push(Snapshot {
                function: function.clone(),
                receipt: receipt.clone(),
            });
        }
    });
}
fn validate_final(
    s: &Snapshot,
    program: &Program,
    calls: &[Call],
    abi: [SsaTypeId; 2],
    source_index: usize,
) -> Option<Edge> {
    let f = validate_caller(s, program, &abi, source_index)?;
    let edge = validate_loop(s, f, calls, 0)?;
    validate_segments(
        s,
        f,
        &calls[1 + s.receipt.loops[0].fact.consumers().len()..],
    )?;
    Some(edge)
}
fn validate_caller<'a>(
    s: &Snapshot,
    program: &'a Program,
    abi: &[SsaTypeId],
    source_index: usize,
) -> Option<&'a Function> {
    let functions: Vec<_> = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .filter(|f| f.id == s.receipt.caller)
        .collect();
    let [f] = functions.as_slice() else {
        return None;
    };
    if **f != s.function || s.receipt.validate(f).is_err() {
        return None;
    }
    let modules: Vec<_> = program
        .modules
        .iter()
        .filter(|m| m.id == f.id.module)
        .collect();
    let [module] = modules.as_slice() else {
        return None;
    };
    let entry = f.block(f.entry_block()?)?;
    if entry.parameters.len() != abi.len() || source_index >= abi.len() {
        return None;
    }
    for (i, (entity, ty)) in entry.parameters.iter().zip(abi.iter().copied()).enumerate() {
        let kind = if i == source_index {
            SsaTypeKind::StringOwner
        } else {
            SsaTypeKind::Boolean
        };
        if ty.module != f.id.module
            || module.type_kind(ty) != Some(&kind)
            || !matches!(entity, EntityId::Loan(_))
            || f.entity(*entity)?.ty
                != (EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                })
        {
            return None;
        }
    }
    Some(f)
}
fn validate_loop(s: &Snapshot, f: &Function, calls: &[Call], index: usize) -> Option<Edge> {
    let operation = |tag| {
        let ids: Vec<_> = s.receipt.loops[index]
            .instructions
            .iter()
            .filter(|(t, _)| *t == tag)
            .map(|(_, id)| *id)
            .collect();
        let [id] = ids.as_slice() else {
            return None;
        };
        f.instruction(*id)
    };
    let source = operation(Tag::Result)?;
    let Operation::BorrowCall { callee, .. } = source.operation else {
        return None;
    };
    if callee != calls[0].1 {
        return None;
    }
    let [result] = source.results.as_slice() else {
        return None;
    };
    let body_count = s.receipt.loops[index].fact.consumers().len();
    let body = f.block(source.block)?;
    let mut previous = body.instructions.iter().position(|id| *id == source.id)?;
    for i in 0..body_count {
        let consumer = operation(Tag::Consumer(i))?;
        if consumer.block != source.block
            || !matches!(&consumer.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==calls[1+i].1 && arguments.as_slice()==[*result])
        {
            return None;
        }
        let position = body.instructions.iter().position(|id| *id == consumer.id)?;
        if position <= previous {
            return None;
        }
        previous = position;
    }
    let end = operation(Tag::ResultEnd)?;
    if end.block!=source.block || !matches!(end.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==*result)
        || body.instructions.iter().position(|id| *id==end.id)?<=previous
        || f.instructions.iter().filter(|i| matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==*result)).count()!=1 {return None;}
    Some((f.id, source.id, callee))
}
fn validate_segments(s: &Snapshot, f: &Function, calls: &[Call]) -> Option<()> {
    let mut mapped = calls.iter();
    for segment in &s.receipt.segments {
        for id in &segment.instructions {
            let (_, expected) = mapped.next()?;
            if !matches!(f.instruction(*id)?.operation,Operation::DirectCall{callee,receiver:None,..} if callee==*expected)
            {
                return None;
            }
        }
    }
    if mapped.next().is_some() {
        return None;
    }
    Some(())
}
