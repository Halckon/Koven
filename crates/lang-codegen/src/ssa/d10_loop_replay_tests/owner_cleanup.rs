//! Existing no-Replace owner transport only; not a primitive route capability.
use super::*;
use lang_frontend::ownership_checking::UnitDropPoint;

fn replay_owner(
    function: &model::Function,
    drop_origin: model::Origin,
    call_origin: model::Origin,
) -> Option<()> {
    use model::{EntityId, Operation, TerminatorKind};
    let entry = function.block(function.entry_block()?)?;
    let TerminatorKind::Branch(first) = &entry.terminator.as_ref()?.kind else {
        return None;
    };
    let header = function.block(first.target)?;
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        condition,
    } = &header.terminator.as_ref()?.kind
    else {
        return None;
    };
    let body = function.block(when_true.target)?;
    let exit = function.block(when_false.target)?;
    let TerminatorKind::Branch(back) = &body.terminator.as_ref()?.kind else {
        return None;
    };
    if function.blocks.len() != 4
        || BTreeSet::from([entry.id, header.id, body.id, exit.id]).len() != 4
        || back.target != header.id
        || !body.instructions.is_empty()
        || !matches!(&exit.terminator.as_ref()?.kind, TerminatorKind::Return { values } if values.is_empty())
    {
        return None;
    }
    let [literal, read, place, borrow, print, end, drop] = function.instructions.as_slice() else {
        return None;
    };
    if entry.instructions != vec![literal.id]
        || header.instructions != vec![read.id]
        || exit.instructions != vec![place.id, borrow.id, print.id, end.id, drop.id]
        || literal.block != entry.id
        || read.block != header.id
        || [place, borrow, print, end, drop]
            .iter()
            .any(|i| i.block != exit.id)
    {
        return None;
    }
    if !matches!(literal.operation, Operation::StringLiteral { .. }) {
        return None;
    }
    let [EntityId::Value(initial)] = literal.results.as_slice() else {
        return None;
    };
    let [EntityId::Value(header_owner), EntityId::Loan(header_flag)] = header.parameters.as_slice()
    else {
        return None;
    };
    let [EntityId::Value(body_owner), EntityId::Loan(body_flag)] = body.parameters.as_slice()
    else {
        return None;
    };
    let [EntityId::Value(exit_owner), EntityId::Loan(exit_flag)] = exit.parameters.as_slice()
    else {
        return None;
    };
    let [EntityId::Loan(flag)] = entry.parameters.as_slice() else {
        return None;
    };
    if first.arguments != vec![EntityId::Value(*initial), EntityId::Loan(*flag)]
        || when_true.arguments != vec![EntityId::Value(*header_owner), EntityId::Loan(*header_flag)]
        || when_false.arguments != when_true.arguments
        || back.arguments != vec![EntityId::Value(*body_owner), EntityId::Loan(*body_flag)]
        || read.operation
            != (Operation::Read {
                source: model::PlaceAccess::Loan(*header_flag),
            })
        || read.results != vec![EntityId::Value(*condition)]
    {
        return None;
    }
    // Actual block parameter correspondence, not equal numeric owner IDs across blocks.
    for (edge, target) in [
        (first, header),
        (when_true, body),
        (when_false, exit),
        (back, header),
    ] {
        if edge.arguments.len() != target.parameters.len() {
            return None;
        }
        for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
            if function.entity(*argument)?.ty != function.entity(*parameter)?.ty {
                return None;
            }
        }
    }
    let _ = exit_flag;
    let [EntityId::Place(p)] = place.results.as_slice() else {
        return None;
    };
    let [EntityId::Loan(l)] = borrow.results.as_slice() else {
        return None;
    };
    if place.operation != (Operation::RootPlace { owner: *exit_owner })
        || borrow.operation
            != (Operation::BorrowBegin {
                place: *p,
                kind: model::LoanKind::Shared,
            })
        || print.operation != (Operation::PrintString { value: *l })
        || end.operation != (Operation::BorrowEnd { loan: *l })
        || drop.operation != (Operation::Drop { owner: *exit_owner })
        || drop.origin != drop_origin
        || print.origin != call_origin
        || end.origin != call_origin
        || !print.results.is_empty()
        || !end.results.is_empty()
        || !drop.results.is_empty()
    {
        return None;
    }
    Some(())
}

#[test]
fn existing_owner_loop_replays_real_suffix_cleanup_and_rejects_break_shape() {
    for (body, accepted) in [("", true), ("break", false)] {
        let text = format!(
            "fun run(flag: Boolean) {{ var a = \"initial\"; while(flag) {{ {body} }}; println(a) }}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "owner-loop.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "owner-loop.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
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
            let [fact] = owned.ownership().drops() else {
                panic!("exact cleanup")
            };
            let lang_frontend::ownership_checking::UnitDropTarget::Named(root) = fact.target()
            else {
                panic!("named root")
            };
            let unit_names = names
                .names()
                .source_units()
                .iter()
                .find(|n| n.source_unit() == root.source_unit())
                .unwrap();
            let roots = unit_names
                .resolution()
                .symbols()
                .iter()
                .filter(|s| s.name() == "a")
                .collect::<Vec<_>>();
            let [symbol] = roots.as_slice() else {
                panic!("unique actual root declaration")
            };
            assert_eq!(symbol.id(), root.symbol());
            assert_eq!(symbol.span(), fact.value_origin());
            let UnitDropPoint::CallReturn(call) = fact.point() else {
                panic!("actual suffix cleanup point")
            };
            assert_eq!(call.source_unit(), root.source_unit());
            let call_span = parsed
                .ast()
                .expressions()
                .get(call.expression())
                .unwrap()
                .span();
            assert_eq!(sources.slice(call_span).unwrap(), "println(a)");
            let (program, _) = unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
            .unwrap();
            verify_program(&program).unwrap();
            let functions = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .filter(|f| !f.blocks.is_empty())
                .collect::<Vec<_>>();
            let [function] = functions.as_slice() else {
                panic!("single actual function")
            };
            assert_eq!(
                replay_owner(
                    function,
                    model::Origin::Source(fact.value_origin()),
                    model::Origin::Source(call_span)
                )
                .is_some(),
                accepted
            );
        }
    }
}
