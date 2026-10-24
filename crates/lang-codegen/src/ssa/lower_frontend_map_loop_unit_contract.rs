//! Exact Named cleanup and latest Map generation across the original for CFG.
use super::*;
use crate::ssa::model::{EntityType, LoanKind, SsaTypeKind};
use lang_frontend::ownership_checking::{
    UnitDropPoint, UnitIterationCleanupAction as Action, UnitIterationExitKind,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Cleanup {
    Dead,
    Live,
    Resource,
}

struct Access {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    kind: &'static str,
}

fn accesses(unit: &UnitSource, scope: Span) -> Vec<Access> {
    let types = unit.types();
    let mut accesses = Vec::new();
    accesses.extend(types.map_puts().iter().map(|fact| Access {
        expression: fact.expression(),
        receiver: fact.receiver(),
        kind: "put",
    }));
    accesses.extend(types.map_removes().iter().map(|fact| Access {
        expression: fact.expression(),
        receiver: fact.receiver(),
        kind: "remove",
    }));
    accesses.extend(types.map_contains_calls().iter().map(|fact| Access {
        expression: fact.expression(),
        receiver: fact.receiver(),
        kind: "contains",
    }));
    accesses.extend(types.map_gets().iter().map(|fact| Access {
        expression: fact.expression(),
        receiver: fact.receiver(),
        kind: "get",
    }));
    accesses.extend(types.map_sizes().iter().map(|fact| Access {
        expression: fact.expression(),
        receiver: fact.receiver(),
        kind: "size",
    }));
    accesses.retain(|access| within(expression_span(unit, access.expression), scope));
    accesses.sort_by_key(|access| expression_span(unit, access.expression).start());
    accesses
}

fn operation<'a>(function: &'a Function, span: Span, kind: &str) -> &'a Instruction {
    let predicate: fn(&Operation) -> bool = match kind {
        "put" => |operation| matches!(operation, Operation::MapPut { .. }),
        "remove" => |operation| matches!(operation, Operation::MapRemove { .. }),
        "contains" => |operation| matches!(operation, Operation::MapContains { .. }),
        "get" => |operation| matches!(operation, Operation::MapGet { .. }),
        "size" => |operation| matches!(operation, Operation::MapSize { .. }),
        _ => panic!("bounded original Map access"),
    };
    instruction_at(function, span, predicate)
}

fn owner(instruction: &Instruction) -> EntityId {
    match instruction.operation {
        Operation::MapPut { owner, .. } | Operation::MapRemove { owner, .. } => {
            EntityId::Value(owner)
        }
        Operation::MapContains { owner, .. }
        | Operation::MapGet { owner, .. }
        | Operation::MapSize { owner } => owner,
        _ => panic!("actual Map operation"),
    }
}

fn advance(function: &Function, instruction: &Instruction, current: EntityId) -> EntityId {
    transported(function, owner(instruction), current);
    if matches!(
        instruction.operation,
        Operation::MapPut { .. } | Operation::MapRemove { .. }
    ) {
        let updated = result(instruction, 0);
        assert!(matches!(updated, EntityId::Value(_)));
        assert_ne!(updated, current);
        updated
    } else {
        current
    }
}

pub(super) fn map_protocol(
    unit: &UnitSource,
    program: &Program,
    name: &str,
    cleanup: Cleanup,
    has_continue: bool,
) {
    let scope = declaration_span(unit, name);
    let function = function_at(program, scope);
    let plan = iteration(unit, scope);
    let statement = plan.descriptor().statement();
    let loop_span = statement_span(unit, statement);
    let constructors = unit
        .types()
        .map_constructions()
        .iter()
        .filter(|fact| within(expression_span(unit, fact.expression()), scope))
        .collect::<Vec<_>>();
    let construction = one(
        &constructors,
        "one original Map initializer in each function",
    );
    let construction_span = expression_span(unit, construction.expression());
    let roots = unit
        .ownership()
        .drops()
        .iter()
        .filter_map(|fact| {
            let UnitDropTarget::Named(root) = fact.target() else {
                return None;
            };
            if !within(fact.value_origin(), scope)
                || unit.types().symbol_type(root) != Some(construction.map_type())
            {
                return None;
            }
            let span = symbol_span(unit, root, scope);
            (initializer(unit, span) == construction.expression()).then_some(root)
        })
        .collect::<Vec<_>>();
    let root = *one(
        &roots,
        "Map identity comes from its actual Named fact and initializer",
    );
    let fact = named_drop(unit, root, scope);
    assert_eq!(
        unit.types().is_resource_type(construction.map_type()),
        Some(cleanup == Cleanup::Resource)
    );
    match cleanup {
        Cleanup::Dead => assert_eq!(fact.point(), UnitDropPoint::LoopExit(statement)),
        Cleanup::Live => {
            let UnitDropPoint::CallReturn(point) = fact.point() else {
                panic!("live pure Map cleanup follows its last original receiver call")
            };
            assert!(within(expression_span(unit, point), scope));
            assert!(expression_span(unit, point).start() > loop_span.end());
        }
        Cleanup::Resource => assert_eq!(
            fact.point(),
            UnitDropPoint::AfterStatement(body(unit, scope))
        ),
    }
    let exits = plan.exits();
    let exhaustion = exits
        .iter()
        .filter(|exit| exit.kind() == UnitIterationExitKind::Exhaustion)
        .collect::<Vec<_>>();
    assert_eq!(
        one(&exhaustion, "one real exhaustion plan").point(),
        UnitDropPoint::LoopExit(statement)
    );
    let fallthrough = exits
        .iter()
        .filter(|exit| exit.kind() == UnitIterationExitKind::Fallthrough)
        .collect::<Vec<_>>();
    let Statement::For {
        body: loop_body, ..
    } = unit
        .parsed()
        .ast()
        .statements()
        .get(statement.statement())
        .unwrap()
        .payload()
    else {
        unreachable!()
    };
    assert_eq!(
        one(&fallthrough, "one real body fallthrough").point(),
        UnitDropPoint::AfterStatement(UnitStatementId::new(statement.source_unit(), *loop_body))
    );
    assert_eq!(
        exits
            .iter()
            .filter(|exit| matches!(exit.kind(), UnitIterationExitKind::Continue(_)))
            .count(),
        usize::from(has_continue)
    );
    assert_eq!(exits.len(), 2 + usize::from(has_continue));
    for exit in exits {
        let drops = exit.actions().iter().filter(|action| {
            matches!(action, Action::Drop(drop) if drop.target() == UnitDropTarget::Named(root))
        }).count();
        assert_eq!(
            drops,
            usize::from(
                cleanup == Cleanup::Dead && exit.kind() == UnitIterationExitKind::Exhaustion
            )
        );
    }

    let birth = result(
        instruction_at(function, construction_span, |operation| {
            matches!(operation, Operation::MapConstruct { .. })
        }),
        0,
    );
    let accesses = accesses(unit, scope);
    for access in &accesses {
        assert_eq!(reference_root(unit, access.receiver), root);
    }
    let mut current = birth;
    for access in accesses
        .iter()
        .filter(|access| expression_span(unit, access.expression).end() <= loop_span.start())
    {
        current = advance(
            function,
            operation(
                function,
                expression_span(unit, access.expression),
                access.kind,
            ),
            current,
        );
    }
    // remove fixtures have an actual put before for; current is its result,
    // never the older MapConstruct seed. put fixtures enter with the birth.
    let cfg = loop_edges(function, loop_span, 1 + usize::from(has_continue));
    let slot = column(function, cfg.entry, current);
    transported(function, cfg.entry.arguments[slot], current);
    let header_owner = cfg.header.parameters[slot];
    assert!(matches!(header_owner, EntityId::Value(_)));
    assert_eq!(cfg.body.arguments[slot], header_owner);
    assert_eq!(cfg.exhaustion.arguments[slot], header_owner);
    let mut body_current = header_owner;
    let mut body_updates = Vec::new();
    let mut removed_payloads = BTreeSet::new();
    for access in accesses
        .iter()
        .filter(|access| within(expression_span(unit, access.expression), loop_span))
    {
        let instruction = operation(
            function,
            expression_span(unit, access.expression),
            access.kind,
        );
        let updated = advance(function, instruction, body_current);
        if updated != body_current {
            body_updates.push(updated);
        }
        if matches!(instruction.operation, Operation::MapRemove { .. }) {
            removed_payloads.insert(result(instruction, 1));
        }
        body_current = updated;
    }
    assert_eq!(body_updates.len(), 1 + usize::from(has_continue));
    let expected_backedges = if has_continue {
        body_updates.iter().copied().collect::<BTreeSet<_>>()
    } else {
        BTreeSet::from([body_current])
    };
    let mut actual_backedges = BTreeSet::new();
    let continue_spans = exits
        .iter()
        .filter_map(|exit| match exit.kind() {
            UnitIterationExitKind::Continue(expression) => Some(expression_span(unit, expression)),
            _ => None,
        })
        .collect::<Vec<_>>();
    for (block, edge) in &cfg.backedges {
        let roots = transport_roots(function, edge.arguments[slot], &expected_backedges)
            .expect("every real re-entry must carry a latest update");
        assert_eq!(
            roots.len(),
            1,
            "one current generation on each distinct backedge"
        );
        if has_continue {
            let is_continue = block.terminator.as_ref().unwrap().origin
                == Origin::Source(*one(&continue_spans, "actual continue origin"));
            assert_eq!(
                roots,
                BTreeSet::from([body_updates[usize::from(!is_continue)]]),
                "continue carries head update; natural fallthrough carries tail update"
            );
        }
        for entity in &edge.arguments {
            assert!(
                transport_roots(function, *entity, &removed_payloads).is_none(),
                "removed payload never becomes a loop-carried owner"
            );
        }
        actual_backedges.extend(roots);
    }
    assert_eq!(actual_backedges, expected_backedges);
    let all_incoming = expected_backedges
        .union(&BTreeSet::from([current]))
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        transport_roots(function, header_owner, &all_incoming),
        Some(all_incoming)
    );

    let false_block = function.block(cfg.exhaustion.target).unwrap();
    let exhausted_owner = false_block.parameters[slot];
    transported(function, exhausted_owner, header_owner);
    let drops = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(fact.value_origin())
                && matches!(instruction.operation, Operation::Drop { .. })
        })
        .collect::<Vec<_>>();
    let drop = *one(&drops, "one static Drop for this exact Named Map");
    let Operation::Drop { owner: dropped } = drop.operation else {
        unreachable!()
    };
    let post = accesses
        .iter()
        .filter(|access| expression_span(unit, access.expression).start() > loop_span.end())
        .collect::<Vec<_>>();
    if cleanup == Cleanup::Dead {
        assert!(post.is_empty());
        assert_eq!(drop.block, false_block.id);
        transported(function, EntityId::Value(dropped), exhausted_owner);
        // The cleaned Header cannot be revived in the loop's post-loop Exit.
        for edge in edges(false_block) {
            for entity in &edge.arguments {
                assert!(
                    transport_roots(function, *entity, &BTreeSet::from([exhausted_owner]))
                        .is_none()
                );
            }
        }
    } else {
        let mut current = exhausted_owner;
        let mut last = None;
        for access in post {
            let instruction = operation(
                function,
                expression_span(unit, access.expression),
                access.kind,
            );
            current = advance(function, instruction, current);
            last = Some(instruction);
        }
        if cleanup == Cleanup::Live {
            let last = last.expect("live owner reaches its original post-loop use");
            let UnitDropPoint::CallReturn(point) = fact.point() else {
                unreachable!()
            };
            assert_eq!(last.origin, Origin::Source(expression_span(unit, point)));
            precedes(function, last, drop);
        }
        transported(function, EntityId::Value(dropped), current);
    }
    let module = program.module(function.id.module()).unwrap();
    assert_eq!(function.instructions.iter().filter(|instruction| match instruction.operation {
        Operation::Drop { owner } => matches!(function.entity(EntityId::Value(owner)).unwrap().ty,
            EntityType::Value(ty) if matches!(module.types[ty.index()], SsaTypeKind::MapContainer { .. })),
        _ => false,
    }).count(), 1);
}

pub(in super::super) fn check_original(text: &str) {
    let (unit, program, _) = lower(text, &["main"]);
    map_protocol(&unit, &program, "main", Cleanup::Dead, false);
}

pub(in super::super) fn check_normal(text: &str) {
    let (unit, program, _) = lower(
        text,
        &["main", "removeDead", "removeLive", "putDead", "putLive"],
    );
    for name in ["removeDead", "putDead"] {
        map_protocol(&unit, &program, name, Cleanup::Dead, false);
    }
    for name in ["removeLive", "putLive"] {
        map_protocol(&unit, &program, name, Cleanup::Live, false);
    }
    let text = unit.declaration("Text", SymbolKind::Classifier);
    assert!(
        unit.types()
            .signatures()
            .declaration(text)
            .unwrap()
            .nominal()
            .unwrap()
            .deinit()
            .is_none()
    );
}

pub(in super::super) fn check_resource(text: &str) {
    let (unit, program, _) = lower(text, &["main"]);
    map_protocol(&unit, &program, "main", Cleanup::Resource, false);
    let scope = declaration_span(&unit, "main");
    let function = function_at(&program, scope);
    let after = unit
        .parsed()
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            (within(node.span(), scope)
                && unit.sources().slice(node.span()) == Ok("println(\"after\")"))
            .then_some(node.span())
        })
        .collect::<Vec<_>>();
    let after = instruction_at(
        function,
        *one(&after, "original observable after call"),
        |operation| matches!(operation, Operation::PrintString { .. }),
    );
    let map_drop = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(instruction.operation, Operation::Drop { .. })
                && unit.sources().slice(instruction.origin.span()) == Ok("resources")
        })
        .collect::<Vec<_>>();
    let drop = *one(&map_drop, "lexical resources cleanup");
    precedes(function, after, drop);
    let resource = unit.declaration("Resource", SymbolKind::Classifier);
    let descriptor = unit
        .types()
        .signatures()
        .declaration(resource)
        .unwrap()
        .nominal()
        .unwrap()
        .deinit()
        .expect("real Resource deinit dependency");
    assert_eq!(descriptor.owner(), resource);
    let span = unit
        .parsed()
        .ast()
        .items()
        .get(descriptor.item().item())
        .unwrap()
        .span();
    let deinit = function_at(&program, span);
    let Some(EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    }) = deinit.receiver()
    else {
        panic!("Resource deinit retains its shared receiver ABI")
    };
    assert_eq!(
        program.module(deinit.id.module()).unwrap().deinit(target),
        Some(deinit.id)
    );
    assert!(
        deinit
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::PrintString { .. }))
    );
}
