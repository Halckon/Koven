//! Real Unit exit events supplement, but do not retire, the Single owner-table witness.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        OwnershipBindingKind, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as Action,
        UnitIterationExitKind as Exit, UnitIterationSourceAccess, UnitLoanTarget,
        check_compilation_unit_ownership,
    },
    parser::{Expression, ForBinding, NameMarker, Statement, parse_file},
    source::SourceMap,
    type_checking::{
        UnitExpressionId, UnitStatementId, check_compilation_unit_types, standard_environments,
    },
};

#[test]
fn conditional_sources_publish_exact_unit_cleanup_for_each_real_exit() {
    for source_text in [
        "if (flag) listOf<Int>() else listOf(1, 2)",
        "when (flag) { true -> listOf<Int>()\nelse -> listOf(1, 2) }",
    ] {
        for binding in ["_", "n"] {
            for body in ["", "continue", "break", "return"] {
                let text = format!(
                    "fun run(flag: Boolean) {{ for ({binding} in {source_text}) {{ {body} }} }}"
                );
                let mut sources = SourceMap::new();
                let source_id = sources.add_source("iteration.ko", &text).unwrap();
                let lexed = lex(&sources, source_id).unwrap();
                assert!(lexed.diagnostics().is_empty());
                let parsed = parse_file(&sources, &lexed).unwrap();
                assert!(parsed.diagnostics().is_empty());
                let (statement, binding_span, source, body_statement) = parsed
                    .ast()
                    .statements()
                    .iter()
                    .find_map(|(id, node)| match node.payload() {
                        Statement::For {
                            binding: ForBinding::Name(NameMarker::Present(span)),
                            source,
                            body,
                            ..
                        } => Some((id, *span, *source, *body)),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(sources.slice(binding_span).unwrap(), binding);
                let source_span = parsed.ast().expressions().get(source).unwrap().span();
                assert_eq!(sources.slice(source_span).unwrap(), source_text);
                let transfer = parsed.ast().expressions().iter().find_map(|(id, node)| {
                    matches!(
                        node.payload(),
                        Expression::Continue { .. }
                            | Expression::Break { .. }
                            | Expression::Return { .. }
                    )
                    .then_some(id)
                });
                let (environment, types) = standard_environments();
                for input in [
                    SourceUnitInput::memory_document(source_id, &parsed),
                    SourceUnitInput::new("root", "iteration.ko", source_id, &parsed),
                ] {
                    let inputs = [input];
                    let index = index_compilation_unit(&sources, &inputs).unwrap();
                    assert!(index.diagnostics().is_empty());
                    let unit = index.source_units()[0].id();
                    assert_eq!(index.source_units()[0].source_id(), source_id);
                    let names =
                        resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                            .unwrap()
                            .validate()
                            .unwrap();
                    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                        .unwrap()
                        .validate()
                        .unwrap();
                    let owned =
                        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                            .unwrap();
                    assert!(
                        owned.diagnostics().is_empty(),
                        "{text}: {:?}",
                        owned.diagnostics()
                    );
                    assert!(
                        owned.deferred().is_empty(),
                        "{text}: {:?}",
                        owned.deferred()
                    );
                    assert!(owned.is_compatible_with(&typed));
                    assert!(owned.clone().validate().is_ok());
                    let statement = UnitStatementId::new(unit, statement);
                    let source = UnitExpressionId::new(unit, source);
                    let [plan] = owned.iterations() else {
                        panic!("one real provider: {text}")
                    };
                    assert_eq!(owned.iteration(statement), Some(plan));
                    assert_eq!(plan.descriptor().statement(), statement);
                    assert_eq!(plan.descriptor().source(), source);
                    assert_eq!(plan.source(), Some(&UnitLoanTarget::Temporary(source)));
                    assert_eq!(plan.source_access(), UnitIterationSourceAccess::Temporary);
                    assert_eq!(plan.bindings().len(), usize::from(binding == "n"));
                    let [source_names] = names.names().source_units() else {
                        panic!("one actual source name resolution");
                    };
                    assert_eq!(source_names.source_unit(), unit);
                    assert_eq!(source_names.resolution().source_id(), source_id);
                    let declared = source_names
                        .resolution()
                        .symbols()
                        .iter()
                        .filter(|symbol| symbol.span() == binding_span && !symbol.is_synthetic())
                        .collect::<Vec<_>>();
                    let expected_symbol = if binding == "n" {
                        let [local] = declared.as_slice() else {
                            panic!("unique AST loop binding")
                        };
                        assert_eq!(local.name(), "n");
                        let candidates = typed
                            .types()
                            .body_symbol_types()
                            .keys()
                            .filter(|symbol| {
                                symbol.source_unit() == unit && symbol.symbol() == local.id()
                            })
                            .copied()
                            .collect::<Vec<_>>();
                        let [symbol] = candidates.as_slice() else {
                            panic!("qualified AST loop binding type")
                        };
                        let [fact] = plan.bindings() else {
                            panic!("one loop binding fact")
                        };
                        assert_eq!(fact.symbol(), *symbol);
                        assert_eq!(fact.declaration_span(), binding_span);
                        assert_eq!(fact.kind(), OwnershipBindingKind::Shared);
                        assert_eq!(
                            owned.binding_kind(*symbol),
                            Some(OwnershipBindingKind::Shared)
                        );
                        assert!(!owned.formed_owned_locals().contains(symbol));
                        assert!(
                            owned
                                .drops()
                                .iter()
                                .all(|drop| drop.target() != UnitDropTarget::Named(*symbol))
                        );
                        Some(*symbol)
                    } else {
                        None
                    };

                    let body_point = transfer.map_or(
                        UnitDropPoint::AfterStatement(UnitStatementId::new(unit, body_statement)),
                        |id| UnitDropPoint::ControlTransfer(UnitExpressionId::new(unit, id)),
                    );
                    let body_kind = match body {
                        "" => Exit::Fallthrough,
                        "continue" => {
                            Exit::Continue(UnitExpressionId::new(unit, transfer.unwrap()))
                        }
                        "break" => Exit::Break(UnitExpressionId::new(unit, transfer.unwrap())),
                        "return" => Exit::Return(UnitExpressionId::new(unit, transfer.unwrap())),
                        _ => unreachable!(),
                    };
                    let expected_exits = [
                        (body_kind, body_point),
                        (Exit::Exhaustion, UnitDropPoint::LoopExit(statement)),
                    ];
                    assert_eq!(plan.exits().len(), expected_exits.len());
                    let terminal_body = matches!(body_kind, Exit::Break(_) | Exit::Return(_));
                    // No factory-branch temporary survives separately from the selected source.
                    assert_eq!(owned.drops().len(), 1 + usize::from(terminal_body));
                    assert!(owned.drops().iter().all(|drop| {
                        drop.target() == UnitDropTarget::Temporary(source)
                            && drop.value_origin() == source_span
                    }));
                    for (kind, point) in expected_exits {
                        let matching = plan
                            .exits()
                            .iter()
                            .filter(|exit| exit.kind() == kind && exit.point() == point)
                            .collect::<Vec<_>>();
                        let [exit] = matching.as_slice() else {
                            panic!("unique exact exit: {text}")
                        };
                        assert_eq!(owned.iteration_cleanup_at(point), Some(exit.actions()));
                        let exhausted = kind == Exit::Exhaustion;
                        let terminal =
                            exhausted || matches!(kind, Exit::Break(_) | Exit::Return(_));
                        let mut expected = Vec::new();
                        if !exhausted {
                            if let Some(symbol) = expected_symbol {
                                expected.push(Action::EndBinding { statement, symbol });
                            }
                            expected.push(Action::EndElement(statement));
                        }
                        if terminal {
                            expected.push(Action::FinishProvider(statement));
                            expected.push(Action::EndSource(statement));
                            let matching = owned
                                .drops()
                                .iter()
                                .filter(|drop| drop.point() == point)
                                .collect::<Vec<_>>();
                            let [drop] = matching.as_slice() else {
                                panic!("one source drop per terminal edge")
                            };
                            expected.push(Action::Drop(**drop));
                        } else {
                            assert!(
                                owned.drops().iter().all(|drop| drop.point() != point),
                                "continue/fallthrough must preserve provider and selected owner"
                            );
                        }
                        assert_eq!(exit.actions(), expected.as_slice(), "{text}: {kind:?}");
                    }
                }
            }
        }
    }
}
