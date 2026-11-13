//! Independent cross-statement binding verification; existing producers own every effect.
mod emitted {
    use super::*;
    #[cfg(test)]
    mod ci_tests {
        //! CI-only corruption controls. Local checks compile this module without executing it.
        use super::*;
        use crate::ssa::unit_lower_test_support;
        use lang_frontend::{
            name_resolution::SourceUnitInput, ownership_checking::UnitLoanActionObservation,
            source::SourceMap, type_checking::standard_environments,
        };
        use std::cell::RefCell;
        #[derive(Clone)]
        struct Snapshot {
            plans: Plans,
            function: Function,
            receipt: Receipt,
            span: Span,
        }
        thread_local! { static SNAPSHOTS: RefCell<Option<Vec<Snapshot>>> = const { RefCell::new(None) }; }
        pub(super) fn record(plans: &Plans, function: &Function, receipts: &[Receipt], span: Span) {
            SNAPSHOTS.with(|output| {
                if let Some(output) = output.borrow_mut().as_mut() {
                    for receipt in receipts {
                        output.push(Snapshot {
                            plans: plans.clone(),
                            function: function.clone(),
                            receipt: receipt.clone(),
                            span,
                        });
                    }
                }
            });
        }
        fn capture() -> (Snapshot, UnitLoanActionObservation) {
            let mut sources = SourceMap::new();
            let (source, parsed) = unit_lower_test_support::parsed(
                &mut sources,
                "ci-binding.ko",
                "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); consume(item) }",
            );
            let inputs = [SourceUnitInput::memory_document(source, &parsed)];
            let (environment, types) = standard_environments();
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            SNAPSHOTS.with(|output| *output.borrow_mut() = Some(Vec::new()));
            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
            .unwrap();
            let mut output = SNAPSHOTS.with(|output| output.borrow_mut().take().unwrap());
            assert_eq!(output.len(), 1);
            (
                output.pop().unwrap(),
                owned.ownership().loan_action_observations()[0].clone(),
            )
        }
        fn replay(
            snapshot: &Snapshot,
            count: usize,
            rebind: bool,
        ) -> Result<Emitted, LoweringError> {
            let mut emitted = Emitted::new(&snapshot.plans, &snapshot.function, snapshot.span)?;
            let p = &snapshot.plans.entries[0];
            let mut current: BTreeMap<_, _> = [p.owner, p.place, p.source, p.result]
                .into_iter()
                .map(Option::unwrap)
                .zip(snapshot.receipt.entities.iter().copied())
                .collect();
            for (role, id) in snapshot.receipt.instructions.iter().take(count).enumerate() {
                if rebind && role == 4 {
                    current.insert(p.source.unwrap(), snapshot.receipt.entities[3]);
                }
                emitted.statement(
                    &snapshot.plans,
                    Tag {
                        plan: 0,
                        role: u8::try_from(role).unwrap(),
                    },
                    &snapshot.function,
                    *id,
                    &current,
                    snapshot.span,
                )?;
            }
            Ok(emitted)
        }
        #[test]
        fn ci_only_binding_rejects_foreign_catalog_and_action_ids() {
            let (snapshot, first) = capture();
            let (_, foreign) = capture();
            let region = &first.binding_regions()[0];
            let ids = region.action_ids().collect::<Vec<_>>();
            assert!(
                crate::ssa::refine::receiver_receipt::admission::catalog_actions(
                    &first,
                    region,
                    &ids,
                    snapshot.span
                )
                .is_ok()
            );
            assert!(
                crate::ssa::refine::receiver_receipt::admission::catalog_actions(
                    &foreign,
                    region,
                    &ids,
                    snapshot.span
                )
                .is_err()
            );
            let foreign_ids = foreign.binding_regions()[0]
                .action_ids()
                .collect::<Vec<_>>();
            assert!(
                crate::ssa::refine::receiver_receipt::admission::catalog_actions(
                    &first,
                    region,
                    &foreign_ids,
                    snapshot.span
                )
                .is_err()
            );
        }
        #[test]
        fn ci_only_binding_rejects_missing_completion_stage_and_rebound_entity() {
            let (snapshot, _) = capture();
            replay(&snapshot, 7, false)
                .unwrap()
                .finish(&snapshot.plans, &snapshot.function, snapshot.span)
                .unwrap();
            assert!(
                replay(&snapshot, 6, false)
                    .unwrap()
                    .finish(&snapshot.plans, &snapshot.function, snapshot.span)
                    .is_err()
            );
            assert!(replay(&snapshot, 7, true).is_err());
            let mut missing = snapshot.clone();
            missing.plans.entries[0].completed = false;
            assert!(Emitted::new(&missing.plans, &missing.function, missing.span).is_err());
            missing.plans.entries[0].completed = true;
            missing.plans.entries[0].stage = 6;
            assert!(Emitted::new(&missing.plans, &missing.function, missing.span).is_err());
        }
        #[test]
        fn ci_only_binding_rejects_wrong_consumer_and_retirement_identity() {
            let (snapshot, _) = capture();
            let mut plans = snapshot.plans.clone();
            let p = plans.entries[0].clone();
            plans.entries[0].stage = 3;
            plans.entries[0].consumer_authorized = false;
            assert!(
                plans
                    .consumer_argument(
                        p.consumer_call,
                        p.consumer_argument,
                        p.root,
                        p.result.unwrap(),
                        snapshot.span
                    )
                    .is_err()
            );
            plans.entries[0].stage = 4;
            plans.entries[0].retired = false;
            assert!(
                plans
                    .retire(
                        p.binding,
                        p.end_point,
                        p.source,
                        Some(p.source.as_slice()),
                        snapshot.span
                    )
                    .is_err()
            );
            assert!(
                plans
                    .retire(p.binding, p.end_point, p.result, Some(&[]), snapshot.span)
                    .is_err()
            );
        }
    }
    use crate::ssa::model::{BlockId, EntityId, Function, InstructionId, Operation};

    #[derive(Clone, Debug)]
    pub(crate) struct Receipt {
        pub(crate) ids: Vec<UnitLoanActionId>,
        pub(crate) callable: UnitItemId,
        pub(crate) caller: FunctionId,
        pub(crate) binding: UnitSymbolId,
        pub(crate) root: UnitSymbolId,
        pub(crate) source_call: UnitExpressionId,
        pub(crate) consumer_call: UnitExpressionId,
        pub(crate) entry_statement: UnitStatementId,
        pub(crate) completion_statement: UnitStatementId,
        pub(crate) instructions: Vec<InstructionId>,
        pub(crate) entities: Vec<EntityId>,
    }
    pub(in crate::ssa::refine) struct Emitted {
        receipts: Vec<Receipt>,
        blocks: Vec<Option<BlockId>>,
    }
    impl Emitted {
        pub(in crate::ssa::refine) fn new(
            plans: &Plans,
            function: &Function,
            span: Span,
        ) -> Result<Self, LoweringError> {
            let mut receipts = Vec::new();
            for p in &plans.entries {
                if p.stage != 7
                    || !p.completed
                    || !p.retired
                    || !p.committed
                    || !p.consumer_authorized
                    || !p.drop_authorized
                    || !p.activated
                    || !p.source_authorized
                    || p.caller != function.id()
                {
                    return Err(invalid(span));
                }
                receipts.push(Receipt {
                    ids: p.ids.clone(),
                    callable: p.callable,
                    caller: p.caller,
                    binding: p.binding,
                    root: p.root,
                    source_call: p.source_call,
                    consumer_call: p.consumer_call,
                    entry_statement: p.entry_statement,
                    completion_statement: p.completion_statement,
                    instructions: Vec::new(),
                    entities: Vec::new(),
                });
            }
            Ok(Self {
                blocks: vec![None; receipts.len()],
                receipts,
            })
        }
        fn frozen(
            p: &Plan,
            r: &Receipt,
            current: &BTreeMap<Slot, EntityId>,
            span: Span,
        ) -> Result<(), LoweringError> {
            for (slot, frozen) in [p.owner, p.place, p.source, p.result]
                .into_iter()
                .zip(&r.entities)
            {
                if slot.and_then(|s| current.get(&s)) != Some(frozen) {
                    return Err(invalid(span));
                }
            }
            Ok(())
        }
        pub(in crate::ssa::refine) fn alias(
            &self,
            plans: &Plans,
            current: &BTreeMap<Slot, EntityId>,
            span: Span,
        ) -> Result<(), LoweringError> {
            for (p, r) in plans.entries.iter().zip(&self.receipts) {
                Self::frozen(p, r, current, span)?;
            }
            Ok(())
        }
        pub(in crate::ssa::refine) fn statement(
            &mut self,
            plans: &Plans,
            tag: Tag,
            function: &Function,
            instruction: InstructionId,
            current: &BTreeMap<Slot, EntityId>,
            span: Span,
        ) -> Result<(), LoweringError> {
            let p = plans.entries.get(tag.plan).ok_or_else(|| invalid(span))?;
            let r = self
                .receipts
                .get_mut(tag.plan)
                .ok_or_else(|| invalid(span))?;
            let data = function
                .instruction(instruction)
                .ok_or_else(|| invalid(span))?;
            if r.caller != function.id()
                || r.instructions.len() != usize::from(tag.role)
                || self.blocks[tag.plan].is_some_and(|b| b != data.block)
            {
                return Err(invalid(span));
            }
            Self::frozen(p, r, current, span)?;
            let entity = |slot: Option<Slot>| {
                slot.and_then(|s| current.get(&s).copied())
                    .ok_or_else(|| invalid(span))
            };
            match (tag.role, &data.operation) {
                (0, Operation::RootPlace { owner })
                    if EntityId::Value(*owner) == entity(p.owner)? =>
                {
                    let [place @ EntityId::Place(_)] = data.results.as_slice() else {
                        return Err(invalid(span));
                    };
                    if *place != entity(p.place)? || owner.function() != function.id() {
                        return Err(invalid(span));
                    }
                    r.entities.extend([EntityId::Value(*owner), *place]);
                }
                (
                    1,
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                ) if Some(&EntityId::Place(*place)) == r.entities.get(1) => {
                    let [loan @ EntityId::Loan(id)] = data.results.as_slice() else {
                        return Err(invalid(span));
                    };
                    if *loan != entity(p.source)? || id.function() != function.id() {
                        return Err(invalid(span));
                    }
                    r.entities.push(*loan);
                }
                (
                    2,
                    Operation::BorrowCall {
                        callee,
                        arguments,
                        source,
                    },
                ) if *callee == p.source_callee
                    && EntityId::Loan(*source) == entity(p.source)?
                    && arguments.as_slice() == [entity(p.source)?] =>
                {
                    let [loan @ EntityId::Loan(id)] = data.results.as_slice() else {
                        return Err(invalid(span));
                    };
                    if *loan != entity(p.result)?
                        || *loan == entity(p.source)?
                        || id.function() != function.id()
                    {
                        return Err(invalid(span));
                    }
                    r.entities.push(*loan);
                }
                (
                    3,
                    Operation::DirectCall {
                        callee,
                        receiver: None,
                        arguments,
                    },
                ) if *callee == p.consumer_callee
                    && arguments.as_slice() == [entity(p.result)?]
                    && data.results.is_empty() => {}
                (4, Operation::BorrowEnd { loan })
                    if EntityId::Loan(*loan) == entity(p.result)? && data.results.is_empty() => {}
                (5, Operation::BorrowEnd { loan })
                    if EntityId::Loan(*loan) == entity(p.source)? && data.results.is_empty() => {}
                (6, Operation::Drop { owner })
                    if EntityId::Value(*owner) == entity(p.owner)? && data.results.is_empty() => {}
                _ => return Err(invalid(span)),
            }
            self.blocks[tag.plan] = Some(data.block);
            r.instructions.push(instruction);
            Ok(())
        }
        pub(in crate::ssa::refine) fn finish(
            self,
            plans: &Plans,
            function: &Function,
            span: Span,
        ) -> Result<(), LoweringError> {
            if self.receipts.len() != plans.entries.len()
                || self.blocks.len() != plans.entries.len()
            {
                return Err(invalid(span));
            }
            for ((r, b), p) in self.receipts.iter().zip(&self.blocks).zip(&plans.entries) {
                if !p.completed
                    || p.stage != 7
                    || r.ids != p.ids
                    || r.callable != p.callable
                    || r.caller != function.id()
                    || r.binding != p.binding
                    || r.root != p.root
                    || r.source_call != p.source_call
                    || r.consumer_call != p.consumer_call
                    || r.entry_statement != p.entry_statement
                    || r.completion_statement != p.completion_statement
                    || r.instructions.len() != 7
                    || r.entities.len() != 4
                {
                    return Err(invalid(span));
                }
                let block = b
                    .and_then(|b| function.block(b))
                    .ok_or_else(|| invalid(span))?;
                if !block
                    .instructions
                    .windows(7)
                    .any(|ids| ids == r.instructions.as_slice())
                {
                    return Err(invalid(span));
                }
            }
            #[cfg(test)]
            ci_tests::record(plans, function, &self.receipts, span);
            #[cfg(test)]
            RECEIPTS.with(|output| {
                if let Some(output) = output.borrow_mut().as_mut() {
                    output.extend(self.receipts);
                }
            });
            Ok(())
        }
    }
}
use super::body::{BlockRef, ROp, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{FunctionId, LoanKind},
};
pub(super) use emitted::Emitted;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{BorrowSourceLoan, UnitDropPoint, UnitDropTarget, UnitLoanActionId},
    source::Span,
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub(super) struct Tag {
    pub(super) plan: usize,
    pub(super) role: u8,
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    pub(in crate::ssa::refine) entries: Vec<Plan>,
}
#[derive(Clone, Debug)]
pub(in crate::ssa::refine) struct Plan {
    pub(in crate::ssa::refine) ids: Vec<UnitLoanActionId>,
    pub(in crate::ssa::refine) callable: UnitItemId,
    pub(in crate::ssa::refine) caller: FunctionId,
    pub(in crate::ssa::refine) binding: UnitSymbolId,
    pub(in crate::ssa::refine) root: UnitSymbolId,
    pub(in crate::ssa::refine) source_call: UnitExpressionId,
    pub(in crate::ssa::refine) source_argument: UnitExpressionId,
    pub(in crate::ssa::refine) consumer_call: UnitExpressionId,
    pub(in crate::ssa::refine) consumer_argument: UnitExpressionId,
    pub(in crate::ssa::refine) entry_statement: UnitStatementId,
    pub(in crate::ssa::refine) completion_statement: UnitStatementId,
    pub(in crate::ssa::refine) end_point: UnitDropPoint,
    pub(in crate::ssa::refine) source_callee: FunctionId,
    pub(in crate::ssa::refine) consumer_callee: FunctionId,
    pub(in crate::ssa::refine) drop_index: usize,
    pub(in crate::ssa::refine) owner: Option<Slot>,
    pub(in crate::ssa::refine) place: Option<Slot>,
    pub(in crate::ssa::refine) source: Option<Slot>,
    pub(in crate::ssa::refine) result: Option<Slot>,
    pub(in crate::ssa::refine) stage: u8,
    pub(in crate::ssa::refine) activated: bool,
    pub(in crate::ssa::refine) source_authorized: bool,
    pub(in crate::ssa::refine) committed: bool,
    pub(in crate::ssa::refine) consumer_authorized: bool,
    pub(in crate::ssa::refine) retired: bool,
    pub(in crate::ssa::refine) drop_authorized: bool,
    pub(in crate::ssa::refine) completed: bool,
    pub(in crate::ssa::refine) block: Option<BlockRef>,
}
impl Plans {
    pub(super) fn has_entry(&self, statement: UnitStatementId) -> bool {
        self.entries.iter().any(|p| p.entry_statement == statement)
    }
    pub(super) fn has_completion(&self, statement: UnitStatementId) -> bool {
        self.entries
            .iter()
            .any(|p| p.completion_statement == statement)
    }
    pub(super) fn has_consumer(&self, call: UnitExpressionId) -> bool {
        self.entries.iter().any(|p| p.consumer_call == call)
    }
    pub(super) fn enter(
        &mut self,
        statement: UnitStatementId,
        bindings: &BTreeMap<UnitSymbolId, Slot>,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.entry_statement != statement {
                continue;
            }
            if p.owner.is_some() || p.stage != 0 || bindings.len() != 1 {
                return Err(invalid(span));
            }
            p.owner = Some(*bindings.get(&p.root).ok_or_else(|| invalid(span))?);
            p.block = Some(block);
        }
        Ok(())
    }
    pub(super) fn activate(
        &mut self,
        binding: UnitSymbolId,
        initializer: UnitExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.binding != binding {
                continue;
            }
            if p.owner.is_none()
                || p.activated
                || p.stage != 0
                || initializer != p.source_call
                || source.call() != p.source_call
                || source.argument() != p.source_argument
            {
                return Err(invalid(span));
            }
            p.activated = true;
        }
        Ok(())
    }
    pub(super) fn source_operand(
        &mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.source_call != call {
                continue;
            }
            if !p.activated || p.source_authorized || p.stage != 0 || argument != p.source_argument
            {
                return Err(invalid(span));
            }
            p.source_authorized = true;
        }
        Ok(())
    }
    pub(super) fn commit(
        &mut self,
        binding: UnitSymbolId,
        borrowed: &BTreeMap<UnitSymbolId, Slot>,
        sources: &BTreeMap<UnitSymbolId, Vec<Slot>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.binding != binding {
                continue;
            }
            if p.stage != 3
                || p.committed
                || borrowed.get(&binding).copied() != p.result
                || sources.get(&binding).map(Vec::as_slice) != Some(p.source.as_slice())
            {
                return Err(invalid(span));
            }
            p.committed = true;
        }
        Ok(())
    }
    pub(super) fn consumer_argument(
        &mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        binding: UnitSymbolId,
        loan: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.consumer_call != call {
                continue;
            }
            if p.stage != 3
                || !p.committed
                || p.consumer_authorized
                || binding != p.binding
                || argument != p.consumer_argument
                || Some(loan) != p.result
            {
                return Err(invalid(span));
            }
            p.consumer_authorized = true;
        }
        Ok(())
    }
    pub(super) fn retire(
        &mut self,
        binding: UnitSymbolId,
        point: UnitDropPoint,
        result: Option<Slot>,
        sources: Option<&[Slot]>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.binding != binding {
                continue;
            }
            if p.stage != 4
                || p.retired
                || point != p.end_point
                || result != p.result
                || sources != Some(p.source.as_slice())
            {
                return Err(invalid(span));
            }
            p.retired = true;
        }
        Ok(())
    }
    pub(super) fn drop_fact(
        &mut self,
        index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.drop_index != index {
                continue;
            }
            if p.stage != 6
                || !p.retired
                || p.drop_authorized
                || p.owner != Some(slot)
                || target != UnitDropTarget::Named(p.root)
            {
                return Err(invalid(span));
            }
            p.drop_authorized = true;
        }
        Ok(())
    }
    pub(super) fn complete(
        &mut self,
        statement: UnitStatementId,
        bindings: &BTreeMap<UnitSymbolId, Slot>,
        dead: &BTreeSet<UnitSymbolId>,
        consumed: &BTreeSet<usize>,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.completion_statement != statement {
                continue;
            }
            if p.completed
                || p.stage != 7
                || p.block != Some(block)
                || bindings.get(&p.root).copied() != p.owner
                || !dead.contains(&p.root)
                || !consumed.contains(&p.drop_index)
            {
                return Err(invalid(span));
            }
            p.completed = true;
        }
        Ok(())
    }
    /// Stamp only the seven real operations during the exact entered statement interval.
    pub(super) fn statement(
        &mut self,
        op: &ROp,
        results: &[Slot],
        block: BlockRef,
        span: Span,
    ) -> Result<Option<Tag>, LoweringError> {
        let mut tag = None;
        for (index, p) in self.entries.iter_mut().enumerate() {
            if p.owner.is_none() || p.completed {
                continue;
            }
            if tag.is_some() || p.block != Some(block) || !p.activated || !p.source_authorized {
                return Err(invalid(span));
            }
            let single = || match results {
                [s] => Ok(*s),
                _ => Err(invalid(span)),
            };
            match (p.stage, op) {
                (0, ROp::RootPlace { owner }) if Some(*owner) == p.owner => {
                    p.place = Some(single()?)
                }
                (
                    1,
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                ) if Some(*place) == p.place => p.source = Some(single()?),
                (
                    2,
                    ROp::BorrowCall {
                        callee,
                        arguments,
                        source,
                    },
                ) if *callee == p.source_callee
                    && Some(*source) == p.source
                    && arguments.as_slice() == p.source.as_slice() =>
                {
                    p.result = Some(single()?);
                    if p.result == p.source {
                        return Err(invalid(span));
                    }
                }
                (
                    3,
                    ROp::DirectCall {
                        callee,
                        receiver: None,
                        arguments,
                    },
                ) if p.committed
                    && p.consumer_authorized
                    && *callee == p.consumer_callee
                    && arguments.as_slice() == p.result.as_slice()
                    && results.is_empty() => {}
                (4, ROp::BorrowEnd { loan })
                    if p.retired && Some(*loan) == p.result && results.is_empty() => {}
                (5, ROp::BorrowEnd { loan })
                    if p.retired && Some(*loan) == p.source && results.is_empty() => {}
                (6, ROp::Drop { owner })
                    if p.drop_authorized && Some(*owner) == p.owner && results.is_empty() => {}
                _ => return Err(invalid(span)),
            }
            tag = Some(Tag {
                plan: index,
                role: p.stage,
            });
            p.stage += 1;
        }
        Ok(tag)
    }
    pub(super) fn finish(
        &self,
        consumed: &BTreeSet<usize>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self
            .entries
            .iter()
            .any(|p| p.stage != 7 || !p.completed || !consumed.contains(&p.drop_index))
        {
            return Err(invalid(span));
        }
        Ok(())
    }
}
fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}
#[cfg(test)]
thread_local! { static RECEIPTS: std::cell::RefCell<Option<Vec<emitted::Receipt>>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<emitted::Receipt>) {
    RECEIPTS.with(|r| {
        assert!(r.borrow().is_none());
        *r.borrow_mut() = Some(Vec::new());
    });
    let result = lower();
    (result, RECEIPTS.with(|r| r.borrow_mut().take().unwrap()))
}
