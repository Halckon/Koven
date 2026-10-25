//! Original same-guard source: real Unit ownership and six-block SSA, no Legacy graph.
use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{Origin, SsaTypeKind},
};
use lang_frontend::{
    name_resolution::{SourceUnitId, SourceUnitKey, SymbolKind, UnitReferenceTarget, UnitSymbolId},
    ownership_checking::{
        IterationSource, OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitDropPoint,
        UnitDropTarget, UnitIterationCleanupAction as Action, UnitIterationExitKind as Exit,
        UnitIterationSourceAccess, UnitLoanTarget, UnitReceiverOwnershipKind,
        UnitReceiverOwnershipTarget,
    },
    parser::{
        BinaryOperator as AstBinary, Expression, ForBinding, Item, NameMarker, Statement,
        VariableKind,
    },
    source::Span,
    type_checking::{
        BuiltinType, ExpressionCategory, IterationProvider, ParameterMode, SequentialContainerKind,
        UnitCallReceiverOrigin, UnitCallTarget, UnitExpressionId, UnitSequentialIterationBinding,
        UnitStatementId, UnitTypeKind,
    },
};

#[path = "lower_frontend_map_same_guard_ssa_test_support.rs"]
mod ssa;

fn one<T>(values: &[T]) -> &T {
    let [value] = values else {
        panic!("one exact original identity, got {}", values.len());
    };
    value
}

fn source_unit(unit: &UnitSource) -> SourceUnitId {
    one(unit.names().names().index().source_units()).id()
}

fn span(unit: &UnitSource, expression: UnitExpressionId) -> Span {
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
    let resolution = one(unit.names().names().source_units()).resolution();
    let symbols = resolution
        .symbols()
        .iter()
        .filter(|candidate| candidate.id() == symbol.symbol())
        .collect::<Vec<_>>();
    let symbol = one(&symbols);
    assert!(!symbol.is_synthetic());
    symbol.span()
}

fn reference(unit: &UnitSource, expression: UnitExpressionId) -> UnitSymbolId {
    let origin = span(unit, expression);
    assert!(matches!(
        unit.parsed()
            .ast()
            .expressions()
            .get(expression.expression())
            .unwrap()
            .payload(),
        Expression::Name
    ));
    let references = unit
        .names()
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == expression.source_unit() && reference.span() == origin
        })
        .collect::<Vec<_>>();
    let UnitReferenceTarget::Symbol(symbol) = one(&references).target() else {
        panic!("original Name resolves to its source-qualified binding");
    };
    *symbol
}

struct Source {
    construction: UnitExpressionId,
    cache: UnitSymbolId,
    provider: UnitExpressionId,
    loop_span: Span,
    conditional: UnitExpressionId,
    condition: UnitExpressionId,
    left: UnitExpressionId,
    right: UnitExpressionId,
    returned: UnitExpressionId,
    operand: UnitExpressionId,
    last_return: UnitExpressionId,
    last_operand: UnitExpressionId,
    head: UnitExpressionId,
    tail: UnitExpressionId,
    query: UnitExpressionId,
    tail_query: UnitExpressionId,
}

fn receiver(
    unit: &UnitSource,
    call: UnitExpressionId,
    receiver: UnitExpressionId,
    cache: UnitSymbolId,
    shared: bool,
) {
    assert_eq!(reference(unit, receiver), cache);
    let owned = unit.ownership();
    let contract = owned.call_receiver_contract(call).unwrap();
    assert_eq!(
        contract.source(),
        UnitCallReceiverOrigin::Expression(receiver)
    );
    assert_eq!(contract.target(), UnitCallTarget::FunctionValue);
    assert_eq!(contract.category(), ExpressionCategory::Place);
    assert_eq!(
        contract.receiver_type(),
        unit.types().symbol_type(cache).unwrap()
    );
    assert_eq!(contract.receiver_span(), span(unit, receiver));
    assert_eq!(contract.call_span(), span(unit, call));
    assert_eq!(
        contract.kind(),
        if shared {
            UnitCallArgumentOwnershipKind::SharedLoan
        } else {
            UnitCallArgumentOwnershipKind::ExclusiveLoan
        }
    );
    let fact = owned.receiver_fact(call).unwrap();
    assert_eq!(fact.source(), contract.source());
    assert_eq!(fact.receiver_type(), contract.receiver_type());
    assert_eq!(fact.begin_span(), contract.receiver_span());
    assert_eq!(fact.end_span(), contract.call_span());
    assert_eq!(
        fact.kind(),
        if shared {
            UnitReceiverOwnershipKind::SharedLoan
        } else {
            UnitReceiverOwnershipKind::ExclusiveLoan
        }
    );
    assert_eq!(fact.is_receiver_reservation(), !shared);
    assert_eq!(fact.activation_point(), (!shared).then_some(call));
    let UnitReceiverOwnershipTarget::Place(place) = fact.target() else {
        panic!("independent receiver capability has an exact Map root");
    };
    assert!(place.is_root());
    assert_eq!(place.root(), cache);
}

fn facts(unit: &UnitSource, stop: UnitSymbolId) -> Source {
    let parsed = unit.parsed();
    let source = source_unit(unit);
    let qualify = |id| UnitExpressionId::new(source, id);
    let typed = unit.types();
    let owned = unit.ownership();
    let loops = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Statement::For { .. }).then_some((id, node))
        })
        .collect::<Vec<_>>();
    let (statement, node) = *one(&loops);
    let loop_span = node.span();
    let Statement::For {
        binding,
        source: provider,
        body,
        ..
    } = node.payload()
    else {
        unreachable!();
    };
    let statement = UnitStatementId::new(source, statement);
    let provider = qualify(*provider);
    let descriptor = one(typed.sequential_iterations());
    let plan = one(owned.iterations());
    assert_eq!(typed.sequential_iteration(statement), Some(descriptor));
    assert_eq!(owned.iteration(statement), Some(plan));
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
    let array = one(typed.container_constructions());
    assert_eq!(array.expression(), provider);
    assert_eq!(array.container(), SequentialContainerKind::Array);
    assert_eq!(array.container_type(), descriptor.source_type());
    assert_eq!(array.element_type(), descriptor.element_type());
    assert_eq!(plan.source_access(), UnitIterationSourceAccess::Temporary);
    assert!(matches!(plan.source_kind(),
        IterationSource::BorrowedSource(UnitLoanTarget::Temporary(actual)) if *actual == provider));
    assert_eq!(
        unit.sources().slice(span(unit, provider)).unwrap(),
        "arrayOf(1, 2, 3)"
    );
    let ForBinding::Name(NameMarker::Present(name)) = binding else {
        panic!("original item binding");
    };
    let UnitSequentialIterationBinding::Name(item) = descriptor.binding() else {
        panic!("real source-qualified element binding");
    };
    assert_eq!(symbol_span(unit, *item), *name);
    assert_eq!(unit.sources().slice(*name).unwrap(), "item");
    assert_eq!(typed.symbol_type(*item), Some(descriptor.element_type()));
    assert_eq!(typed.symbol_type(stop), Some(descriptor.element_type()));
    assert_eq!(
        typed.types().get(descriptor.element_type()),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    assert_ne!(*item, stop);
    assert_eq!(
        owned.binding_kind(*item),
        Some(OwnershipBindingKind::Shared)
    );
    let binding = one(plan.bindings());
    assert_eq!(binding.symbol(), *item);
    assert_eq!(binding.kind(), OwnershipBindingKind::Shared);
    assert_eq!(binding.declaration_span(), *name);

    let conditionals = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::If { .. }).then_some((qualify(id), node))
        })
        .collect::<Vec<_>>();
    let (conditional, node) = *one(&conditionals);
    let Expression::If {
        condition,
        then_branch,
        else_branch: None,
        ..
    } = node.payload()
    else {
        panic!("the original outer If has one then arm and an implicit false arm");
    };
    let condition = qualify(*condition);
    let Expression::Binary {
        left,
        operator: AstBinary::Equal,
        right,
        ..
    } = parsed
        .ast()
        .expressions()
        .get(condition.expression())
        .unwrap()
        .payload()
    else {
        panic!("the exact item == stop guard");
    };
    let (left, right) = (qualify(*left), qualify(*right));
    assert_eq!(reference(unit, left), *item);
    assert_eq!(reference(unit, right), stop);
    let then_span = parsed.ast().statements().get(*then_branch).unwrap().span();
    let mut returns = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::Return { .. }).then_some(qualify(id))
        })
        .collect::<Vec<_>>();
    returns.sort_by_key(|id| span(unit, *id).start());
    let [returned, last_return] = returns.as_slice() else {
        panic!("both original Return expressions, not a manufactured entry");
    };
    let returned_span = span(unit, *returned);
    assert!(then_span.start() < returned_span.start() && returned_span.end() < then_span.end());
    assert_eq!((returned_span.start(), returned_span.end()), (209, 220));
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
        panic!("Return has its original Name operand");
    };
    let operand = qualify(*operand);
    assert_eq!(reference(unit, operand), *item);
    assert_eq!(
        (span(unit, operand).start(), span(unit, operand).end()),
        (216, 220)
    );
    let Expression::Return {
        value: Some(last_operand),
        ..
    } = parsed
        .ast()
        .expressions()
        .get(last_return.expression())
        .unwrap()
        .payload()
    else {
        panic!("the final Return retains its original zero operand");
    };
    let last_operand = qualify(*last_operand);
    assert_eq!(unit.sources().slice(span(unit, last_operand)).unwrap(), "0");

    let construction = one(typed.map_constructions());
    let mut puts = typed.map_puts().iter().collect::<Vec<_>>();
    puts.sort_by_key(|put| span(unit, put.expression()).start());
    let [head, tail] = puts.as_slice() else {
        panic!("both original Map updates");
    };
    let cache = reference(unit, head.receiver());
    let cache_span = symbol_span(unit, cache);
    let cache_symbols = one(unit.names().names().source_units())
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.name() == "cache")
        .collect::<Vec<_>>();
    assert_eq!(one(&cache_symbols).id(), cache.symbol());
    assert_eq!((cache_span.start(), cache_span.end()), (46, 51));
    assert_eq!(unit.sources().slice(cache_span).unwrap(), "cache");
    assert_eq!(typed.symbol_type(cache), Some(construction.map_type()));
    let locals = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Variable {
                kind: VariableKind::Var,
                name: NameMarker::Present(name),
                initializer,
                ..
            } if *name == cache_span => Some(qualify(*initializer)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(*one(&locals), construction.expression());
    for put in [head, tail] {
        receiver(unit, put.expression(), put.receiver(), cache, false);
        assert_eq!(put.key_type(), construction.key_type());
        assert_eq!(put.value_type(), construction.value_type());
        assert_eq!(typed.expression_type(put.key()), Some(put.key_type()));
        assert_eq!(typed.expression_type(put.value()), Some(put.value_type()));
    }
    assert_eq!(reference(unit, head.key()), *item);
    let mut queries = typed.map_contains_calls().iter().collect::<Vec<_>>();
    queries.sort_by_key(|query| span(unit, query.expression()).start());
    let [query, tail_query] = queries.as_slice() else {
        panic!("inner and live-tail original contains calls");
    };
    assert_eq!(span(unit, query.expression()).start(), 187);
    assert_eq!(span(unit, query.expression()).end(), 207);
    assert!(span(unit, query.expression()).end() < returned_span.start());
    assert_ne!(query.expression(), operand);
    assert_eq!(reference(unit, query.key()), *item);
    for query in [query, tail_query] {
        assert!(typed.map_remove(query.expression()).is_none());
        receiver(unit, query.expression(), query.receiver(), cache, true);
        assert_eq!(query.key_type(), construction.key_type());
    }
    let key_loans = owned
        .loans()
        .iter()
        .filter(|loan| loan.call() == query.expression() && loan.argument() == query.key())
        .collect::<Vec<_>>();
    let key_loan = one(&key_loans);
    assert_eq!(
        key_loan.kind(),
        lang_frontend::ownership_checking::LoanKind::Shared
    );
    assert_eq!(key_loan.begin_span(), span(unit, query.key()));
    assert_eq!(key_loan.end_span(), span(unit, query.expression()));
    assert!(matches!(key_loan.target(), UnitLoanTarget::Place(place)
        if place.is_root() && place.root() == *item));
    assert_ne!(
        query.key(),
        query.receiver(),
        "key loan is not receiver ownership"
    );
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(cache))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 2);
    for call in [query.expression(), tail_query.expression()] {
        let exact = drops
            .iter()
            .filter(|drop| drop.point() == UnitDropPoint::CallReturn(call))
            .collect::<Vec<_>>();
        assert_eq!(one(&exact).value_origin(), cache_span);
    }
    assert_eq!(owned.drops().len(), 6);
    assert_eq!(owned.loans().len(), 4);
    assert_eq!(owned.receiver_facts().len(), 4);
    let expected = [
        (
            Exit::Return(*returned),
            UnitDropPoint::ControlTransfer(*returned),
        ),
        (
            Exit::Fallthrough,
            UnitDropPoint::AfterStatement(UnitStatementId::new(source, *body)),
        ),
        (Exit::Exhaustion, UnitDropPoint::LoopExit(statement)),
    ];
    assert_eq!(plan.exits().len(), expected.len());
    for (exit, (kind, point)) in plan.exits().iter().zip(expected) {
        assert_eq!(exit.kind(), kind);
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
            let source_drops = owned
                .drops()
                .iter()
                .filter(|drop| {
                    drop.point() == point && drop.target() == UnitDropTarget::Temporary(provider)
                })
                .collect::<Vec<_>>();
            let drop = **one(&source_drops);
            assert_eq!(drop.value_origin(), span(unit, provider));
            actions.push(Action::Drop(drop));
        }
        assert_eq!(exit.actions(), actions);
        assert_eq!(owned.iteration_cleanup_at(point), Some(exit.actions()));
        assert!(!exit.actions().iter().any(|action| {
            matches!(action, Action::Drop(drop) if drop.target() == UnitDropTarget::Named(cache))
        }));
    }
    Source {
        construction: construction.expression(),
        cache,
        provider,
        loop_span,
        conditional,
        condition,
        left,
        right,
        returned: *returned,
        operand,
        last_return: *last_return,
        last_operand,
        head: head.expression(),
        tail: tail.expression(),
        query: query.expression(),
        tail_query: tail_query.expression(),
    }
}

pub(super) fn check(text: &str) {
    assert_eq!(
        text.len(),
        303,
        "unchanged original source, not a generated wrapper"
    );
    let unit = UnitSource::analyze("lowering.ko", text).unwrap();
    unit.assert_frontend_clean();
    let index = unit.names().names().index();
    let source = one(index.source_units());
    assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(source.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(source.source_id()).unwrap(),
        "lowering.ko"
    );
    assert_eq!(
        unit.sources().source_text(source.source_id()).unwrap(),
        text
    );
    let declaration = one(index.declarations());
    assert_eq!(declaration.kind(), SymbolKind::Function);
    assert_eq!(declaration.name(), "updatingOperand");
    assert_eq!(declaration.source_unit(), source.id());
    assert_eq!(declaration.package(), source.package());
    assert_eq!(unit.parsed().roots(), &[declaration.root()]);
    let declaration_span = unit.declaration_span(declaration.id());
    assert_eq!((declaration_span.start(), declaration_span.end()), (0, 302));
    assert_eq!(declaration.name_span().source_id(), source.source_id());
    assert_eq!(
        (
            declaration.name_span().start(),
            declaration.name_span().end()
        ),
        (4, 19)
    );
    let callable = unit
        .types()
        .signatures()
        .declaration(declaration.id())
        .unwrap()
        .callable()
        .unwrap();
    assert!(callable.type_parameters().is_empty());
    let parameter = one(callable.parameters());
    assert_eq!(parameter.mode(), ParameterMode::Borrow);
    let stop = parameter.symbol().unwrap();
    assert_eq!(
        unit.sources().slice(symbol_span(&unit, stop)).unwrap(),
        "stop"
    );
    assert_eq!(unit.types().symbol_type(stop), Some(parameter.ty()));
    let source = facts(&unit, stop);
    let (program, entry) = unit.lower_entry(declaration.id()).unwrap();
    let functions = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect::<Vec<_>>();
    let function = one(&functions);
    assert_eq!(function.id, entry);
    assert_eq!(function.origin, Origin::Source(declaration_span));
    assert_eq!(function.blocks.len(), 6);
    assert_eq!(function.instructions.len(), 46);
    let module = program.module(entry.module()).unwrap();
    let stop = *one(&function
        .block(function.entry_block().unwrap())
        .unwrap()
        .parameters);
    assert_eq!(
        module.type_kind(function.entity(stop).unwrap().ty.semantic_type()),
        Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true
        })
    );
    ssa::assert_ssa(&unit, &source, function);
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
}
