//! Feature-only actual entry lifetime events. This is not a route or catalog capability.
use super::{
    RefineInputs,
    body::{BlockRef, RefinedBody, Slot},
};
use crate::ssa::model::{EntityId, Function, FunctionId, Instruction, Operation, TerminatorKind};
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitDropFact, UnitDropPoint, UnitLoanTarget, d10_catalog_experiment::EntryBindingEvidence,
    },
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::collections::BTreeMap;

pub(super) type Position = (BlockRef, usize);
#[derive(Clone, Debug)]
pub(super) struct Pending {
    pub evidence: EntryBindingEvidence,
    pub commit: Option<(Position, Slot, Vec<Slot>)>,
    pub arguments: Vec<(UnitExpressionId, Position, Slot)>,
    pub consumers: Vec<(UnitExpressionId, Position)>,
    pub retirement: Option<(Position, Slot, Vec<Slot>)>,
    pub source_retirement: Option<(Position, Vec<Slot>)>,
    pub drop: Option<(Position, UnitDropFact, Slot)>,
    pub invalid: bool,
    pub scalar: Option<(Position, Slot, Slot, crate::ssa::model::EntityType)>,
    consumer_callees: Vec<FunctionId>,
    expected_drop: UnitDropFact,
}
impl Pending {
    pub(super) fn new(
        inputs: &RefineInputs<'_>,
        call: UnitExpressionId,
        callable: UnitItemId,
    ) -> Option<Self> {
        let candidates: Vec<_> = inputs
            .owned
            .loan_action_observations()
            .iter()
            .flat_map(|o| o.entry_binding_evidence().iter().map(move |e| (o, e)))
            .filter(|(_, e)| {
                e.callable() == callable && e.transaction().source_loan().call() == call
            })
            .collect();
        let [(catalog, evidence)] = candidates.as_slice() else {
            return None;
        };
        if !catalog.owns_entry_binding_evidence(evidence)
            || evidence.ambiguous_use()
            || !matches!(evidence.uses().len(), 1 | 2)
            || evidence.uses().iter().any(|u| {
                u.returned().is_none_or(|f| f.call() != u.consumer()) || u.begin().is_none()
            })
            || evidence
                .uses()
                .iter()
                .map(|u| u.consumer())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != evidence.uses().len()
            || evidence.source_begin().is_none()
            || evidence.handoff().is_none()
            || evidence.use_begin().is_none()
            || evidence.binding_end().is_none()
            || !matches!(
                evidence.transaction().point(),
                UnitDropPoint::AfterStatement(_)
            )
        {
            return None;
        }
        let UnitLoanTarget::Place(place) = evidence.transaction().source_loan().target() else {
            return None;
        };
        let drops: Vec<_> = inputs
            .owned
            .drops()
            .iter()
            .filter(|f| {
                f.point() == evidence.transaction().point()
                    && f.target()
                        == lang_frontend::ownership_checking::UnitDropTarget::Named(place.root())
            })
            .collect();
        let [expected_drop] = drops.as_slice() else {
            return None;
        };
        let consumer_callees = evidence
            .uses()
            .iter()
            .map(|u| {
                let site = inputs
                    .source_plan
                    .call_site(inputs.source_token, u.consumer())?;
                if !site.delegation().is_empty() {
                    return None;
                }
                inputs.function_ids.get(site.dispatch_key()?).copied()
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            consumer_callees,
            expected_drop: **expected_drop,
            evidence: (*evidence).clone(),
            commit: None,
            arguments: Vec::new(),
            consumers: Vec::new(),
            retirement: None,
            source_retirement: None,
            drop: None,
            invalid: false,
            scalar: None,
        })
    }
    pub(super) fn binding(&self) -> UnitSymbolId {
        self.evidence.transaction().binding()
    }
}
#[derive(Default)]
pub(super) struct Emitted {
    instructions: BTreeMap<Position, (Instruction, BTreeMap<Slot, EntityId>)>,
    duplicate: bool,
    literal_bytes: BTreeMap<Position, Vec<u8>>,
}
impl Emitted {
    pub(super) fn statement(
        &mut self,
        position: Position,
        statement: &super::body::RStmt,
        instruction: &Instruction,
        current: &BTreeMap<Slot, EntityId>,
    ) {
        if let super::body::ROp::StringLiteral { bytes, .. } = &statement.op {
            self.literal_bytes.insert(position, bytes.clone());
        }
        self.duplicate |= self
            .instructions
            .insert(position, (instruction.clone(), current.clone()))
            .is_some();
    }
    pub(super) fn finish(&self, body: &RefinedBody, function: &Function) {
        for entry in &body.entry_borrows {
            if let Some(pending) = &entry.lifecycle {
                let calls: Vec<_> = self
                    .instructions
                    .values()
                    .filter_map(|(i, current)| entry.emitted(function, i.id, current))
                    .collect();
                let [call_receipt] = calls.as_slice() else {
                    continue;
                };
                let receipt = Receipt {
                    call_instruction: call_receipt.instruction.clone(),
                    call: entry.call,
                    function: function.clone(),
                    pending: pending.clone(),
                    emitted: self.instructions.clone(),
                    literal_bytes: self.literal_bytes.clone(),
                };
                if !self.duplicate && receipt.valid() {
                    crate::ssa::d10_project_audit::entry_borrow::record_lifetime(receipt);
                }
            }
        }
    }
}
#[derive(Clone)]
pub(crate) struct Receipt {
    pub(crate) call: UnitExpressionId,
    call_instruction: Instruction,
    literal_bytes: BTreeMap<Position, Vec<u8>>,
    pub(crate) function: Function,
    pending: Pending,
    emitted: BTreeMap<Position, (Instruction, BTreeMap<Slot, EntityId>)>,
}
impl Receipt {
    pub(crate) fn evidence(&self) -> &EntryBindingEvidence {
        &self.pending.evidence
    }
    pub(crate) fn caller(&self) -> FunctionId {
        self.function.id
    }
    pub(crate) fn matches_final(&self, function: &Function, call: &Instruction) -> bool {
        call == &self.call_instruction && function == &self.function && self.valid()
    }
    fn valid(&self) -> bool {
        self.validate().is_some()
    }
    fn validate(&self) -> Option<()> {
        let p = &self.pending;
        if p.invalid {
            return None;
        }
        let (commit, result, continued) = p.commit.as_ref()?;
        let count = p.evidence.uses().len();
        let scalar_width = if p.evidence.scalar().is_some() { 2 } else { 0 };
        if (scalar_width != 0 && count != 2) || p.scalar.is_some() != p.evidence.scalar().is_some()
        {
            return None;
        }
        if !matches!(count, 1 | 2)
            || p.arguments.len() != count
            || p.consumers.len() != count
            || p.consumer_callees.len() != count
        {
            return None;
        }

        let (retire, ended, sources) = p.retirement.as_ref()?;
        let (drop, fact, owner) = p.drop.as_ref()?;
        if fact != &p.expected_drop {
            return None;
        }
        let (source_position, removed_sources) = p.source_retirement.as_ref()?;
        if *source_position != (retire.0, retire.1 + 1) || removed_sources != sources {
            return None;
        }
        if result != ended
            || continued != sources
            || continued.len() != 1
            || commit.0 != BlockRef(0)
            || retire.0 != commit.0
            || drop.0 != commit.0
            || retire.1 != commit.1 + count + scalar_width
            || retire.1 + 2 != drop.1
            || fact.point() != p.evidence.transaction().point()
        {
            return None;
        }
        let source = p.evidence.transaction().source_loan();
        let UnitLoanTarget::Place(place) = source.target() else {
            return None;
        };
        if fact.target() != lang_frontend::ownership_checking::UnitDropTarget::Named(place.root()) {
            return None;
        }
        let f = &self.function;
        let [block] = f.blocks.as_slice() else {
            return None;
        };
        if f.entry_block() != Some(block.id)
            || block.parameters.len() != 1
            || f.borrow_return.is_some()
            || f.carrier_return.is_some()
            || f.receiver.is_some()
            || f.never_returns
            || !f.return_types.is_empty()
            || !matches!(block.terminator.as_ref()?.kind,TerminatorKind::Return{ref values} if values.is_empty())
        {
            return None;
        }
        let mut used_entity = None;
        for (index, expected) in p.evidence.uses().iter().enumerate() {
            let (arg_call, arg, used) = p.arguments[index];
            let (actual_call, consumer) = p.consumers[index];
            if arg_call != expected.consumer()
                || actual_call != arg_call
                || used != *result
                || arg != consumer
                || consumer
                    != (
                        commit.0,
                        commit.1 + index + if index == 1 { scalar_width } else { 0 },
                    )
            {
                return None;
            }
            let (instruction, map) = self.emitted.get(&consumer)?;
            let entity = *map.get(result)?;
            if used_entity.is_some_and(|previous| previous != entity)
                || !instruction.results.is_empty()
                || !matches!(&instruction.operation,Operation::DirectCall{callee,arguments,receiver:None} if *callee==p.consumer_callees[index] && arguments==&[entity])
            {
                return None;
            }
            used_entity = Some(entity);
        }
        if let Some(expected) = p.evidence.scalar() {
            let (position, value, binding, ty) = p.scalar?;
            if position != (commit.0, commit.1 + 1) {
                return None;
            }
            let (instruction, map) = self.emitted.get(&position)?;
            let entity = *map.get(&value)?;
            let (_, next_map) = self.emitted.get(&p.consumers[1].1)?;
            if !matches!(entity, EntityId::Value(_))
                || next_map.get(&binding) != Some(&entity)
                || instruction.results != [entity]
                || instruction.operation
                    != Operation::Constant(crate::ssa::model::ScalarConstant::Integer(
                        expected.value(),
                    ))
                || f.entity(entity)?.ty != ty
            {
                return None;
            }
        }
        let result_entity = used_entity?;
        let (end_result, end_map) = self.emitted.get(retire)?;
        let EntityId::Loan(result_loan) = *end_map.get(result)? else {
            return None;
        };
        let EntityId::Loan(source_loan) = *end_map.get(&continued[0])? else {
            return None;
        };
        if result_entity != EntityId::Loan(result_loan) {
            return None;
        }
        if end_result.operation != (Operation::BorrowEnd { loan: result_loan })
            || !end_result.results.is_empty()
        {
            return None;
        }
        let (end_source, _) = self.emitted.get(&(retire.0, retire.1 + 1))?;
        if end_source.operation != (Operation::BorrowEnd { loan: source_loan })
            || !end_source.results.is_empty()
        {
            return None;
        }
        let (drop_instruction, drop_map) = self.emitted.get(drop)?;
        let EntityId::Value(owner_value) = *drop_map.get(owner)? else {
            return None;
        };
        if drop_instruction.operation != (Operation::Drop { owner: owner_value })
            || !drop_instruction.results.is_empty()
        {
            return None;
        }
        // Every final instruction must be an actual emitted statement of this sole block.
        if block.terminator.as_ref()?.abort.is_some() {
            return None;
        }
        let recorded: Vec<_> = self.emitted.values().map(|(i, _)| i.id).collect();
        if recorded
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != recorded.len()
            || recorded != block.instructions
            || recorded.len() != f.instructions.len()
            || self
                .emitted
                .values()
                .any(|(i, _)| f.instruction(i.id) != Some(i))
        {
            return None;
        }
        // This narrow entry contains one creation, one source acquisition, one source call,
        // exactly one or two consumers, then the two retirements and original owner drop.
        let mut literals = 0;
        let mut roots = 0;
        let mut begins = 0;
        let mut source_calls = 0;
        let mut source_place = None;
        for (position, (i, _)) in &self.emitted {
            if p.scalar
                .as_ref()
                .is_some_and(|(scalar, _, _, _)| position == scalar)
                || p.consumers.iter().any(|(_, consumer)| position == consumer)
                || *position == *retire
                || *position == (retire.0, retire.1 + 1)
                || *position == *drop
            {
                continue;
            }
            if position.1 >= commit.1 {
                return None;
            }
            match &i.operation {
                Operation::StringLiteral { bytes, .. }
                    if self.literal_bytes.get(position) == Some(bytes)
                        && i.results == [EntityId::Value(owner_value)] =>
                {
                    literals += 1
                }
                Operation::RootPlace { owner } if *owner == owner_value => {
                    let [EntityId::Place(place)] = i.results.as_slice() else {
                        return None;
                    };
                    source_place = Some(*place);
                    roots += 1;
                }
                Operation::BorrowBegin {
                    place,
                    kind: crate::ssa::model::LoanKind::Shared,
                } if Some(*place) == source_place && i.results == [EntityId::Loan(source_loan)] => {
                    begins += 1
                }
                Operation::BorrowCall { source, .. }
                    if *source == source_loan
                        && i.results == [result_entity]
                        && i == &self.call_instruction =>
                {
                    source_calls += 1
                }
                _ => return None,
            }
        }
        if (literals, roots, begins, source_calls) != (1, 1, 1, 1) {
            return None;
        }
        let mut entities = std::collections::BTreeSet::new();
        for (index, id) in block.parameters.iter().enumerate() {
            if !entities.insert(*id)
                || f.entity(*id)?.definition
                    != (crate::ssa::model::Definition::BlockParameter {
                        block: block.id,
                        index,
                    })
            {
                return None;
            }
        }
        for instruction in &f.instructions {
            for (index, id) in instruction.results.iter().enumerate() {
                if !entities.insert(*id)
                    || f.entity(*id)?.definition
                        != (crate::ssa::model::Definition::InstructionResult {
                            instruction: instruction.id,
                            index,
                        })
                {
                    return None;
                }
            }
        }
        (entities.len() == f.values.len() + f.places.len() + f.loans.len()
            && entities.iter().all(|e| e.function() == f.id))
        .then_some(())
    }
}
