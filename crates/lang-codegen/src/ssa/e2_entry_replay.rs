//! Independent source entry replay. Reads actual instruction results; does not rerun emission.
use super::{e2_entry::EmittedEntry, model::*};
use lang_frontend::{
    ownership_checking::{
        UnitDropTarget,
        two_source_experiment::{TwoSourceCounterFact, TwoSourceExperiment},
    },
    type_checking::{UnitCallTarget, ValidatedCompilationUnitTypes},
};
struct Cursor<'a> {
    f: &'a Function,
    ids: &'a [InstructionId],
    index: usize,
}
impl Cursor<'_> {
    fn step(
        &mut self,
        op: Operation,
        origin: Origin,
        ty: Option<EntityType>,
    ) -> Option<Option<EntityId>> {
        let i = self.f.instruction(*self.ids.get(self.index)?)?;
        self.index += 1;
        if i.operation != op || i.origin != origin {
            return None;
        }
        match ty {
            Some(ty) => {
                let [id] = i.results.as_slice() else {
                    return None;
                };
                if self.f.entity(*id)?.ty != ty {
                    return None;
                }
                Some(Some(*id))
            }
            None => {
                if !i.results.is_empty() {
                    return None;
                }
                Some(None)
            }
        }
    }
}
fn value(e: Option<EntityId>) -> Option<ValueId> {
    if let Some(EntityId::Value(v)) = e {
        Some(v)
    } else {
        None
    }
}
fn place(e: Option<EntityId>) -> Option<PlaceId> {
    if let Some(EntityId::Place(v)) = e {
        Some(v)
    } else {
        None
    }
}
fn loan(e: Option<EntityId>) -> Option<LoanId> {
    if let Some(EntityId::Loan(v)) = e {
        Some(v)
    } else {
        None
    }
}
pub(super) fn replay(
    emitted: &EmittedEntry<'_>,
    experiment: &TwoSourceExperiment,
    fact: &TwoSourceCounterFact,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<()> {
    let d = emitted.descriptor();
    if !experiment.matches_types(typed)
        || !experiment.owns_entry(d)
        || d.instance().target() != UnitCallTarget::Declaration(experiment.callable(fact)?)
    {
        return None;
    }
    super::e2_replay::replay_closure(
        &emitted.function,
        experiment,
        fact,
        typed,
        Some(emitted.entry),
    )?;
    replay_body(
        &emitted.function.fragments.leaves.program.modules[0],
        emitted.entry,
        emitted.function.caller,
        d,
    )
}
/// Independent real entry instruction/cleanup replay; no emitter reconstruction.
pub(super) fn replay_body(
    module: &Module,
    entry: FunctionId,
    caller: FunctionId,
    d: &lang_frontend::ownership_checking::two_source_experiment::EntryDescriptor,
) -> Option<()> {
    replay_declared_body(module, entry, caller, d, d.body())
}
pub(super) fn replay_declared_body(
    module: &Module,
    entry: FunctionId,
    caller: FunctionId,
    d: &lang_frontend::ownership_checking::two_source_experiment::EntryDescriptor,
    declaration: lang_frontend::source::Span,
) -> Option<()> {
    let find = |kind: &SsaTypeKind| {
        module
            .types
            .iter()
            .position(|t| t == kind)
            .map(|index| SsaTypeId {
                module: module.id,
                index,
            })
    };
    let string = find(&SsaTypeKind::StringOwner)?;
    let int = find(&SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    })?;
    let boolean = find(&SsaTypeKind::Boolean)?;
    let f = module.function(entry)?;
    if f.receiver.is_some()
        || f.carrier_return.is_some()
        || f.never_returns
        || f.origin != Origin::Source(declaration)
        || !f.return_types.is_empty()
        || f.borrow_return.is_some()
    {
        return None;
    }
    let [block] = f.blocks.as_slice() else {
        return None;
    };
    if !block.parameters.is_empty()
        || block.origin != Origin::Source(declaration)
        || block.instructions.len() != 19
        || f.instructions.len() != 19
    {
        return None;
    }
    let t = block.terminator.as_ref()?;
    if t.abort.is_some()
        || t.kind != (TerminatorKind::Return { values: vec![] })
        || t.origin != Origin::Source(d.body())
    {
        return None;
    }
    let mut cursor = Cursor {
        f,
        ids: &block.instructions,
        index: 0,
    };
    let mut roots = Vec::new();
    for (i, (ty, c)) in [
        (int, ScalarConstant::Integer(d.rounds().into())),
        (boolean, ScalarConstant::Boolean(d.choose())),
    ]
    .into_iter()
    .enumerate()
    {
        let origin = Origin::Source(d.initializer_spans()[i]);
        let owner = value(cursor.step(
            Operation::Constant(c),
            origin.clone(),
            Some(EntityType::Value(ty)),
        )?)?;
        roots.push(place(cursor.step(
            Operation::RootPlace { owner },
            origin,
            Some(EntityType::Place(ty)),
        )?)?);
    }
    let mut owners = Vec::new();
    let mut loans = Vec::new();
    for (i, c) in d.contracts().iter().enumerate() {
        let origin = Origin::Source(c.argument_span());
        let ty = [string, string, int, boolean][i];
        let root = if i < 2 {
            let owner = value(cursor.step(
                Operation::StringLiteral {
                    string,
                    bytes: d.strings()[i].clone(),
                },
                origin.clone(),
                Some(EntityType::Value(string)),
            )?)?;
            owners.push(owner);
            place(cursor.step(
                Operation::RootPlace { owner },
                origin.clone(),
                Some(EntityType::Place(string)),
            )?)?
        } else {
            roots[i - 2]
        };
        loans.push(loan(cursor.step(
            Operation::BorrowBegin {
                place: root,
                kind: LoanKind::Shared,
            },
            origin,
            Some(EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            }),
        )?)?);
    }
    let origin = Origin::Source(d.contracts()[0].call_span());
    cursor.step(
        Operation::DirectCall {
            receiver: None,
            callee: caller,
            arguments: loans.iter().copied().map(EntityId::Loan).collect(),
        },
        origin.clone(),
        None,
    )?;
    for end in d.ends() {
        let i = d
            .contracts()
            .iter()
            .position(|c| c.argument() == end.loan().argument())?;
        cursor.step(
            Operation::BorrowEnd { loan: loans[i] },
            origin.clone(),
            None,
        )?;
    }
    for drop in d.drops() {
        let UnitDropTarget::Temporary(argument) = drop.target() else {
            return None;
        };
        let i = d.contracts()[..2]
            .iter()
            .position(|c| c.argument() == argument)?;
        cursor.step(Operation::Drop { owner: owners[i] }, origin.clone(), None)?;
    }
    (cursor.index == block.instructions.len()).then_some(())
}
