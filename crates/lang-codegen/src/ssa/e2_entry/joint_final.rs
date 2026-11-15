//! Two original owners remain live until their original joint terminal cleanup.
use super::*;

pub(super) fn emit(
    module: &mut Module,
    entry: FunctionId,
    bindings: &crate::ssa::e2_occurrences::Bindings,
    d: &EntryDescriptor,
) -> Option<()> {
    let (owners, terminal_drops) = d.joint_final_owners()?;
    if d.calls().len() != 2 || owners[0].root() == owners[1].root() {
        return None;
    }
    let string = module.intern_type(SsaTypeKind::StringOwner);
    let mut actual = Vec::new();
    for descriptor in owners {
        let f = module.function_mut(entry)?;
        let block = f.blocks.first()?.id;
        let origin = Origin::Source(descriptor.span());
        let owner = value(append(
            f,
            block,
            Operation::StringLiteral {
                string,
                bytes: descriptor.bytes().to_vec(),
            },
            EntityType::Value(string),
            origin.clone(),
        )?)?;
        let root = place(append(
            f,
            block,
            Operation::RootPlace { owner },
            EntityType::Place(string),
            origin,
        )?)?;
        actual.push((descriptor.root(), owner, root, false));
    }
    for (index, call) in d.calls().iter().enumerate() {
        let caller = bindings.caller(index, call)?;
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
        emit_call(module, entry, caller, call, Some(&active))?;
        let point = lang_frontend::ownership_checking::UnitDropPoint::CallReturn(
            call.contracts()[0].call(),
        );
        if index == 0 {
            continue;
        }
        for drop in terminal_drops {
            let matches: Vec<_> = owners
                .iter()
                .enumerate()
                .filter(|(_, owner)| owner.drop_fact() == drop)
                .collect();
            let [(index, descriptor)] = matches.as_slice() else {
                return None;
            };
            if drop.point() != point {
                return None;
            }
            if descriptor.drop_fact().target() != UnitDropTarget::Named(descriptor.root())
                || actual[*index].3
            {
                return None;
            }
            let f = module.function_mut(entry)?;
            f.append_instruction(
                f.blocks.first()?.id,
                Operation::Drop {
                    owner: actual[*index].1,
                },
                vec![],
                Origin::Source(descriptor.drop_fact().value_origin()),
            )
            .ok()?;
            actual[*index].3 = true;
        }
    }
    actual
        .iter()
        .all(|(_, _, _, dropped)| *dropped)
        .then_some(())
}
