//! Original M16/M17 bytes, one admitted MemoryDocument and every real function root.
//! Unit exposes ordered provider cleanup, not Legacy closure-phi transport tables.
//! The nullable factory lowers; unsupported operand comparisons retain their frontend obligations.

use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::{LoweringErrorKind, model::Origin},
};
use lang_frontend::{
    name_resolution::{
        DeclarationId, SourceUnitId, SourceUnitKey, SymbolId, SymbolKind, UnitReferenceTarget,
        UnitSymbolId,
    },
    ownership_checking::{
        IterationSource, OwnershipBindingKind, UnitDropFact, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as Action, UnitIterationExitKind as Exit,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::{Expression, ForBinding, Item, NameMarker, Statement, VariableKind},
    source::Span,
    type_checking::{
        BuiltinType, Copyability, ExpressionCategory, IterationProvider, ParameterMode,
        SequentialContainerKind, UnitCallTarget, UnitExpressionId, UnitSequentialIterationBinding,
        UnitStatementId, UnitTypeKind,
    },
};

#[path = "lower_frontend_map_item_operand_ssa_test_support.rs"]
mod item_ssa;

fn one<'a, T>(values: &'a [T], label: &str) -> &'a T {
    let [value] = values else {
        panic!("expected exactly one {label}, got {}", values.len());
    };
    value
}

fn source(unit: &UnitSource) -> SourceUnitId {
    let source = one(
        unit.names().names().index().source_units(),
        "MemoryDocument",
    );
    assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(source.source_id(), unit.parsed().source_id());
    source.id()
}

fn within(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn expression_span(unit: &UnitSource, expression: UnitExpressionId) -> Span {
    assert_eq!(expression.source_unit(), source(unit));
    unit.parsed()
        .ast()
        .expressions()
        .get(expression.expression())
        .unwrap()
        .span()
}

fn symbol_span(unit: &UnitSource, symbol: UnitSymbolId) -> Span {
    assert_eq!(symbol.source_unit(), source(unit));
    let resolution = one(unit.names().names().source_units(), "original resolution");
    assert_eq!(resolution.source_unit(), source(unit));
    let symbols = resolution
        .resolution()
        .symbols()
        .iter()
        .filter(|candidate| candidate.id() == symbol.symbol())
        .collect::<Vec<_>>();
    let symbol = one(&symbols, "actual qualified symbol");
    assert!(!symbol.is_synthetic());
    symbol.span()
}

fn reference_symbol(unit: &UnitSource, expression: UnitExpressionId) -> UnitSymbolId {
    let span = expression_span(unit, expression);
    let references = unit
        .names()
        .names()
        .references()
        .iter()
        .filter(|reference| reference.source_unit() == source(unit) && reference.span() == span)
        .collect::<Vec<_>>();
    let UnitReferenceTarget::Symbol(symbol) = one(&references, "exact source reference").target()
    else {
        panic!("the original local reference must resolve to its real symbol");
    };
    *symbol
}

fn qualified_symbol(unit: &UnitSource, local: SymbolId) -> UnitSymbolId {
    let source = source(unit);
    let mut symbols = unit
        .names()
        .names()
        .references()
        .iter()
        .filter_map(|reference| match reference.target() {
            UnitReferenceTarget::Symbol(symbol)
                if reference.source_unit() == source
                    && symbol.source_unit() == source
                    && symbol.symbol() == local =>
            {
                Some(*symbol)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    // Multiple real uses must identify the same published source-qualified local.
    symbols.sort_unstable();
    symbols.dedup();
    *one(&symbols, "original source-qualified symbol")
}

fn roots(unit: &UnitSource, supported: bool) -> Vec<DeclarationId> {
    let index = unit.names().names().index();
    let source = one(index.source_units(), "original source");
    let mut declarations = index.declarations().iter().collect::<Vec<_>>();
    declarations.sort_by_key(|declaration| declaration.name_span().start());
    let expected: &[&str] = if supported {
        &["updatingOperand"]
    } else {
        &["nullableValue", "updatingOperand"]
    };
    assert_eq!(unit.parsed().roots().len(), expected.len());
    assert_eq!(declarations.len(), expected.len());
    assert_eq!(
        declarations
            .iter()
            .map(|declaration| declaration.name())
            .collect::<Vec<_>>(),
        expected,
        "every original non-generic function is an independent root"
    );
    declarations
        .into_iter()
        .map(|declaration| {
            assert_eq!(declaration.kind(), SymbolKind::Function);
            assert_eq!(declaration.source_unit(), source.id());
            assert_eq!(declaration.package(), source.package());
            assert_eq!(declaration.name_span().source_id(), source.source_id());
            assert!(unit.parsed().roots().contains(&declaration.root()));
            let node = unit.parsed().ast().items().get(declaration.root()).unwrap();
            let Item::Function {
                name: NameMarker::Present(name),
                ..
            } = node.payload()
            else {
                panic!("the root is its original named function");
            };
            assert_eq!(declaration.name_span(), *name);
            assert_eq!(unit.declaration_span(declaration.id()), node.span());
            let signature = unit
                .types()
                .signatures()
                .declaration(declaration.id())
                .and_then(|signature| signature.callable())
                .unwrap();
            assert!(signature.type_parameters().is_empty());
            assert_eq!(
                signature.parameters().len(),
                1,
                "no synthetic zero-argument wrapper"
            );
            declaration.id()
        })
        .collect()
}

struct LoopIdentity {
    statement: UnitStatementId,
    body: UnitStatementId,
    provider: UnitExpressionId,
    item: UnitSymbolId,
    returned: UnitExpressionId,
    operand: UnitExpressionId,
    scope: Span,
}

fn iteration(unit: &UnitSource) -> LoopIdentity {
    let parsed = unit.parsed();
    let source = source(unit);
    let loops = parsed
        .ast()
        .statements()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Statement::For { .. }))
        .collect::<Vec<_>>();
    let (id, node) = *one(&loops, "original for");
    let Statement::For {
        binding,
        source: provider,
        body,
        ..
    } = node.payload()
    else {
        unreachable!()
    };
    let statement = UnitStatementId::new(source, id);
    let provider = UnitExpressionId::new(source, *provider);
    let descriptor = one(unit.types().sequential_iterations(), "typed Array provider");
    let plan = one(unit.ownership().iterations(), "owned Array provider");
    assert_eq!(descriptor.statement(), statement);
    assert_eq!(descriptor.source(), provider);
    assert_eq!(
        unit.types().sequential_iteration(statement),
        Some(descriptor)
    );
    assert_eq!(unit.ownership().iteration(statement), Some(plan));
    assert_eq!(plan.descriptor(), descriptor);
    assert_eq!(
        descriptor.provider(),
        IterationProvider::Sequential(SequentialContainerKind::Array)
    );
    assert_eq!(descriptor.delivery(), ParameterMode::Borrow);
    assert_eq!(
        unit.types().expression_type(provider),
        Some(descriptor.source_type())
    );
    assert_eq!(
        unit.types().expression_category(provider),
        Some(ExpressionCategory::Temporary)
    );
    assert_eq!(
        unit.types().copyability(descriptor.source_type()),
        Copyability::MoveOnly,
        "the temporary Array owner remains even when there is no user Map"
    );
    assert_eq!(
        unit.sources()
            .slice(expression_span(unit, provider))
            .unwrap(),
        "arrayOf(1, 2, 3)"
    );
    let construction = one(
        unit.types().container_constructions(),
        "original Array construction",
    );
    assert_eq!(construction.expression(), provider);
    assert_eq!(construction.container(), SequentialContainerKind::Array);
    assert_eq!(construction.container_type(), descriptor.source_type());
    assert_eq!(construction.element_type(), descriptor.element_type());
    assert_eq!(plan.source_access(), UnitIterationSourceAccess::Temporary);
    assert!(matches!(
        plan.source_kind(),
        IterationSource::BorrowedSource(UnitLoanTarget::Temporary(actual)) if *actual == provider
    ));
    let ForBinding::Name(NameMarker::Present(name)) = binding else {
        panic!("the original for has a named item");
    };
    let UnitSequentialIterationBinding::Name(item) = descriptor.binding() else {
        panic!("the typed provider retains the original item");
    };
    assert_eq!(symbol_span(unit, *item), *name);
    assert_eq!(unit.sources().slice(*name).unwrap(), "item");
    assert_eq!(
        unit.types().symbol_type(*item),
        Some(descriptor.element_type())
    );
    assert_eq!(
        unit.types().types().get(descriptor.element_type()),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    assert_eq!(
        unit.types().copyability(descriptor.element_type()),
        Copyability::Copyable
    );
    let binding = one(plan.bindings(), "actual Shared item binding");
    assert_eq!(binding.symbol(), *item);
    assert_eq!(binding.declaration_span(), *name);
    assert_eq!(binding.kind(), OwnershipBindingKind::Shared);
    assert_eq!(
        unit.ownership().binding_kind(*item),
        Some(OwnershipBindingKind::Shared)
    );
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
    let returned = *one(&returns, "conditional Return inside this original for");
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
        panic!("the original Return has its actual operand");
    };
    let scope = unit.declaration_span(unit.declaration("updatingOperand", SymbolKind::Function));
    assert!(within(node.span(), scope));
    LoopIdentity {
        statement,
        body: UnitStatementId::new(source, *body),
        provider,
        item: *item,
        returned,
        operand: UnitExpressionId::new(source, *operand),
        scope,
    }
}

fn exact_drop(
    unit: &UnitSource,
    point: UnitDropPoint,
    target: UnitDropTarget,
    origin: Span,
) -> UnitDropFact {
    let drops = unit
        .ownership()
        .drops()
        .iter()
        .filter(|drop| drop.point() == point && drop.target() == target)
        .collect::<Vec<_>>();
    let drop = **one(&drops, "exact qualified owner drop");
    assert_eq!(drop.value_origin(), origin);
    drop
}

fn operand_call(
    unit: &UnitSource,
    identity: &LoopIdentity,
) -> (UnitExpressionId, Option<UnitExpressionId>) {
    let parsed = unit.parsed();
    let Expression::If { condition, .. } = parsed
        .ast()
        .expressions()
        .get(identity.operand.expression())
        .unwrap()
        .payload()
    else {
        panic!("the original conditional operand remains an If");
    };
    let condition = UnitExpressionId::new(source(unit), *condition);
    match parsed
        .ast()
        .expressions()
        .get(condition.expression())
        .unwrap()
        .payload()
    {
        Expression::Binary { left, .. } => {
            (UnitExpressionId::new(source(unit), *left), Some(condition))
        }
        Expression::Call { .. } => (condition, None),
        _ => panic!("the original operand uses a presence test or contains call"),
    }
}

fn assert_no_map(unit: &UnitSource) {
    let typed = unit.types();
    assert!(typed.map_constructions().is_empty());
    assert!(typed.map_puts().is_empty());
    assert!(typed.map_contains_calls().is_empty());
    assert!(typed.map_removes().is_empty());
    assert!(typed.map_gets().is_empty());
    assert!(unit.ownership().formed_owned_locals().is_empty());
    assert!(
        !unit
            .ownership()
            .drops()
            .iter()
            .any(|drop| { matches!(drop.target(), UnitDropTarget::Named(_)) })
    );
    assert!(
        !unit
            .parsed()
            .ast()
            .items()
            .iter()
            .any(|(_, node)| { matches!(node.payload(), Item::Variable { .. }) })
    );
    let resolution = one(
        unit.names().names().source_units(),
        "original no-map resolution",
    );
    assert_eq!(resolution.source_unit(), source(unit));
    assert!(
        !resolution
            .resolution()
            .symbols()
            .iter()
            .any(|symbol| symbol.name() == "cache")
    );
    let mut values = resolution
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| {
            !symbol.is_synthetic()
                && matches!(
                    symbol.kind(),
                    SymbolKind::ValueParameter | SymbolKind::ForBinding
                )
        })
        .collect::<Vec<_>>();
    values.sort_by_key(|symbol| symbol.span().start());
    assert_eq!(
        values
            .iter()
            .map(|symbol| (symbol.name(), symbol.kind()))
            .collect::<Vec<_>>(),
        [
            ("item", SymbolKind::ValueParameter),
            ("stop", SymbolKind::ValueParameter),
            ("item", SymbolKind::ForBinding),
        ]
    );
    for symbol in values {
        let symbol = qualified_symbol(unit, symbol.id());
        let ty = typed.symbol_type(symbol).unwrap();
        assert_eq!(
            typed.types().get(ty),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert_eq!(typed.copyability(ty), Copyability::Copyable);
    }
}

fn assert_map_drops(
    unit: &UnitSource,
    identity: &LoopIdentity,
    name: &str,
) -> Option<UnitSymbolId> {
    let typed = unit.types();
    let no_map = name.starts_with("no-map");
    let item = name.starts_with("item-");
    let nullable = name.starts_with("independent-nullable") || no_map;
    let call = if item {
        assert!(matches!(
            unit.parsed()
                .ast()
                .expressions()
                .get(identity.operand.expression())
                .unwrap()
                .payload(),
            Expression::Name
        ));
        assert_eq!(reference_symbol(unit, identity.operand), identity.item);
        None
    } else {
        let (call, binary) = operand_call(unit, identity);
        if nullable {
            assert_eq!(
                typed.call(call).unwrap().target(),
                UnitCallTarget::Declaration(
                    unit.declaration("nullableValue", SymbolKind::Function)
                )
            );
            assert!(typed.map_remove(call).is_none());
            assert!(typed.map_contains(call).is_none());
        }
        assert_eq!(binary.is_some(), nullable || name.starts_with("remove"));
        if let Some(binary) = binary {
            exact_drop(
                unit,
                UnitDropPoint::AfterBinaryOperands(binary),
                UnitDropTarget::Temporary(call),
                expression_span(unit, call),
            );
        }
        Some(call)
    };
    if no_map {
        assert_no_map(unit);
        return None;
    }
    let variables = unit
        .parsed()
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Variable {
                kind: VariableKind::Var,
                name: NameMarker::Present(name),
                initializer,
                ..
            } if within(node.span(), identity.scope) => Some((*name, *initializer)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let (declaration, initializer) = *one(&variables, "original outer Map variable");
    assert_eq!(unit.sources().slice(declaration).unwrap(), "cache");
    let resolution = one(
        unit.names().names().source_units(),
        "original Map resolution",
    );
    assert_eq!(resolution.source_unit(), source(unit));
    let symbols = resolution
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.span() == declaration && symbol.kind() == SymbolKind::Variable)
        .collect::<Vec<_>>();
    let symbol = one(&symbols, "original cache declaration symbol");
    assert!(!symbol.is_synthetic());
    let cache = qualified_symbol(unit, symbol.id());
    assert_eq!(symbol_span(unit, cache), declaration);
    let initializer = UnitExpressionId::new(source(unit), initializer);
    let construction = one(typed.map_constructions(), "actual Map construction");
    assert_eq!(typed.map_construction(initializer), Some(construction));
    assert_eq!(typed.symbol_type(cache), Some(construction.map_type()));
    assert_eq!(
        typed.expression_type(initializer),
        Some(construction.map_type())
    );
    assert_eq!(
        typed.copyability(construction.map_type()),
        Copyability::MoveOnly
    );
    assert!(unit.ownership().formed_owned_locals().contains(&cache));
    assert_eq!(typed.map_puts().len(), 2);
    let mut expected = if item {
        vec![UnitDropPoint::ControlTransfer(identity.returned)]
    } else if nullable {
        vec![
            UnitDropPoint::BranchExit {
                control: identity.operand,
                branch: 0,
            },
            UnitDropPoint::BranchExit {
                control: identity.operand,
                branch: 1,
            },
        ]
    } else {
        let call = call.unwrap();
        let receiver = if name.starts_with("remove") {
            assert!(typed.map_contains(call).is_none());
            typed.map_remove(call).unwrap().receiver()
        } else {
            assert!(typed.map_remove(call).is_none());
            typed.map_contains(call).unwrap().receiver()
        };
        assert_eq!(reference_symbol(unit, receiver), cache);
        vec![UnitDropPoint::CallReturn(call)]
    };
    if name.ends_with("live=true") {
        let queries = typed
            .map_contains_calls()
            .iter()
            .filter(|query| {
                expression_span(unit, query.expression()).start()
                    > expression_span(unit, identity.returned).end()
            })
            .collect::<Vec<_>>();
        let query = one(&queries, "original post-loop live query");
        assert_eq!(reference_symbol(unit, query.receiver()), cache);
        expected.push(UnitDropPoint::CallReturn(query.expression()));
    } else {
        expected.push(UnitDropPoint::LoopExit(identity.statement));
    }
    let named = unit
        .ownership()
        .drops()
        .iter()
        .filter(|drop| matches!(drop.target(), UnitDropTarget::Named(_)))
        .collect::<Vec<_>>();
    assert_eq!(
        named.iter().map(|drop| drop.point()).collect::<Vec<_>>(),
        expected
    );
    for drop in named {
        assert_eq!(drop.target(), UnitDropTarget::Named(cache));
        assert_eq!(drop.value_origin(), declaration);
        exact_drop(
            unit,
            drop.point(),
            UnitDropTarget::Named(cache),
            declaration,
        );
    }
    Some(cache)
}

fn assert_exits(
    unit: &UnitSource,
    identity: &LoopIdentity,
    name: &str,
    cache: Option<UnitSymbolId>,
) {
    let plan = one(unit.ownership().iterations(), "original provider plan");
    let expected = [
        (
            Exit::Return(identity.returned),
            UnitDropPoint::ControlTransfer(identity.returned),
        ),
        (
            Exit::Fallthrough,
            UnitDropPoint::AfterStatement(identity.body),
        ),
        (
            Exit::Exhaustion,
            UnitDropPoint::LoopExit(identity.statement),
        ),
    ];
    assert_eq!(plan.exits().len(), expected.len());
    for (exit, (kind, point)) in plan.exits().iter().zip(expected) {
        assert_eq!(exit.kind(), kind, "original source-ordered provider exits");
        assert_eq!(exit.point(), point);
        let mut actions = Vec::new();
        if kind != Exit::Exhaustion {
            actions.push(Action::EndBinding {
                statement: identity.statement,
                symbol: identity.item,
            });
            actions.push(Action::EndElement(identity.statement));
        }
        if kind != Exit::Fallthrough {
            actions.push(Action::FinishProvider(identity.statement));
            actions.push(Action::EndSource(identity.statement));
            actions.push(Action::Drop(exact_drop(
                unit,
                point,
                UnitDropTarget::Temporary(identity.provider),
                expression_span(unit, identity.provider),
            )));
        }
        if let Some(cache) = cache
            && ((kind == Exit::Return(identity.returned) && name.starts_with("item-"))
                || (kind == Exit::Exhaustion && !name.ends_with("live=true")))
        {
            actions.push(Action::Drop(exact_drop(
                unit,
                point,
                UnitDropTarget::Named(cache),
                symbol_span(unit, cache),
            )));
        }
        assert_eq!(
            exit.actions(),
            actions,
            "the complete cleanup sequence at {point:?}"
        );
        assert_eq!(
            unit.ownership().iteration_cleanup_at(point),
            Some(exit.actions())
        );
        let action_drops = actions
            .iter()
            .filter_map(|action| match action {
                Action::Drop(drop) => Some(*drop),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            unit.ownership()
                .drops()
                .iter()
                .copied()
                .filter(|drop| drop.point() == point)
                .collect::<Vec<_>>(),
            action_drops,
            "no hidden ordinary drop or unlisted owner at the provider exit"
        );
    }
    let provider_drops = unit
        .ownership()
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Temporary(identity.provider))
        .collect::<Vec<_>>();
    assert_eq!(
        provider_drops.len(),
        2,
        "Return and Exhaustion own separate cleanup paths"
    );
}

pub(super) fn check(text: &str, name: &str, supported: bool) {
    let unit = UnitSource::analyze("lowering.ko", text)
        .unwrap_or_else(|error| panic!("{name}: original Unit admission: {error:?}"));
    unit.assert_frontend_clean();
    source(&unit);
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
    let roots = roots(&unit, supported);
    let identity = iteration(&unit);
    let cache = assert_map_drops(&unit, &identity, name);
    assert_exits(&unit, &identity, name, cache);
    for declaration in roots {
        // Each root consumes these same admitted products; a negative root never supplies SSA.
        let result = unit.lower_entry(declaration);
        if !supported && declaration == unit.declaration("nullableValue", SymbolKind::Function) {
            let (program, entry) = result.expect("String? value-if now lowers");
            let functions = program
                .modules
                .iter()
                .flat_map(|module| &module.functions)
                .collect::<Vec<_>>();
            let function = one(&functions, "original nullableValue body without wrappers");
            assert_eq!(function.id, entry);
            assert_eq!(
                function.origin,
                Origin::Source(unit.declaration_span(declaration))
            );
            crate::ssa::verify::verify_program(&program).unwrap();
            crate::llvm::render_verified_program(&program).unwrap();
            continue;
        }
        if !supported {
            let error = match result {
                Ok(_) => panic!("{name}: root {declaration:?} unexpectedly lowered"),
                Err(error) => error,
            };
            assert_eq!(
                error.kind,
                LoweringErrorKind::UnsupportedNode,
                "{name}: lower stage"
            );
            let span = error.span.unwrap();
            assert_eq!(span.source_id(), unit.parsed().source_id());
            assert_eq!(
                unit.sources().source_name(span.source_id()).unwrap(),
                "lowering.ko"
            );
            let boundary = "nullableValue(item) != null";
            assert_eq!(text.matches(boundary).count(), 1);
            let start = text.find(boundary).unwrap();
            assert_eq!((span.start(), span.end()), (start, start + boundary.len()));
            assert_eq!(unit.sources().slice(span).unwrap(), boundary);
            continue;
        }
        let (program, entry) = result.unwrap_or_else(|error| panic!("{name}: lower: {error:?}"));
        let functions = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .collect::<Vec<_>>();
        let function = one(
            &functions,
            "original updatingOperand body, without wrappers",
        );
        assert_eq!(function.id, entry);
        assert_eq!(
            function.origin,
            Origin::Source(unit.declaration_span(declaration))
        );
        assert_eq!(
            super::super::operand_structure::assert_source(&program, unit.parsed()),
            usize::from(name.starts_with("remove") || name.starts_with("contains"))
        );
        if name.starts_with("item-") {
            item_ssa::check(&unit, &program, &identity, cache.unwrap());
        }
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
    }
}
