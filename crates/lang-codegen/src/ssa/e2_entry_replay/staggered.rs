//! Independent original two-owner replay; never reconstruct emission or synthesize cleanup.
use super::*;
use lang_frontend::ownership_checking::two_source_experiment::EntryDescriptor;

pub(super) fn replay(
    module: &Module,
    cursor: &mut Cursor<'_>,
    caller: FunctionId,
    d: &EntryDescriptor,
) -> Option<()> {
    let owners = d.staggered_owners()?;
    if d.calls().len() != 2 || owners[0].root() == owners[1].root() {
        return None;
    }
    let string = SsaTypeId {
        module: module.id,
        index: module
            .types
            .iter()
            .position(|ty| *ty == SsaTypeKind::StringOwner)?,
    };
    let mut actual = Vec::new();
    for descriptor in owners {
        let origin = Origin::Source(descriptor.span());
        let owner = value(cursor.step(
            Operation::StringLiteral {
                string,
                bytes: descriptor.bytes().to_vec(),
            },
            origin.clone(),
            Some(EntityType::Value(string)),
        )?)?;
        let root = place(cursor.step(
            Operation::RootPlace { owner },
            origin,
            Some(EntityType::Place(string)),
        )?)?;
        actual.push((descriptor.root(), owner, root, false));
    }
    for call in d.calls() {
        if !call.initialization_order().is_empty()
            || !call.drops().is_empty()
            || (0..4).any(|i| call.temporary_string(i).is_some())
        {
            return None;
        }
        let active: Vec<_> = actual
            .iter()
            .filter(|(_, _, _, dropped)| !*dropped)
            .map(|(symbol, _, root, _)| (*symbol, *root))
            .collect();
        replay_call(module, cursor, caller, call, Some(&active))?;
        let point = lang_frontend::ownership_checking::UnitDropPoint::CallReturn(
            call.contracts()[0].call(),
        );
        let matching: Vec<_> = owners
            .iter()
            .enumerate()
            .filter(|(_, owner)| owner.drop_fact().point() == point)
            .collect();
        let [(index, descriptor)] = matching.as_slice() else {
            return None;
        };
        if descriptor.drop_fact().target() != UnitDropTarget::Named(descriptor.root())
            || actual[*index].3
        {
            return None;
        }
        cursor.step(
            Operation::Drop {
                owner: actual[*index].1,
            },
            Origin::Source(descriptor.drop_fact().value_origin()),
            None,
        )?;
        actual[*index].3 = true;
    }
    actual
        .iter()
        .all(|(_, _, _, dropped)| *dropped)
        .then_some(())
}
