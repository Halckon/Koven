//! Complete source entry emission in the experiment; no production entry capability.
use super::{e2_function, model::*};
use lang_frontend::{
    ownership_checking::{
        UnitDropTarget,
        two_source_experiment::{EntryDescriptor, TwoSourceCounterFact, TwoSourceExperiment},
    },
    type_checking::{UnitCallTarget, ValidatedCompilationUnitTypes},
};
pub(super) struct EmittedEntry<'a> {
    pub(super) function: e2_function::EmittedFunction<'a>,
    pub(super) entry: FunctionId,
    descriptor: &'a EntryDescriptor,
}
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
    let module = &mut function.fragments.leaves.program.modules[0];
    let string = module.intern_type(SsaTypeKind::StringOwner);
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let entry = module
        .add_function("e2_source_entry", vec![], Origin::Source(descriptor.body()))
        .ok()?;
    let f = module.function_mut(entry)?;
    let block = f
        .add_block(vec![], Origin::Source(descriptor.body()))
        .ok()?;
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
    let mut owners = Vec::new();
    let mut loans = Vec::new();
    for (i, c) in descriptor.contracts().iter().enumerate() {
        let ty = [string, string, int, boolean][i];
        let origin = Origin::Source(c.argument_span());
        let root = if i < 2 {
            let owner = value(append(
                f,
                block,
                Operation::StringLiteral {
                    string,
                    bytes: descriptor.strings()[i].clone(),
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
            callee: function.caller,
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
        let UnitDropTarget::Temporary(argument) = drop.target() else {
            return None;
        };
        let i = descriptor.contracts()[..2]
            .iter()
            .position(|c| c.argument() == argument)?;
        f.append_instruction(
            block,
            Operation::Drop { owner: owners[i] },
            vec![],
            call_origin.clone(),
        )
        .ok()?;
    }
    f.set_terminator(
        block,
        TerminatorKind::Return { values: vec![] },
        Origin::Source(descriptor.body()),
    )
    .ok()?;
    super::verify::verify_program(&function.fragments.leaves.program).ok()?;
    Some(EmittedEntry {
        function,
        entry,
        descriptor,
    })
}
