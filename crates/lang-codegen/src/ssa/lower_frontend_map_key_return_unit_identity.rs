//! Same-analysis Unit receiver/return facts and directed SSA owner identity.
//! Unit has no public single-file ControlTransfer loan-end table. Its actual
//! receiver contract and Named drop are checked instead; owned Map SSA operands
//! are Values, so no synthetic BorrowEnd is required or accepted here.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{
        Block, BlockId, Definition, Edge, EntityId, Function, Instruction, Operation, Origin,
        Program, TerminatorKind, ValueId,
    },
};
use lang_frontend::{
    name_resolution::{SourceUnitKey, SymbolKind, UnitReferenceTarget},
    ownership_checking::{
        UnitCallArgumentOwnershipKind, UnitDropPoint, UnitDropTarget, UnitReceiverOwnershipKind,
        UnitReceiverOwnershipTarget,
    },
    parser::{Expression, Item, NameMarker, VariableKind},
    source::Span,
    type_checking::{ExpressionCategory, UnitCallReceiverOrigin, UnitCallTarget, UnitExpressionId},
};

struct Anchors {
    inspect: Span,
    construction: Span,
    root: Span,
    call: Span,
    key: Span,
    suffix: Span,
    suffix_declaration: Span,
    returns: Vec<Span>,
}

fn one<'a, T>(values: &'a [T], label: &str) -> &'a T {
    assert_eq!(values.len(), 1, "{label}");
    &values[0]
}

fn within(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && inner.start() >= outer.start()
        && inner.end() <= outer.end()
}

fn frontend(unit: &UnitSource, operation: &str, key_text: &str, count: usize) -> Anchors {
    let parsed = unit.parsed();
    let names = unit.names().names();
    let source = one(names.index().source_units(), "one original MemoryDocument");
    assert_eq!(source.source_id(), parsed.source_id());
    assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
    let inspect = unit.declaration_span(unit.declaration("inspect", SymbolKind::Function));
    let suffix_declaration =
        unit.declaration_span(unit.declaration("suffix", SymbolKind::Function));
    let typed = unit.types();
    let (call, receiver, key) = match operation {
        "get" => {
            let descriptor = one(typed.map_gets(), "one real get descriptor");
            (
                descriptor.expression(),
                descriptor.receiver(),
                descriptor.key(),
            )
        }
        "contains" => {
            let descriptor = one(typed.map_contains_calls(), "one real contains descriptor");
            (
                descriptor.expression(),
                descriptor.receiver(),
                descriptor.key(),
            )
        }
        "remove" => {
            let descriptor = one(typed.map_removes(), "one real remove descriptor");
            (
                descriptor.expression(),
                descriptor.receiver(),
                descriptor.key(),
            )
        }
        _ => panic!("unknown original Map operation {operation}"),
    };
    assert_eq!(
        typed.map_gets().len() + typed.map_contains_calls().len() + typed.map_removes().len(),
        1
    );
    for expression in [call, receiver, key] {
        assert_eq!(expression.source_unit(), source.id());
        let span = parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .unwrap()
            .span();
        assert!(within(span, inspect));
    }
    let call_node = parsed.ast().expressions().get(call.expression()).unwrap();
    let Expression::Call {
        callee, arguments, ..
    } = call_node.payload()
    else {
        panic!("descriptor must name the original member Call")
    };
    assert_eq!(
        one(arguments, "one original key argument").value,
        key.expression()
    );
    let member = parsed.ast().expressions().get(*callee).unwrap();
    let Expression::Member {
        receiver: actual,
        name_span,
        safe,
        ..
    } = member.payload()
    else {
        panic!("original Map member receiver")
    };
    assert_eq!(*actual, receiver.expression());
    assert!(!safe);
    assert_eq!(unit.sources().slice(*name_span).unwrap(), operation);
    let receiver_node = parsed
        .ast()
        .expressions()
        .get(receiver.expression())
        .unwrap();
    assert!(matches!(receiver_node.payload(), Expression::Name));
    assert_eq!(unit.sources().slice(receiver_node.span()).unwrap(), "m");
    let key_span = parsed
        .ast()
        .expressions()
        .get(key.expression())
        .unwrap()
        .span();
    assert_eq!(unit.sources().slice(key_span).unwrap(), key_text);
    let references = names
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == source.id() && reference.span() == receiver_node.span()
        })
        .collect::<Vec<_>>();
    let UnitReferenceTarget::Symbol(root) = one(&references, "exact receiver reference").target()
    else {
        panic!("receiver must resolve to its original local root")
    };
    assert_eq!(root.source_unit(), source.id());
    let resolutions = names
        .source_units()
        .iter()
        .filter(|resolution| resolution.source_unit() == source.id())
        .collect::<Vec<_>>();
    let symbols = one(&resolutions, "same source resolution")
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.id() == root.symbol())
        .collect::<Vec<_>>();
    let symbol = one(&symbols, "unique original m symbol");
    assert_eq!(symbol.name(), "m");
    assert!(!symbol.is_synthetic());
    assert!(within(symbol.span(), inspect));
    let initializers = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Variable {
                kind: VariableKind::Var,
                name: NameMarker::Present(span),
                initializer,
                ..
            } if *span == symbol.span() && within(node.span(), inspect) => Some(*initializer),
            _ => None,
        })
        .collect::<Vec<_>>();
    let construction = one(typed.map_constructions(), "original local Map construction");
    assert_eq!(
        construction.expression(),
        UnitExpressionId::new(source.id(), *one(&initializers, "m's actual initializer"))
    );
    assert_eq!(typed.symbol_type(*root), Some(construction.map_type()));
    assert_eq!(
        typed.expression_type(receiver),
        Some(construction.map_type())
    );

    let owned = unit.ownership();
    let contracts = owned
        .call_receiver_contracts()
        .iter()
        .filter(|contract| contract.call() == call)
        .collect::<Vec<_>>();
    let contract = **one(&contracts, "unique real receiver contract");
    assert_eq!(owned.call_receiver_contract(call), Some(contract));
    assert_eq!(
        contract.source(),
        UnitCallReceiverOrigin::Expression(receiver)
    );
    assert_eq!(contract.target(), UnitCallTarget::FunctionValue);
    assert_eq!(contract.receiver_type(), construction.map_type());
    assert_eq!(contract.category(), ExpressionCategory::Place);
    assert_eq!(contract.receiver_span(), receiver_node.span());
    assert_eq!(contract.call_span(), call_node.span());
    assert_eq!(
        contract.kind(),
        if operation == "remove" {
            UnitCallArgumentOwnershipKind::ExclusiveLoan
        } else {
            UnitCallArgumentOwnershipKind::SharedLoan
        }
    );
    let facts = owned
        .receiver_facts()
        .iter()
        .filter(|fact| fact.call() == call)
        .collect::<Vec<_>>();
    let fact = *one(&facts, "unique real receiver ownership effect");
    assert_eq!(owned.receiver_fact(call), Some(fact));
    assert_eq!(fact.source(), contract.source());
    assert_eq!(fact.receiver_type(), contract.receiver_type());
    assert_eq!(fact.begin_span(), receiver_node.span());
    assert_eq!(fact.end_span(), call_node.span());
    let UnitReceiverOwnershipTarget::Place(place) = fact.target() else {
        panic!("named Map receiver must retain its actual root place")
    };
    assert_eq!(place.root(), *root);
    assert!(place.is_root());
    assert_eq!(
        fact.kind(),
        if operation == "remove" {
            UnitReceiverOwnershipKind::ExclusiveLoan
        } else {
            UnitReceiverOwnershipKind::SharedLoan
        }
    );
    assert_eq!(fact.is_receiver_reservation(), operation == "remove");
    assert_eq!(
        fact.activation_point(),
        (operation == "remove" && count == 1).then_some(call)
    );

    let returns = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            if matches!(node.payload(), Expression::Return { value: None, .. })
                && within(node.span(), inspect)
            {
                assert!(within(node.span(), key_span));
                Some((UnitExpressionId::new(source.id(), id), node.span()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        returns.len(),
        if key_text == "if (flag) { return } else { return }" {
            2
        } else {
            1
        }
    );
    let transfers = owned
        .drops()
        .iter()
        .filter(|drop| matches!(drop.point(), UnitDropPoint::ControlTransfer(_)))
        .collect::<Vec<_>>();
    assert_eq!(
        transfers.len(),
        returns.len(),
        "no omitted or extra transfer cleanup"
    );
    for (id, span) in &returns {
        assert_eq!(span.source_id(), parsed.source_id());
        assert_eq!(unit.sources().slice(*span).unwrap(), "return");
        let drops = transfers
            .iter()
            .filter(|drop| drop.point() == UnitDropPoint::ControlTransfer(*id))
            .collect::<Vec<_>>();
        let drop = one(&drops, "each actual Return has one real Unit drop");
        assert_eq!(drop.target(), UnitDropTarget::Named(*root));
        assert_eq!(drop.value_origin(), symbol.span());
    }
    let suffixes = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            (matches!(node.payload(), Expression::Call { .. })
                && within(node.span(), inspect)
                && unit.sources().slice(node.span()) == Ok("suffix()"))
            .then_some(node.span())
        })
        .collect::<Vec<_>>();
    Anchors {
        inspect,
        construction: parsed
            .ast()
            .expressions()
            .get(construction.expression().expression())
            .unwrap()
            .span(),
        root: symbol.span(),
        call: call_node.span(),
        key: key_span,
        suffix: *one(&suffixes, "actual suffix Call in inspect"),
        suffix_declaration,
        returns: returns.into_iter().map(|(_, span)| span).collect(),
    }
}

fn function_at(program: &Program, span: Span) -> &Function {
    let functions = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == Origin::Source(span))
        .collect::<Vec<_>>();
    one(&functions, "unique function at original declaration Origin")
}

fn edges(block: &Block) -> Vec<&Edge> {
    match &block.terminator.as_ref().unwrap().kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::Return { .. } => Vec::new(),
        _ => panic!("original key-return source has only ordinary If/Return CFG"),
    }
}

// Reverse every real incoming column. Every leaf must be the exact requested
// birth, and every visited SCC must reach that birth. No type-only, undirected,
// first-predecessor, mixed-root, or rootless-cycle equivalence is accepted.
fn from_birth(function: &Function, current: ValueId, birth: ValueId) {
    let mut pending = vec![current];
    let mut dependencies = BTreeMap::<ValueId, Vec<ValueId>>::new();
    while let Some(value) = pending.pop() {
        if dependencies.contains_key(&value) {
            continue;
        }
        let data = function.entity(EntityId::Value(value)).unwrap();
        assert_eq!(data.ty, function.entity(EntityId::Value(birth)).unwrap().ty);
        if value == birth {
            dependencies.insert(value, Vec::new());
            continue;
        }
        let Definition::BlockParameter { block, index } = data.definition else {
            panic!("Map owner transport reached a foreign instruction root: {value:?}")
        };
        assert_eq!(
            function.block(block).unwrap().parameters[index],
            EntityId::Value(value)
        );
        let incoming = function
            .blocks
            .iter()
            .flat_map(edges)
            .filter(|edge| edge.target == block)
            .collect::<Vec<_>>();
        assert!(
            !incoming.is_empty(),
            "owner transport must have real incoming edges"
        );
        let inputs = incoming
            .iter()
            .map(|edge| {
                let Some(EntityId::Value(value)) = edge.arguments.get(index) else {
                    panic!("every incoming owner column must carry a Value")
                };
                *value
            })
            .collect::<Vec<_>>();
        pending.extend(inputs.iter().copied());
        dependencies.insert(value, inputs);
    }
    let mut grounded = BTreeSet::from([birth]);
    loop {
        let before = grounded.len();
        for (value, inputs) in &dependencies {
            if inputs.iter().any(|input| grounded.contains(input)) {
                grounded.insert(*value);
            }
        }
        if before == grounded.len() {
            break;
        }
    }
    assert!(
        dependencies.keys().all(|value| grounded.contains(value)),
        "every incoming transport component must be grounded at the original Map birth"
    );
}

fn operation_owner(instruction: &Instruction, operation: &str) -> Option<ValueId> {
    match (&instruction.operation, operation) {
        (
            Operation::MapGet {
                owner: EntityId::Value(owner),
                ..
            },
            "get",
        )
        | (
            Operation::MapContains {
                owner: EntityId::Value(owner),
                ..
            },
            "contains",
        )
        | (Operation::MapRemove { owner, .. }, "remove") => Some(*owner),
        _ => None,
    }
}

#[derive(Clone, Default)]
struct Path {
    blocks: BTreeSet<BlockId>,
    operations: usize,
    suffixes: usize,
    sizes: usize,
    drops: usize,
}

fn check_paths(function: &Function, anchors: &Anchors, count: usize) {
    let mut pending = vec![(function.entry_block().unwrap(), Path::default())];
    let mut exits = 0;
    let mut live = 0;
    while let Some((block, mut path)) = pending.pop() {
        assert!(
            path.blocks.insert(block),
            "the original key-return CFG is acyclic"
        );
        let block = function.block(block).unwrap();
        for id in &block.instructions {
            let instruction = function.instruction(*id).unwrap();
            match instruction.operation {
                Operation::MapGet { .. }
                | Operation::MapContains { .. }
                | Operation::MapRemove { .. } => {
                    assert_eq!(path.suffixes, 0, "operation must precede suffix");
                    path.operations += 1;
                }
                Operation::DirectCall { .. } => {
                    assert_eq!(instruction.origin, Origin::Source(anchors.suffix));
                    assert_eq!(
                        path.operations, 1,
                        "suffix only follows completed Map operation"
                    );
                    path.suffixes += 1;
                }
                Operation::MapSize { .. } => {
                    assert_eq!(path.suffixes, 1, "original size follows suffix");
                    path.sizes += 1;
                }
                Operation::Drop { .. } => path.drops += 1,
                _ => {}
            }
        }
        let terminator = block.terminator.as_ref().unwrap();
        if let TerminatorKind::Return { values } = &terminator.kind {
            assert!(values.is_empty());
            assert_eq!(path.drops, 1, "each exit consumes its Map exactly once");
            if anchors
                .returns
                .iter()
                .any(|span| terminator.origin == Origin::Source(*span))
            {
                assert_eq!(
                    (path.operations, path.suffixes, path.sizes),
                    (0, 0, 0),
                    "return in key cannot execute the operation or its suffix"
                );
                exits += 1;
            } else {
                assert_eq!((path.operations, path.suffixes, path.sizes), (1, 1, 1));
                live += 1;
            }
        } else {
            for edge in edges(block) {
                pending.push((edge.target, path.clone()));
            }
        }
    }
    assert_eq!(exits, anchors.returns.len());
    assert_eq!(
        live, count,
        "only the original live branch reaches the suffix"
    );
}

fn ssa(program: &Program, anchors: &Anchors, operation: &str, count: usize) {
    let function = function_at(program, anchors.inspect);
    let suffix = function_at(program, anchors.suffix_declaration);
    let constructions = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::MapConstruct { .. }))
        .collect::<Vec<_>>();
    let construction = one(&constructions, "exactly one original MapConstruct");
    assert_eq!(construction.origin, Origin::Source(anchors.construction));
    let EntityId::Value(birth) = *one(&construction.results, "MapConstruct owner result") else {
        panic!("original MapConstruct must produce its owned Value")
    };
    assert!(
        function.loans.is_empty(),
        "owned Map primitive uses Values; Return must have no active or fabricated loan"
    );
    assert!(!function.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::BorrowBegin { .. } | Operation::BorrowEnd { .. }
    )));
    let operations = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::MapGet { .. }
                    | Operation::MapContains { .. }
                    | Operation::MapRemove { .. }
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        operations.len(),
        count,
        "original exact Map operation count"
    );
    let mut live_birth = birth;
    for instruction in &operations {
        assert_eq!(instruction.origin, Origin::Source(anchors.call));
        let owner = operation_owner(instruction, operation)
            .unwrap_or_else(|| panic!("exact original Map operation and Value receiver"));
        from_birth(function, owner, birth);
        if operation == "remove" {
            let Some(EntityId::Value(updated)) = instruction.results.first() else {
                panic!("MapRemove preserves an explicit updated owner")
            };
            live_birth = *updated;
        }
    }
    let calls = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), count, "full key return cannot lower suffix");
    for instruction in calls {
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments,
        } = &instruction.operation
        else {
            panic!("original suffix is a direct top-level call")
        };
        assert_eq!(*callee, suffix.id);
        assert!(arguments.is_empty());
        assert_eq!(instruction.origin, Origin::Source(anchors.suffix));
    }
    let mut return_blocks = BTreeSet::new();
    for span in &anchors.returns {
        assert!(within(*span, anchors.key));
        let blocks = function
            .blocks
            .iter()
            .filter(|block| {
                let term = block.terminator.as_ref().unwrap();
                term.origin == Origin::Source(*span)
                    && matches!(&term.kind, TerminatorKind::Return { values } if values.is_empty())
            })
            .collect::<Vec<_>>();
        let block = one(&blocks, "one SSA exit for each actual source Return");
        assert!(return_blocks.insert(block.id));
        let drops = block
            .instructions
            .iter()
            .map(|id| function.instruction(*id).unwrap())
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .collect::<Vec<_>>();
        let drop = one(&drops, "Return consumes the Named Map exactly once");
        assert_eq!(drop.origin, Origin::Source(anchors.root));
        let Operation::Drop { owner } = drop.operation else {
            unreachable!()
        };
        from_birth(function, owner, birth);
    }
    for instruction in &function.instructions {
        if let Operation::Drop { owner } = instruction.operation {
            assert_eq!(instruction.origin, Origin::Source(anchors.root));
            from_birth(
                function,
                owner,
                if return_blocks.contains(&instruction.block) {
                    birth
                } else {
                    live_birth
                },
            );
        }
    }
    check_paths(function, anchors, count);
}

pub(super) fn check(source: &str, operation: &str, key: &str, count: usize) {
    let unit = UnitSource::analyze("lowering.ko", source)
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    unit.assert_frontend_clean();
    let anchors = frontend(&unit, operation, key, count);
    let entry = unit.declaration("entry", SymbolKind::Function);
    let entry_span = unit.declaration_span(entry);
    let (program, entry) = unit
        .lower_entry(entry)
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    assert_eq!(function_at(&program, entry_span).id, entry);
    ssa(&program, &anchors, operation, count);
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
}
