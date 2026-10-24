//! Owned source presence is checked at each actual provider edge and cleanup.
use super::*;
use crate::ssa::model::{BinaryOperator, ComparisonOperator, ScalarConstant};
use lang_frontend::ownership_checking::{
    UnitDropPoint, UnitIterationCleanupAction as Action, UnitIterationExitKind,
    UnitIterationSourceAccess, UnitLoanTarget,
};

#[derive(Clone, Copy)]
pub(in super::super) enum SourceScenario {
    NamedAfterUse,
    NamedLoopExit,
    Temporary,
    Providers,
}

fn source_protocol(unit: &UnitSource, program: &Program, name: &str, after_use: bool, named: bool) {
    let scope = declaration_span(unit, name);
    let function = function_at(program, scope);
    let plan = iteration(unit, scope);
    let descriptor = plan.descriptor();
    let statement = descriptor.statement();
    let loop_span = statement_span(unit, statement);
    let (construction, target, origin) = match plan.source().unwrap() {
        UnitLoanTarget::Place(place) => {
            assert!(named);
            assert!(place.is_root());
            assert_eq!(plan.source_access(), UnitIterationSourceAccess::Owned);
            let root = reference_root(unit, descriptor.source());
            assert_eq!(root, place.root());
            let span = symbol_span(unit, root, scope);
            assert_eq!(unit.sources().slice(span).unwrap(), "keys");
            assert_eq!(
                unit.types().symbol_type(root),
                Some(descriptor.source_type())
            );
            (initializer(unit, span), UnitDropTarget::Named(root), span)
        }
        UnitLoanTarget::Temporary(expression) => {
            assert!(!named);
            assert!(!after_use);
            assert_eq!(plan.source_access(), UnitIterationSourceAccess::Temporary);
            assert_eq!(*expression, descriptor.source());
            (
                *expression,
                UnitDropTarget::Temporary(*expression),
                expression_span(unit, *expression),
            )
        }
        UnitLoanTarget::This(_) => panic!("these originals have no receiver source"),
    };
    assert_eq!(
        unit.types().is_resource_type(descriptor.source_type()),
        Some(false)
    );
    let constructors = unit
        .types()
        .container_constructions()
        .iter()
        .filter(|fact| fact.expression() == construction)
        .collect::<Vec<_>>();
    let constructor = one(
        &constructors,
        "source birth is the actual typed container construction",
    );
    assert_eq!(constructor.container_type(), descriptor.source_type());
    let drops = unit
        .ownership()
        .drops()
        .iter()
        .filter(|fact| fact.target() == target)
        .collect::<Vec<_>>();
    let drop_fact = **one(&drops, "one exact named or temporary source cleanup");
    assert_eq!(drop_fact.value_origin(), origin);
    let sizes = unit
        .types()
        .container_sizes()
        .iter()
        .filter(|size| within(size.span(), scope) && size.span().start() > loop_span.end())
        .collect::<Vec<_>>();
    let size = if after_use {
        let size = **one(&sizes, "the original post-loop keys.size descriptor");
        let UnitDropTarget::Named(root) = target else {
            unreachable!()
        };
        assert_eq!(reference_root(unit, size.receiver()), root);
        assert_eq!(unit.sources().slice(size.span()).unwrap(), "keys.size");
        assert_eq!(expression_span(unit, size.expression()), size.span());
        assert_eq!(
            drop_fact.point(),
            UnitDropPoint::CallReturn(size.expression())
        );
        Some(size)
    } else {
        assert!(sizes.is_empty());
        assert_eq!(drop_fact.point(), UnitDropPoint::LoopExit(statement));
        None
    };
    let exits = plan.exits();
    assert_eq!(exits.len(), 3);
    let continues = exits
        .iter()
        .filter_map(|exit| match exit.kind() {
            UnitIterationExitKind::Continue(expression) => {
                assert_eq!(exit.point(), UnitDropPoint::ControlTransfer(expression));
                assert!(matches!(
                    unit.parsed()
                        .ast()
                        .expressions()
                        .get(expression.expression())
                        .unwrap()
                        .payload(),
                    Expression::Continue { .. }
                ));
                assert!(within(expression_span(unit, expression), loop_span));
                Some(expression)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let continue_expression = *one(&continues, "one real conditional continue");
    let mut exhaustion_seen = 0;
    let mut fallthrough_seen = 0;
    for exit in exits {
        let exhaustion = exit.kind() == UnitIterationExitKind::Exhaustion;
        if exhaustion {
            exhaustion_seen += 1;
            assert_eq!(exit.point(), UnitDropPoint::LoopExit(statement));
            let finish = exit
                .actions()
                .iter()
                .position(|action| *action == Action::FinishProvider(statement))
                .unwrap();
            let end = exit
                .actions()
                .iter()
                .position(|action| *action == Action::EndSource(statement))
                .unwrap();
            assert!(finish < end);
            let dropping = exit
                .actions()
                .iter()
                .enumerate()
                .filter_map(|(index, action)| {
                    matches!(action, Action::Drop(fact) if fact.target() == target).then_some(index)
                })
                .collect::<Vec<_>>();
            if after_use {
                assert!(dropping.is_empty(), "live keys survives exhaustion");
            } else {
                assert!(end < *one(&dropping, "EndSource precedes exact source Drop"));
            }
        } else {
            fallthrough_seen += usize::from(exit.kind() == UnitIterationExitKind::Fallthrough);
            assert!(exit.actions().contains(&Action::EndElement(statement)));
            assert!(exit.actions().iter().all(|action| {
                !matches!(action, Action::EndSource(id) | Action::FinishProvider(id) if *id == statement)
                    && !matches!(action, Action::Drop(fact) if fact.target() == target)
            }), "continue/fallthrough preserve the source, but retire the element");
        }
    }
    assert_eq!((exhaustion_seen, fallthrough_seen), (1, 1));

    let birth = result(
        instruction_at(function, expression_span(unit, construction), |operation| {
            matches!(operation, Operation::ContainerConstruct { .. })
        }),
        0,
    );
    let cfg = loop_edges(function, loop_span, 2);
    let snapshot = instruction_at(function, loop_span, |operation| {
        matches!(operation, Operation::ContainerLength { .. })
    });
    let Operation::ContainerLength {
        owner: EntityId::Loan(source_loan),
    } = snapshot.operation
    else {
        panic!("provider snapshot uses its actual source loan")
    };
    let source_loan = EntityId::Loan(source_loan);
    transported(function, source_loan, birth);
    let owner_slot = column(function, cfg.entry, birth);
    let loan_slot = column(function, cfg.entry, source_loan);
    let length_slot = column(function, cfg.entry, result(snapshot, 0));
    let header_owner = cfg.header.parameters[owner_slot];
    let header_loan = cfg.header.parameters[loan_slot];
    let header_length = cfg.header.parameters[length_slot];
    transported(function, header_owner, birth);
    transported(function, header_loan, source_loan);
    transported(function, header_length, result(snapshot, 0));
    let compares = cfg
        .header
        .instructions
        .iter()
        .map(|id| function.instruction(*id).unwrap())
        .filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::Compare {
                    operator: ComparisonOperator::LessThan,
                    ..
                }
            )
        })
        .collect::<Vec<_>>();
    let comparison = one(&compares, "one true provider guard");
    let Operation::Compare {
        left: cursor,
        right: length,
        ..
    } = comparison.operation
    else {
        unreachable!()
    };
    assert_eq!(EntityId::Value(length), header_length);
    let cursor = EntityId::Value(cursor);
    let cursor_slot = cfg
        .header
        .parameters
        .iter()
        .position(|entity| *entity == cursor)
        .unwrap();
    let Definition::InstructionResult {
        instruction: zero,
        index: 0,
    } = function
        .entity(cfg.entry.arguments[cursor_slot])
        .unwrap()
        .definition
    else {
        panic!("the entry cursor is the original zero constant")
    };
    assert_eq!(
        function.instruction(zero).unwrap().operation,
        Operation::Constant(ScalarConstant::Integer(0))
    );
    for edge in [cfg.body, cfg.exhaustion] {
        assert_eq!(edge.arguments[owner_slot], header_owner);
        assert_eq!(edge.arguments[loan_slot], header_loan);
        assert_eq!(edge.arguments[length_slot], header_length);
        assert_eq!(edge.arguments[cursor_slot], cursor);
    }
    let body_block = function.block(cfg.body.target).unwrap();
    let element_places = body_block
        .instructions
        .iter()
        .map(|id| function.instruction(*id).unwrap())
        .filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::ContainerElementPlace { .. }
            )
        })
        .collect::<Vec<_>>();
    let element_place = one(&element_places, "one guarded element place");
    let Operation::ContainerElementPlace {
        owner: element_source,
        index,
    } = element_place.operation
    else {
        unreachable!()
    };
    transported(function, element_source, header_loan);
    transported(function, EntityId::Value(index), cursor);
    let element_loans = body_block.instructions.iter().map(|id| function.instruction(*id).unwrap())
        .filter(|instruction| matches!(instruction.operation,
            Operation::BorrowBegin { place, .. } if EntityId::Place(place) == result(element_place, 0)))
        .collect::<Vec<_>>();
    let element_loan = result(
        one::<&Instruction>(&element_loans, "one guarded element loan"),
        0,
    );
    let continue_span = expression_span(unit, continue_expression);
    let mut continue_edges = 0;
    for (block, edge) in &cfg.backedges {
        continue_edges +=
            usize::from(block.terminator.as_ref().unwrap().origin == Origin::Source(continue_span));
        transported(function, edge.arguments[owner_slot], header_owner);
        transported(function, edge.arguments[loan_slot], header_loan);
        transported(function, edge.arguments[length_slot], header_length);
        let Definition::InstructionResult {
            instruction,
            index: 0,
        } = function
            .entity(edge.arguments[cursor_slot])
            .unwrap()
            .definition
        else {
            panic!("each backedge publishes its actual increment")
        };
        let increment = function.instruction(instruction).unwrap();
        assert_eq!(increment.block, block.id);
        let Operation::Binary {
            operator: BinaryOperator::Add,
            left,
            right,
        } = increment.operation
        else {
            panic!("provider cursor advances by one")
        };
        transported(function, EntityId::Value(left), cursor);
        let Definition::InstructionResult {
            instruction: step,
            index: 0,
        } = function.entity(EntityId::Value(right)).unwrap().definition
        else {
            panic!("fixed step");
        };
        assert_eq!(
            function.instruction(step).unwrap().operation,
            Operation::Constant(ScalarConstant::Integer(1))
        );
        assert_eq!(function.instruction(step).unwrap().block, cfg.preheader.id);
        let ends = block
            .instructions
            .iter()
            .map(|id| function.instruction(*id).unwrap())
            .filter_map(|instruction| match instruction.operation {
                Operation::BorrowEnd { loan } => Some((instruction, EntityId::Loan(loan))),
                _ => None,
            })
            .collect::<Vec<_>>();
        let elements = ends
            .iter()
            .filter(|(_, loan)| {
                transport_roots(function, *loan, &BTreeSet::from([element_loan]))
                    == Some(BTreeSet::from([element_loan]))
            })
            .collect::<Vec<_>>();
        let (end, _) = one(
            &elements,
            "each backedge ends its current element exactly once",
        );
        precedes(function, end, increment);
        assert!(
            ends.iter().all(|(_, loan)| {
                transport_roots(function, *loan, &BTreeSet::from([source_loan])).is_none()
            }),
            "neither re-entry ends the source loan"
        );
        assert!(block.instructions.iter().all(|id| {
            let instruction = function.instruction(*id).unwrap();
            !matches!(instruction.operation, Operation::Drop { owner }
                if transport_roots(function, EntityId::Value(owner), &BTreeSet::from([birth])).is_some())
        }));
    }
    assert_eq!(
        continue_edges, 1,
        "one continue and one natural fallthrough re-entry"
    );
    let exhausted = function.block(cfg.exhaustion.target).unwrap();
    let exhausted_owner = exhausted.parameters[owner_slot];
    let exhausted_loan = exhausted.parameters[loan_slot];
    transported(function, exhausted_owner, header_owner);
    transported(function, exhausted_loan, header_loan);
    let ends = exhausted
        .instructions
        .iter()
        .map(|id| function.instruction(*id).unwrap())
        .filter(|instruction| {
            matches!(instruction.operation,
            Operation::BorrowEnd { loan } if EntityId::Loan(loan) == exhausted_loan)
        })
        .collect::<Vec<_>>();
    let end = *one(
        &ends,
        "exhaustion ends its current source loan exactly once",
    );
    let drop = instruction_at(function, origin, |operation| {
        matches!(operation, Operation::Drop { .. })
    });
    let Operation::Drop { owner } = drop.operation else {
        unreachable!()
    };
    transported(function, EntityId::Value(owner), exhausted_owner);
    precedes(function, end, drop);
    if let Some(size) = size {
        let read = instruction_at(function, size.span(), |operation| {
            matches!(operation, Operation::ContainerLength { .. })
        });
        let Operation::ContainerLength { owner: read_owner } = read.operation else {
            unreachable!()
        };
        transported(function, read_owner, exhausted_owner);
        precedes(function, end, read);
        precedes(function, read, drop);
        let reads_end = instruction_at(function, size.span(), |operation| {
            matches!(operation, Operation::BorrowEnd { .. })
        });
        let Operation::BorrowEnd { loan } = reads_end.operation else {
            unreachable!()
        };
        transported(function, EntityId::Loan(loan), read_owner);
        precedes(function, read, reads_end);
        precedes(function, reads_end, drop);
    } else {
        assert_eq!(drop.block, exhausted.id);
        for edge in edges(exhausted) {
            assert!(
                edge.arguments.iter().all(|entity| {
                    transport_roots(function, *entity, &BTreeSet::from([exhausted_owner])).is_none()
                }),
                "cleaned source does not reappear after loop exit"
            );
        }
    }
}

pub(in super::super) fn check_source(text: &str, scenario: SourceScenario) {
    let providers = [
        "main",
        "namedArray",
        "temporaryArray",
        "namedList",
        "temporaryList",
        "namedMutableList",
        "temporaryMutableList",
    ];
    let functions: &[&str] = if matches!(scenario, SourceScenario::Providers) {
        &providers
    } else {
        &["main"]
    };
    let (unit, program, _) = lower(text, functions);
    let cases: &[(&str, bool, bool, bool)] = match scenario {
        SourceScenario::NamedAfterUse => &[("main", true, false, true)],
        SourceScenario::NamedLoopExit => &[("main", false, true, true)],
        SourceScenario::Temporary => &[("main", false, true, false)],
        SourceScenario::Providers => &[
            ("namedArray", true, true, true),
            ("temporaryArray", false, true, false),
            ("namedList", true, true, true),
            ("temporaryList", false, true, false),
            ("namedMutableList", true, true, true),
            ("temporaryMutableList", false, true, false),
        ],
    };
    for (name, after_use, has_map, named) in cases {
        source_protocol(&unit, &program, name, *after_use, *named);
        if *has_map {
            map_contract::map_protocol(&unit, &program, name, map_contract::Cleanup::Live, true);
        }
    }
    if matches!(scenario, SourceScenario::Providers) {
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
}
