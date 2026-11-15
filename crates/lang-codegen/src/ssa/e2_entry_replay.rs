//! Independent source entry replay. Reads actual instruction results; does not rerun emission.
#[cfg(all(test, feature = "e2-two-source-test-support"))]
use super::e2_entry::EmittedEntry;
use super::model::*;
use lang_frontend::ownership_checking::{
    UnitDropTarget, two_source_experiment::EntryInitialization,
};
#[cfg(all(test, feature = "e2-two-source-test-support"))]
use lang_frontend::{
    ownership_checking::two_source_experiment::{TwoSourceCounterFact, TwoSourceExperiment},
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
#[cfg(all(test, feature = "e2-two-source-test-support"))]
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
#[cfg(all(test, feature = "e2-two-source-test-support"))]
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
        || block.instructions.len()
            != match (
                d.named_strings().len(),
                (0..4).filter(|i| d.temporary_string(*i).is_some()).count(),
            ) {
                (1, 0) => 16,
                (0, 2) | (1, 1) | (2, 0) => 19,
                _ => return None,
            }
        || f.instructions.len() != block.instructions.len()
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
    let mut owners = Vec::new();
    let mut named_roots = Vec::new();
    let mut roots = [None; 2];
    let named = d.named_strings();
    for initialization in d.initialization_order() {
        match initialization {
            EntryInitialization::Named(symbol) => {
                if named_roots.iter().any(|(root, _)| root == symbol) {
                    return None;
                }
                let mut matches = named.iter().filter(|(root, _, _, _)| root == symbol);
                let (_, _, span, bytes) = matches.next()?;
                if matches.next().is_some() {
                    return None;
                }
                let origin = Origin::Source(*span);
                let owner = value(cursor.step(
                    Operation::StringLiteral {
                        string,
                        bytes: bytes.to_vec(),
                    },
                    origin.clone(),
                    Some(EntityType::Value(string)),
                )?)?;
                owners.push(owner);
                let root = place(cursor.step(
                    Operation::RootPlace { owner },
                    origin,
                    Some(EntityType::Place(string)),
                )?)?;
                named_roots.push((*symbol, root));
            }
            EntryInitialization::Rounds | EntryInitialization::Choose => {
                let i = usize::from(matches!(initialization, EntryInitialization::Choose));
                if roots[i].is_some() {
                    return None;
                }
                let (ty, constant) = if i == 0 {
                    (int, ScalarConstant::Integer(d.rounds().into()))
                } else {
                    (boolean, ScalarConstant::Boolean(d.choose()))
                };
                let origin = Origin::Source(d.initializer_spans()[i]);
                let owner = value(cursor.step(
                    Operation::Constant(constant),
                    origin.clone(),
                    Some(EntityType::Value(ty)),
                )?)?;
                roots[i] = Some(place(cursor.step(
                    Operation::RootPlace { owner },
                    origin,
                    Some(EntityType::Place(ty)),
                )?)?);
            }
        }
    }
    if named_roots.len() != named.len() || roots.iter().any(Option::is_none) {
        return None;
    }
    let mut temporary_owners = Vec::new();
    let mut loans = Vec::new();
    for (i, c) in d.contracts().iter().enumerate() {
        let origin = Origin::Source(c.argument_span());
        let role = d.argument_role(i)?;
        let ty = *[string, string, int, boolean].get(role)?;
        let root = if let Some(symbol) = d.named_argument_root(i) {
            let mut matches = named_roots
                .iter()
                .filter(|(candidate, _)| *candidate == symbol);
            let root = matches.next()?.1;
            if matches.next().is_some() {
                return None;
            }
            root
        } else if role < 2 {
            let owner = value(cursor.step(
                Operation::StringLiteral {
                    string,
                    bytes: d.temporary_string(i)?.to_vec(),
                },
                origin.clone(),
                Some(EntityType::Value(string)),
            )?)?;
            temporary_owners.push((c.argument(), owners.len()));
            owners.push(owner);
            place(cursor.step(
                Operation::RootPlace { owner },
                origin.clone(),
                Some(EntityType::Place(string)),
            )?)?
        } else {
            (*roots.get(role - 2)?)?
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
    // Borrow evaluation stays in source order; only the call delivery uses ABI order.
    let mut arguments = [None; 4];
    for (contract, loan) in d.contracts().iter().zip(&loans) {
        if arguments
            .get_mut(contract.parameter_index())?
            .replace(EntityId::Loan(*loan))
            .is_some()
        {
            return None;
        }
    }
    let arguments = arguments.into_iter().collect::<Option<Vec<_>>>()?;
    let origin = Origin::Source(d.contracts()[0].call_span());
    cursor.step(
        Operation::DirectCall {
            receiver: None,
            callee: caller,
            arguments,
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
    let mut dropped = vec![false; owners.len()];
    for drop in d.drops() {
        let (index, drop_origin) = match drop.target() {
            UnitDropTarget::Temporary(argument) => {
                let mut matches = temporary_owners
                    .iter()
                    .filter(|(candidate, _)| *candidate == argument);
                let i = matches.next()?.1;
                if matches.next().is_some() {
                    return None;
                }
                (i, origin.clone())
            }
            UnitDropTarget::Named(root) => {
                let i = named_roots.iter().position(|(symbol, _)| *symbol == root)?;
                (i, Origin::Source(drop.value_origin()))
            }
            _ => return None,
        };
        if *dropped.get(index)? {
            return None;
        }
        dropped[index] = true;
        let owner = *owners.get(index)?;
        cursor.step(Operation::Drop { owner }, drop_origin, None)?;
    }

    (cursor.index == block.instructions.len() && dropped.iter().all(|consumed| *consumed))
        .then_some(())
}
