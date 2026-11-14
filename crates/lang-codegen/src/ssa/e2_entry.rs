//! Complete source entry emission in the experiment; no production entry capability.
#[cfg(all(test, feature = "e2-two-source-test-support"))]
use super::e2_function;
use super::model::*;
use lang_frontend::ownership_checking::{UnitDropTarget, two_source_experiment::EntryDescriptor};
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
    let string = module.intern_type(SsaTypeKind::StringOwner);
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let f = module.function_mut(entry)?;
    let block = empty_declaration(f, &[])?;
    let mut owners = Vec::new();
    let named_root = if let Some((_, _, span, bytes)) = descriptor.named_string() {
        let origin = Origin::Source(span);
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
        Some(place(append(
            f,
            block,
            Operation::RootPlace { owner },
            EntityType::Place(string),
            origin,
        )?)?)
    } else {
        None
    };
    let mut scalar_places = Vec::new();
    for (i, (ty, constant)) in [
        (int, ScalarConstant::Integer(descriptor.rounds().into())),
        (boolean, ScalarConstant::Boolean(descriptor.choose())),
    ]
    .into_iter()
    .enumerate()
    {
        let origin = Origin::Source(descriptor.initializer_spans()[i]);
        let owner = value(append(
            f,
            block,
            Operation::Constant(constant),
            EntityType::Value(ty),
            origin.clone(),
        )?)?;
        scalar_places.push(place(append(
            f,
            block,
            Operation::RootPlace { owner },
            EntityType::Place(ty),
            origin,
        )?)?);
    }
    let mut loans = Vec::new();
    for (i, c) in descriptor.contracts().iter().enumerate() {
        let ty = [string, string, int, boolean][i];
        let origin = Origin::Source(c.argument_span());
        let root = if i < 2 && named_root.is_some() {
            named_root?
        } else if i < 2 {
            let owner = value(append(
                f,
                block,
                Operation::StringLiteral {
                    string,
                    bytes: descriptor.strings()?[i].clone(),
                },
                EntityType::Value(string),
                origin.clone(),
            )?)?;
            owners.push(owner);
            place(append(
                f,
                block,
                Operation::RootPlace { owner },
                EntityType::Place(string),
                origin.clone(),
            )?)?
        } else {
            scalar_places[i - 2]
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
    let call_origin = Origin::Source(descriptor.contracts()[0].call_span());
    f.append_instruction(
        block,
        Operation::DirectCall {
            receiver: None,
            callee: caller,
            arguments: loans.iter().copied().map(EntityId::Loan).collect(),
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
    for drop in descriptor.drops() {
        let (owner, origin) = match drop.target() {
            UnitDropTarget::Temporary(argument) => {
                let i = descriptor.contracts()[..2]
                    .iter()
                    .position(|c| c.argument() == argument)?;
                (owners[i], call_origin.clone())
            }
            UnitDropTarget::Named(root)
                if descriptor.named_string().is_some_and(|n| n.0 == root) =>
            {
                (*owners.first()?, Origin::Source(drop.value_origin()))
            }
            _ => return None,
        };
        f.append_instruction(block, Operation::Drop { owner }, vec![], origin)
            .ok()?;
    }
    f.set_terminator(
        block,
        TerminatorKind::Return { values: vec![] },
        Origin::Source(descriptor.body()),
    )
    .ok()?;
    Some(entry)
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
