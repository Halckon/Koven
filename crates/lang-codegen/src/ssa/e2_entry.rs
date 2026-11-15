//! Complete source entry emission in the experiment; no production entry capability.
#[cfg(all(test, feature = "e2-two-source-test-support"))]
use super::e2_function;
use super::model::*;
use lang_frontend::ownership_checking::{
    UnitDropTarget,
    two_source_experiment::{EntryCallDescriptor, EntryDescriptor, EntryInitialization},
};
#[cfg(all(test, feature = "e2-two-source-test-support"))]
use lang_frontend::{
    ownership_checking::two_source_experiment::{TwoSourceCounterFact, TwoSourceExperiment},
    type_checking::{UnitCallTarget, ValidatedCompilationUnitTypes},
};
#[cfg(all(test, feature = "e2-two-source-test-support"))]
pub(super) struct EmittedEntry<'a> {
    pub(super) function: e2_function::EmittedFunction<'a>,
    pub(super) entry: FunctionId,
    descriptor: &'a EntryDescriptor,
}
#[cfg(all(test, feature = "e2-two-source-test-support"))]
impl EmittedEntry<'_> {
    pub(super) fn descriptor(&self) -> &EntryDescriptor {
        self.descriptor
    }
}
fn append(
    f: &mut Function,
    b: BlockId,
    op: Operation,
    ty: EntityType,
    origin: Origin,
) -> Option<EntityId> {
    let (_, results) = f.append_instruction(b, op, vec![ty], origin).ok()?;
    (results.len() == 1).then(|| results[0])
}
fn value(id: EntityId) -> Option<ValueId> {
    if let EntityId::Value(v) = id {
        Some(v)
    } else {
        None
    }
}
fn place(id: EntityId) -> Option<PlaceId> {
    if let EntityId::Place(v) = id {
        Some(v)
    } else {
        None
    }
}
fn loan(id: EntityId) -> Option<LoanId> {
    if let EntityId::Loan(v) = id {
        Some(v)
    } else {
        None
    }
}
#[cfg(all(test, feature = "e2-two-source-test-support"))]
pub(super) fn emit<'a>(
    experiment: &'a TwoSourceExperiment,
    fact: &'a TwoSourceCounterFact,
    descriptor: &'a EntryDescriptor,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<EmittedEntry<'a>> {
    if !experiment.matches_types(typed)
        || !experiment.owns_entry(descriptor)
        || descriptor.instance().target() != UnitCallTarget::Declaration(experiment.callable(fact)?)
    {
        return None;
    }
    let mut function = e2_function::emit(experiment, fact, typed)?;
    // Before adding the source entry, verify the unchanged source closure independently.
    super::e2_replay::replay(&function, experiment, fact, typed)?;
    let entry = append_entry(
        &mut function.fragments.leaves.program.modules[0],
        function.caller,
        descriptor,
    )?;
    super::verify::verify_program(&function.fragments.leaves.program).ok()?;
    Some(EmittedEntry {
        function,
        entry,
        descriptor,
    })
}
/// Internal same-Program append; the caller owns descriptor/caller identity validation.
#[cfg(all(test, feature = "e2-two-source-test-support"))]
pub(super) fn append_entry(
    module: &mut Module,
    caller: FunctionId,
    descriptor: &EntryDescriptor,
) -> Option<FunctionId> {
    let entry = module
        .add_function(
            format!("e2_source_entry_{}", module.functions.len()),
            vec![],
            Origin::Source(descriptor.body()),
        )
        .ok()?;
    module
        .function_mut(entry)?
        .add_block(vec![], Origin::Source(descriptor.body()))
        .ok()?;
    fill_entry(module, entry, caller, descriptor)
}
/// Preserve a predeclared source identity and its real declaration/entry origins.
pub(super) fn fill_entry(
    module: &mut Module,
    entry: FunctionId,
    caller: FunctionId,
    descriptor: &EntryDescriptor,
) -> Option<FunctionId> {
    let block = empty_declaration(module.function(entry)?, &[])?;
    for call in descriptor.calls() {
        emit_call(module, entry, caller, call)?;
    }
    module
        .function_mut(entry)?
        .set_terminator(
            block,
            TerminatorKind::Return { values: vec![] },
            Origin::Source(descriptor.body()),
        )
        .ok()?;
    Some(entry)
}
fn emit_call(
    module: &mut Module,
    entry: FunctionId,
    caller: FunctionId,
    descriptor: &EntryCallDescriptor,
) -> Option<()> {
    let string = module.intern_type(SsaTypeKind::StringOwner);
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let f = module.function_mut(entry)?;
    let block = *f.blocks.first().map(|b| &b.id)?;
    let mut owners = Vec::new();
    let mut named_roots = Vec::new();
    let mut scalar_places = [None; 2];
    let named = descriptor.named_strings();
    for initialization in descriptor.initialization_order() {
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
                let owner = value(append(
                    f,
                    block,
                    Operation::StringLiteral {
                        string,
                        bytes: bytes.to_vec(),
                    },
                    EntityType::Value(string),
                    origin.clone(),
                )?)?;
                owners.push(owner);
                let root = place(append(
                    f,
                    block,
                    Operation::RootPlace { owner },
                    EntityType::Place(string),
                    origin,
                )?)?;
                named_roots.push((*symbol, root));
            }
            EntryInitialization::Rounds | EntryInitialization::Choose => {
                let i = usize::from(matches!(initialization, EntryInitialization::Choose));
                if scalar_places[i].is_some() {
                    return None;
                }
                let (ty, constant) = if i == 0 {
                    (int, ScalarConstant::Integer(descriptor.rounds().into()))
                } else {
                    (boolean, ScalarConstant::Boolean(descriptor.choose()))
                };
                let origin = Origin::Source(descriptor.initializer_spans()[i]);
                let owner = value(append(
                    f,
                    block,
                    Operation::Constant(constant),
                    EntityType::Value(ty),
                    origin.clone(),
                )?)?;
                scalar_places[i] = Some(place(append(
                    f,
                    block,
                    Operation::RootPlace { owner },
                    EntityType::Place(ty),
                    origin,
                )?)?);
            }
        }
    }
    if named_roots.len() != named.len() {
        return None;
    }
    let mut temporary_owners = Vec::new();
    let mut loans = Vec::new();
    for (i, c) in descriptor.contracts().iter().enumerate() {
        let role = descriptor.argument_role(i)?;
        let ty = *[string, string, int, boolean].get(role)?;
        let origin = Origin::Source(c.argument_span());
        let root = if let Some(symbol) = descriptor.named_argument_root(i) {
            let mut matches = named_roots
                .iter()
                .filter(|(candidate, _)| *candidate == symbol);
            let root = matches.next()?.1;
            if matches.next().is_some() {
                return None;
            }
            root
        } else if role < 2 {
            let owner = value(append(
                f,
                block,
                Operation::StringLiteral {
                    string,
                    bytes: descriptor.temporary_string(i)?.to_vec(),
                },
                EntityType::Value(string),
                origin.clone(),
            )?)?;
            temporary_owners.push((c.argument(), owners.len()));
            owners.push(owner);
            place(append(
                f,
                block,
                Operation::RootPlace { owner },
                EntityType::Place(string),
                origin.clone(),
            )?)?
        } else if descriptor.temporary_scalar_argument(i) == Some(c.argument()) {
            if scalar_places[role - 2].is_some() {
                return None;
            }
            let constant = if role == 2 {
                ScalarConstant::Integer(descriptor.rounds().into())
            } else {
                ScalarConstant::Boolean(descriptor.choose())
            };
            let owner = value(append(
                f,
                block,
                Operation::Constant(constant),
                EntityType::Value(ty),
                origin.clone(),
            )?)?;
            let root = place(append(
                f,
                block,
                Operation::RootPlace { owner },
                EntityType::Place(ty),
                origin.clone(),
            )?)?;
            scalar_places[role - 2] = Some(root);
            root
        } else {
            (*scalar_places.get(role - 2)?)?
        };
        loans.push(loan(append(
            f,
            block,
            Operation::BorrowBegin {
                place: root,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            },
            origin,
        )?)?);
    }
    if scalar_places.iter().any(Option::is_none) {
        return None;
    }
    // Borrow evaluation stays in source order; only the call delivery uses ABI order.
    let mut arguments = [None; 4];
    for (contract, loan) in descriptor.contracts().iter().zip(&loans) {
        if arguments
            .get_mut(contract.parameter_index())?
            .replace(EntityId::Loan(*loan))
            .is_some()
        {
            return None;
        }
    }
    let arguments = arguments.into_iter().collect::<Option<Vec<_>>>()?;
    let call_origin = Origin::Source(descriptor.contracts()[0].call_span());
    f.append_instruction(
        block,
        Operation::DirectCall {
            receiver: None,
            callee: caller,
            arguments,
        },
        vec![],
        call_origin.clone(),
    )
    .ok()?;
    for end in descriptor.ends() {
        let i = descriptor
            .contracts()
            .iter()
            .position(|c| c.argument() == end.loan().argument())?;
        f.append_instruction(
            block,
            Operation::BorrowEnd { loan: loans[i] },
            vec![],
            call_origin.clone(),
        )
        .ok()?;
    }
    let mut dropped = vec![false; owners.len()];
    for drop in descriptor.drops() {
        let (index, origin) = match drop.target() {
            UnitDropTarget::Temporary(argument) => {
                let mut matches = temporary_owners
                    .iter()
                    .filter(|(candidate, _)| *candidate == argument);
                let i = matches.next()?.1;
                if matches.next().is_some() {
                    return None;
                }
                (i, call_origin.clone())
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
        f.append_instruction(block, Operation::Drop { owner }, vec![], origin)
            .ok()?;
    }
    if dropped.iter().any(|consumed| !consumed) {
        return None;
    }
    Some(())
}

/// ABI and empty-body check only; caller must separately prove same-analysis source ownership.
pub(super) fn empty_declaration(f: &Function, parameters: &[EntityType]) -> Option<BlockId> {
    let [block] = f.blocks.as_slice() else {
        return None;
    };
    if !f.return_types.is_empty()
        || f.never_returns
        || f.borrow_return.is_some()
        || f.carrier_return.is_some()
        || f.receiver.is_some()
        || !f.instructions.is_empty()
        || !block.instructions.is_empty()
        || block.terminator.is_some()
        || block.parameters.len() != parameters.len()
        || f.values.len() + f.places.len() + f.loans.len() != parameters.len()
    {
        return None;
    }
    for (entity, expected) in block.parameters.iter().zip(parameters) {
        if f.entity(*entity)?.ty != *expected {
            return None;
        }
    }
    Some(block.id)
}
