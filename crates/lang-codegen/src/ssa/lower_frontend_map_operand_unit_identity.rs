//! Original operand sources consume one admitted Unit analysis and its actual SSA.
//! Unit publishes ordered provider exits, not the Legacy closure-phi table. The
//! unchanged operand oracle checks the real Entry/Fallthrough/Exhaustion transport
//! and the terminal Return instead of manufacturing a compatibility graph.

use std::collections::BTreeSet;

use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{EntityId, FunctionId, LoanKind, Operation, Origin, Program},
};
use lang_frontend::{
    name_resolution::{SourceUnitId, SourceUnitKey, SymbolKind, UnitReferenceTarget, UnitSymbolId},
    ownership_checking::{
        IterationSource, OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitCallableOrigin,
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as Action,
        UnitIterationExitKind as Exit, UnitIterationSourceAccess, UnitLoanTarget,
        UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    },
    parser::{Expression, ForBinding, Item, NameMarker, Statement, VariableKind},
    source::Span,
    type_checking::{
        ContainerConstructionKind, ExpressionCategory, IterationProvider, ParameterMode,
        SequentialContainerKind, UnitCallReceiverOrigin, UnitCallTarget, UnitExpressionId,
        UnitSequentialIterationBinding, UnitStatementId,
    },
};

use super::super::structure::{before, definition, only, source_function};

fn one<'a, T>(values: &'a [T], label: &str) -> &'a T {
    let [value] = values else {
        panic!("expected exactly one {label}, got {}", values.len());
    };
    value
}

fn within(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn source_unit(unit: &UnitSource) -> SourceUnitId {
    let source = one(
        unit.names().names().index().source_units(),
        "original source unit",
    );
    assert_eq!(source.source_id(), unit.parsed().source_id());
    assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
    source.id()
}

fn expression_span(unit: &UnitSource, expression: UnitExpressionId) -> Span {
    assert_eq!(expression.source_unit(), source_unit(unit));
    unit.parsed()
        .ast()
        .expressions()
        .get(expression.expression())
        .unwrap()
        .span()
}

fn symbol_span(unit: &UnitSource, symbol: UnitSymbolId) -> Span {
    assert_eq!(symbol.source_unit(), source_unit(unit));
    let sources = unit
        .names()
        .names()
        .source_units()
        .iter()
        .filter(|source| source.source_unit() == symbol.source_unit())
        .collect::<Vec<_>>();
    let symbols = one(&sources, "same source resolution")
        .resolution()
        .symbols()
        .iter()
        .filter(|candidate| candidate.id() == symbol.symbol())
        .collect::<Vec<_>>();
    let symbol = one(&symbols, "qualified original symbol");
    assert!(!symbol.is_synthetic());
    symbol.span()
}

fn receiver_symbol(unit: &UnitSource, receiver: UnitExpressionId) -> UnitSymbolId {
    let span = expression_span(unit, receiver);
    assert_eq!(unit.sources().slice(span).unwrap(), "cache");
    let references = unit
        .names()
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == receiver.source_unit() && reference.span() == span
        })
        .collect::<Vec<_>>();
    let UnitReferenceTarget::Symbol(symbol) = one(&references, "exact cache reference").target()
    else {
        panic!("the Map receiver must resolve to its actual local binding");
    };
    *symbol
}

struct Operand {
    scope: Span,
    call: UnitExpressionId,
    cache: UnitSymbolId,
    construction: UnitExpressionId,
    remove: bool,
}

fn assert_operand(unit: &UnitSource, returned: UnitExpressionId, scope: Span) -> Operand {
    let parsed = unit.parsed();
    let source = source_unit(unit);
    let Expression::Return {
        value: Some(operand),
        ..
    } = parsed
        .ast()
        .expressions()
        .get(returned.expression())
        .unwrap()
        .payload()
    else {
        panic!("original Return must carry its conditional operand");
    };
    let Expression::If { condition, .. } =
        parsed.ast().expressions().get(*operand).unwrap().payload()
    else {
        panic!("the unchanged Return operand is an If");
    };
    let condition = UnitExpressionId::new(source, *condition);
    let (call, binary) = match parsed
        .ast()
        .expressions()
        .get(condition.expression())
        .unwrap()
        .payload()
    {
        Expression::Binary { left, .. } => (UnitExpressionId::new(source, *left), Some(condition)),
        Expression::Call { .. } => (condition, None),
        _ => panic!("original remove presence or contains condition"),
    };
    let typed = unit.types();
    let (receiver, remove) = if let Some(descriptor) = typed.map_remove(call) {
        assert!(typed.map_contains(call).is_none());
        (descriptor.receiver(), true)
    } else {
        let descriptor = typed.map_contains(call).unwrap();
        (descriptor.receiver(), false)
    };
    let map_type = typed.expression_type(receiver).unwrap();
    assert_eq!(remove, binary.is_some());
    let cache = receiver_symbol(unit, receiver);
    let declaration = symbol_span(unit, cache);
    assert!(within(declaration, scope));
    let initializers = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, item)| match item.payload() {
            Item::Variable {
                kind: VariableKind::Var,
                name: NameMarker::Present(name),
                initializer,
                ..
            } if *name == declaration && within(item.span(), scope) => Some(*initializer),
            _ => None,
        })
        .collect::<Vec<_>>();
    let initializer = UnitExpressionId::new(source, *one(&initializers, "cache declaration"));
    let construction = typed.map_construction(initializer).unwrap();
    assert_eq!(construction.map_type(), map_type);
    assert_eq!(typed.symbol_type(cache), Some(map_type));
    assert_eq!(typed.expression_type(receiver), Some(map_type));

    let owned = unit.ownership();
    let contract = owned.call_receiver_contract(call).unwrap();
    assert_eq!(
        contract.source(),
        UnitCallReceiverOrigin::Expression(receiver)
    );
    assert_eq!(contract.target(), UnitCallTarget::FunctionValue);
    assert_eq!(contract.receiver_type(), map_type);
    assert_eq!(contract.category(), ExpressionCategory::Place);
    assert_eq!(contract.receiver_span(), expression_span(unit, receiver));
    assert_eq!(contract.call_span(), expression_span(unit, call));
    assert_eq!(
        contract.kind(),
        if remove {
            UnitCallArgumentOwnershipKind::ExclusiveLoan
        } else {
            UnitCallArgumentOwnershipKind::SharedLoan
        }
    );
    let receiver_fact = owned.receiver_fact(call).unwrap();
    assert_eq!(receiver_fact.source(), contract.source());
    assert_eq!(receiver_fact.receiver_type(), map_type);
    assert_eq!(receiver_fact.begin_span(), contract.receiver_span());
    assert_eq!(receiver_fact.end_span(), contract.call_span());
    let UnitReceiverOwnershipTarget::Place(place) = receiver_fact.target() else {
        panic!("actual Map receiver root");
    };
    assert!(place.is_root());
    assert_eq!(place.root(), cache);
    assert_eq!(
        receiver_fact.kind(),
        if remove {
            UnitReceiverOwnershipKind::ExclusiveLoan
        } else {
            UnitReceiverOwnershipKind::SharedLoan
        }
    );
    assert_eq!(receiver_fact.is_receiver_reservation(), remove);
    assert_eq!(receiver_fact.activation_point(), remove.then_some(call));
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| {
            drop.point() == UnitDropPoint::CallReturn(call)
                && drop.target() == UnitDropTarget::Named(cache)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        one(&drops, "operand's exact Named Map drop").value_origin(),
        declaration
    );
    assert!(
        !owned.drops().iter().any(|drop| {
            drop.point() == UnitDropPoint::ControlTransfer(returned)
                && (drop.target() == UnitDropTarget::Named(cache)
                    || drop.target()
                        == UnitDropTarget::Temporary(UnitExpressionId::new(source, *operand)))
        }),
        "operand completes its Map cleanup before Return; returned result is not dropped"
    );
    if let Some(binary) = binary {
        let nullable_drops = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(call))
            .collect::<Vec<_>>();
        let nullable_drop = one(&nullable_drops, "independent remove nullable result drop");
        assert_eq!(
            nullable_drop.point(),
            UnitDropPoint::AfterBinaryOperands(binary)
        );
        assert_eq!(nullable_drop.value_origin(), expression_span(unit, call));
        assert_ne!(nullable_drop.target(), UnitDropTarget::Named(cache));
    }
    Operand {
        scope,
        call,
        cache,
        construction: initializer,
        remove,
    }
}

fn assert_iterations(unit: &UnitSource, expected: usize) -> Vec<Operand> {
    let parsed = unit.parsed();
    let source = source_unit(unit);
    let typed = unit.types();
    let owned = unit.ownership();
    let mut statements = BTreeSet::new();
    let mut operands = Vec::new();
    for (id, node) in parsed.ast().statements().iter() {
        let Statement::For {
            binding,
            source: provider,
            body,
            ..
        } = node.payload()
        else {
            continue;
        };
        let statement = UnitStatementId::new(source, id);
        assert!(statements.insert(statement));
        let provider = UnitExpressionId::new(source, *provider);
        let descriptor = typed.sequential_iteration(statement).unwrap();
        let plan = owned.iteration(statement).unwrap();
        assert_eq!(plan.descriptor(), descriptor);
        assert_eq!(descriptor.statement(), statement);
        assert_eq!(descriptor.source(), provider);
        assert_eq!(
            descriptor.provider(),
            IterationProvider::Sequential(SequentialContainerKind::Array)
        );
        assert_eq!(descriptor.delivery(), ParameterMode::Borrow);
        assert_eq!(
            typed.expression_type(provider),
            Some(descriptor.source_type())
        );
        assert_eq!(
            typed.expression_category(provider),
            Some(ExpressionCategory::Temporary)
        );
        assert_eq!(plan.source_access(), UnitIterationSourceAccess::Temporary);
        assert!(
            matches!(plan.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Temporary(actual)) if *actual == provider)
        );
        let ForBinding::Name(NameMarker::Present(name)) = binding else {
            panic!("original for has one named item");
        };
        let UnitSequentialIterationBinding::Name(item) = descriptor.binding() else {
            panic!("typed provider retains the original item binding");
        };
        assert_eq!(symbol_span(unit, *item), *name);
        assert_eq!(unit.sources().slice(*name).unwrap(), "item");
        assert_eq!(typed.symbol_type(*item), Some(descriptor.element_type()));
        assert_eq!(
            owned.binding_kind(*item),
            Some(OwnershipBindingKind::Shared)
        );
        let binding = one(plan.bindings(), "actual Shared iteration binding");
        assert_eq!(binding.symbol(), *item);
        assert_eq!(binding.kind(), OwnershipBindingKind::Shared);
        assert_eq!(binding.declaration_span(), *name);

        let returns = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, expression)| {
                (matches!(expression.payload(), Expression::Return { .. })
                    && within(expression.span(), node.span()))
                .then_some(UnitExpressionId::new(source, id))
            })
            .collect::<Vec<_>>();
        let returned = *one(&returns, "original conditional Return inside this for");
        let scopes = parsed
            .roots()
            .iter()
            .map(|root| parsed.ast().items().get(*root).unwrap())
            .filter(|item| {
                matches!(item.payload(), Item::Function { .. }) && within(node.span(), item.span())
            })
            .collect::<Vec<_>>();
        let operand = assert_operand(
            unit,
            returned,
            one(&scopes, "enclosing real function").span(),
        );
        let cache = operand.cache;
        assert_eq!(
            plan.exits().len(),
            3,
            "the actual Return, Fallthrough and Exhaustion"
        );
        for (kind, point) in [
            (
                Exit::Return(returned),
                UnitDropPoint::ControlTransfer(returned),
            ),
            (
                Exit::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(source, *body)),
            ),
            (Exit::Exhaustion, UnitDropPoint::LoopExit(statement)),
        ] {
            let exits = plan
                .exits()
                .iter()
                .filter(|exit| exit.kind() == kind)
                .collect::<Vec<_>>();
            let exit = one(&exits, "original provider exit kind");
            assert_eq!(exit.point(), point);
            let mut actions = Vec::new();
            if kind != Exit::Exhaustion {
                actions.push(Action::EndBinding {
                    statement,
                    symbol: *item,
                });
                actions.push(Action::EndElement(statement));
            }
            if kind != Exit::Fallthrough {
                actions.push(Action::FinishProvider(statement));
                actions.push(Action::EndSource(statement));
                let drops = owned
                    .drops()
                    .iter()
                    .filter(|drop| {
                        drop.point() == point
                            && drop.target() == UnitDropTarget::Temporary(provider)
                    })
                    .collect::<Vec<_>>();
                let drop = **one(&drops, "original hidden source owner at this exit");
                assert_eq!(drop.value_origin(), expression_span(unit, provider));
                actions.push(Action::Drop(drop));
            }
            // Exhaustion may also release the dead Map; its exact Named identity
            // and actual SSA transport are checked separately, never by its type.
            let actual = exit.actions();
            assert!(actual.starts_with(&actions), "{kind:?}: {actual:?}");
            if kind == Exit::Exhaustion {
                for action in &actual[actions.len()..] {
                    let Action::Drop(drop) = action else {
                        panic!("only the exact dead Map drop may follow provider exhaustion");
                    };
                    assert_eq!(drop.target(), UnitDropTarget::Named(cache));
                    assert_eq!(drop.point(), point);
                    assert_eq!(drop.value_origin(), symbol_span(unit, cache));
                    assert!(owned.drops().contains(drop));
                }
                assert!(actual.len() <= actions.len() + 1);
            } else {
                assert_eq!(actual, actions);
            }
            assert_eq!(owned.iteration_cleanup_at(point), Some(actual));
        }
        operands.push(operand);
    }
    assert_eq!(statements.len(), expected);
    assert_eq!(owned.iterations().len(), expected);
    assert_eq!(typed.sequential_iterations().len(), expected);
    operands
}

// Join the published Named/Temporary identities to actual SSA operation results.
// The unchanged structural oracle additionally checks every incoming owner edge,
// complete cleanup ordering, latest Map version and Return exclusion.
fn assert_operand_origins(unit: &UnitSource, program: &Program, operands: &[Operand]) {
    for operand in operands {
        let (_, function) = source_function(program, operand.scope);
        let construction = only(function, |instruction| {
            matches!(instruction.operation, Operation::MapConstruct { .. })
        });
        assert_eq!(
            construction.origin,
            Origin::Source(expression_span(unit, operand.construction))
        );
        let call = only(function, |instruction| {
            instruction.origin == Origin::Source(expression_span(unit, operand.call))
                && matches!(
                    instruction.operation,
                    Operation::MapRemove { .. } | Operation::MapContains { .. }
                )
        });
        let (current, nullable) = match call.operation {
            Operation::MapRemove { .. } => {
                assert!(operand.remove);
                assert_eq!(call.results.len(), 2);
                (call.results[0], Some(call.results[1]))
            }
            Operation::MapContains { owner, .. } => {
                assert!(!operand.remove);
                (owner, None)
            }
            _ => unreachable!(),
        };
        let drop = only(
            function,
            |instruction| matches!(instruction.operation, Operation::Drop { owner } if EntityId::Value(owner) == current),
        );
        assert_eq!(
            drop.origin,
            Origin::Source(symbol_span(unit, operand.cache))
        );
        if let Some(nullable) = nullable {
            let drop = only(
                function,
                |instruction| matches!(instruction.operation, Operation::Drop { owner } if EntityId::Value(owner) == nullable),
            );
            assert_eq!(
                drop.origin,
                Origin::Source(expression_span(unit, operand.call))
            );
        }
        // The live tail query is an original obligation too, not an optional
        // arbitrary same-type query found in the emitted function.
        for descriptor in unit.types().map_contains_calls() {
            let span = expression_span(unit, descriptor.expression());
            if !within(span, operand.scope) {
                continue;
            }
            assert_eq!(receiver_symbol(unit, descriptor.receiver()), operand.cache);
            only(function, |instruction| {
                instruction.origin == Origin::Source(span)
                    && matches!(instruction.operation, Operation::MapContains { .. })
            });
        }
    }
}

fn assert_source_functions(
    unit: &UnitSource,
    program: &Program,
    entry: FunctionId,
) -> BTreeSet<FunctionId> {
    let source = source_unit(unit);
    let parsed = unit.parsed();
    let mut functions = BTreeSet::new();
    for root in parsed.roots() {
        let node = parsed.ast().items().get(*root).unwrap();
        let Item::Function {
            name: NameMarker::Present(name),
            ..
        } = node.payload()
        else {
            panic!("the unchanged operand fixture has only top-level functions");
        };
        let declarations = unit
            .names()
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.root() == *root
                    && declaration.source_unit() == source
                    && declaration.kind() == SymbolKind::Function
            })
            .collect::<Vec<_>>();
        let declaration = one(&declarations, "real top-level function declaration");
        assert_eq!(declaration.name_span(), *name);
        assert_eq!(unit.declaration_span(declaration.id()), node.span());
        let (_, function) = source_function(program, node.span());
        assert!(
            functions.insert(function.id),
            "distinct original function roots"
        );

        // All original calls, including main's 17 confirm/operand pairs, retain
        // their actual declaration target. Entry-rooted planning cannot trim them.
        for descriptor in unit.types().calls() {
            let span = expression_span(unit, descriptor.expression());
            if !within(span, node.span()) {
                continue;
            }
            let UnitCallTarget::Declaration(target) = descriptor.target() else {
                continue;
            };
            let (_, callee) = source_function(program, unit.declaration_span(target));
            let call = only(function, |instruction| {
                instruction.origin == Origin::Source(span)
                    && matches!(instruction.operation, Operation::DirectCall { .. })
            });
            assert!(
                matches!(call.operation, Operation::DirectCall { callee: actual, receiver: None, .. } if actual == callee.id)
            );
        }
    }
    assert!(functions.contains(&entry));
    functions
}

fn assert_runtime_initializers(unit: &UnitSource, program: &Program) -> BTreeSet<FunctionId> {
    let parsed = unit.parsed();
    let source = source_unit(unit);
    let mut callbacks = BTreeSet::new();
    let mut lambdas = BTreeSet::new();
    for descriptor in unit.types().container_constructions() {
        if descriptor.kind() != ContainerConstructionKind::RuntimeLength {
            continue;
        }
        assert_eq!(descriptor.container(), SequentialContainerKind::Array);
        assert_eq!(
            descriptor.parameter_modes(),
            [ParameterMode::Borrow, ParameterMode::Borrow]
        );
        let call = descriptor.expression();
        let call_span = expression_span(unit, call);
        let Expression::Call { arguments, .. } = parsed
            .ast()
            .expressions()
            .get(call.expression())
            .unwrap()
            .payload()
        else {
            panic!("the exact runtime Array construction Call");
        };
        let [_, initializer] = arguments.as_slice() else {
            panic!("the original length and lambda operands");
        };
        let initializer = UnitExpressionId::new(source, initializer.value);
        let lambda_span = expression_span(unit, initializer);
        assert!(matches!(
            parsed
                .ast()
                .expressions()
                .get(initializer.expression())
                .unwrap()
                .payload(),
            Expression::Lambda { .. }
        ));
        assert!(lambdas.insert(initializer));
        let provenance = unit.ownership().callable_origin(initializer).unwrap();
        assert_eq!(provenance.expression(), initializer);
        assert_eq!(provenance.origin(), UnitCallableOrigin::Lambda(initializer));
        assert_eq!(provenance.span(), lambda_span);
        assert!(unit.ownership().closure(initializer).is_some());
        let loans = unit
            .ownership()
            .loans()
            .iter()
            .filter(|loan| loan.call() == call && loan.argument() == initializer)
            .collect::<Vec<_>>();
        let loan = one(&loans, "runtime initializer's actual synchronous loan");
        assert_eq!(
            loan.kind(),
            lang_frontend::ownership_checking::LoanKind::Shared
        );
        assert_eq!(loan.target(), &UnitLoanTarget::Temporary(initializer));
        assert_eq!(loan.begin_span(), lambda_span);
        assert_eq!(loan.end_span(), call_span);
        let owners = parsed
            .roots()
            .iter()
            .map(|root| parsed.ast().items().get(*root).unwrap())
            .filter(|node| within(call_span, node.span()))
            .collect::<Vec<_>>();
        let (module, function) = source_function(
            program,
            one(&owners, "initializer's source function").span(),
        );
        let generate = only(function, |instruction| {
            instruction.origin == Origin::Source(call_span)
                && matches!(
                    instruction.operation,
                    Operation::ContainerGenerateBorrowed { .. }
                )
        });
        let Operation::ContainerGenerateBorrowed {
            initializer: borrowed,
            ..
        } = generate.operation
        else {
            unreachable!()
        };
        let borrow = definition(function, EntityId::Loan(borrowed));
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = borrow.operation
        else {
            panic!("runtime generator consumes the actual initializer Shared loan");
        };
        let root = definition(function, EntityId::Place(place));
        let Operation::RootPlace { owner } = root.operation else {
            panic!("the callback loan is rooted in its exact function value");
        };
        let address = definition(function, EntityId::Value(owner));
        let Operation::FunctionAddress { target } = address.operation else {
            panic!("these original uncaptured Array lambdas have function addresses");
        };
        assert_eq!(address.origin, Origin::Source(lambda_span));
        let callback = module.function(target).unwrap();
        assert_eq!(callback.origin, Origin::Source(lambda_span));
        assert_eq!(source_function(program, lambda_span).1.id, target);
        assert!(
            callbacks.insert(target),
            "each original lambda has its own actual body"
        );
        before(function, address, root);
        before(function, root, borrow);
        before(function, borrow, generate);
        let end = only(
            function,
            |instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == borrowed),
        );
        before(function, generate, end);
    }
    let original_lambdas = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::Lambda { .. })
                .then_some(UnitExpressionId::new(source, id))
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        lambdas, original_lambdas,
        "all original Array initializer bodies remain reachable"
    );
    callbacks
}

pub(super) fn check(text: &str, entry_name: &str, expected: usize) {
    let unit = UnitSource::analyze("lowering.ko", text).unwrap();
    unit.assert_frontend_clean();
    assert_eq!(
        unit.sources()
            .source_name(unit.parsed().source_id())
            .unwrap(),
        "lowering.ko"
    );
    assert_eq!(
        unit.sources()
            .source_text(unit.parsed().source_id())
            .unwrap(),
        text
    );
    let operands = assert_iterations(&unit, expected);
    let declaration = unit.declaration(entry_name, SymbolKind::Function);
    let (program, entry) = unit.lower_entry(declaration).unwrap();
    assert_eq!(
        source_function(&program, unit.declaration_span(declaration))
            .1
            .id,
        entry
    );
    assert_operand_origins(&unit, &program, &operands);
    let mut bodies = assert_source_functions(&unit, &program, entry);
    let callbacks = assert_runtime_initializers(&unit, &program);
    assert!(bodies.is_disjoint(&callbacks));
    if entry_name == "main" {
        assert_eq!(bodies.len(), 6, "all six original top-level functions");
        assert_eq!(
            callbacks.len(),
            4,
            "all four original runtime Array lambdas"
        );
    } else {
        assert_eq!(
            bodies.len(),
            1,
            "the original parameterized updatingOperand, no wrapper"
        );
        assert!(callbacks.is_empty());
    }
    bodies.extend(callbacks);
    assert_eq!(
        bodies,
        program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .map(|function| function.id)
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        super::super::operand_structure::assert_source(&program, unit.parsed()),
        expected
    );
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
}
