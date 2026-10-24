//! Read-only identities shared by the seven original loop/source Unit consumers.
//! No Legacy facts, fabricated phase products, or type-based alias equivalence.

#[path = "lower_frontend_map_loop_unit_contract.rs"]
mod map_contract;
#[path = "lower_frontend_map_source_unit_contract.rs"]
mod source_contract;

pub(super) use map_contract::{check_normal, check_original, check_resource};
pub(super) use source_contract::{SourceScenario, check_source};

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{
        Block, BlockId, Definition, Edge, EntityId, Function, FunctionId, Instruction, Operation,
        Origin, Program, TerminatorKind,
    },
};
use lang_frontend::{
    name_resolution::{SourceUnitKey, SymbolKind, UnitReferenceTarget, UnitSymbolId},
    ownership_checking::{UnitDropFact, UnitDropTarget, UnitIterationOwnershipPlan},
    parser::{Expression, FunctionBody, FunctionForm, Item, NameMarker, Statement},
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};

fn one<'a, T>(values: &'a [T], label: &str) -> &'a T {
    assert_eq!(values.len(), 1, "{label}");
    &values[0]
}

fn within(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn expression_span(unit: &UnitSource, expression: UnitExpressionId) -> Span {
    let source = one(unit.names().names().index().source_units(), "one source");
    assert_eq!(expression.source_unit(), source.id());
    unit.parsed()
        .ast()
        .expressions()
        .get(expression.expression())
        .unwrap()
        .span()
}

fn statement_span(unit: &UnitSource, statement: UnitStatementId) -> Span {
    let source = one(unit.names().names().index().source_units(), "one source");
    assert_eq!(statement.source_unit(), source.id());
    unit.parsed()
        .ast()
        .statements()
        .get(statement.statement())
        .unwrap()
        .span()
}

fn declaration_span(unit: &UnitSource, name: &str) -> Span {
    unit.declaration_span(unit.declaration(name, SymbolKind::Function))
}

fn function_at(program: &Program, span: Span) -> &Function {
    let matches = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == Origin::Source(span))
        .collect::<Vec<_>>();
    one::<&Function>(&matches, "one function at the original declaration")
}

fn lower(text: &str, functions: &[&str]) -> (UnitSource, Program, FunctionId) {
    let unit = UnitSource::analyze("lowering.ko", text)
        .unwrap_or_else(|error| panic!("{text}\n{error:?}"));
    unit.assert_frontend_clean();
    let source = one(
        unit.names().names().index().source_units(),
        "one original input",
    );
    assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(source.source_id(), unit.parsed().source_id());
    let declarations = unit.names().names().index().declarations();
    let actual = declarations
        .iter()
        .filter(|declaration| declaration.kind() == SymbolKind::Function)
        .map(|declaration| declaration.name())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, functions.iter().copied().collect::<BTreeSet<_>>());
    let (program, entry) = unit
        .lower_entry(unit.declaration("main", SymbolKind::Function))
        .unwrap_or_else(|error| panic!("{text}\n{error:?}"));
    assert_eq!(
        function_at(&program, declaration_span(&unit, "main")).id,
        entry
    );
    for name in functions {
        function_at(&program, declaration_span(&unit, name));
    }
    // Every non-entry original function is reached by an actual call in main.
    let main = function_at(&program, declaration_span(&unit, "main"));
    let callees = main
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } => Some(callee),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let expected = functions
        .iter()
        .filter(|name| **name != "main")
        .map(|name| function_at(&program, declaration_span(&unit, name)).id)
        .collect::<BTreeSet<_>>();
    assert_eq!(callees, expected);
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    (unit, program, entry)
}

fn body(unit: &UnitSource, span: Span) -> UnitStatementId {
    let bodies = unit
        .parsed()
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| {
            if node.span() != span {
                return None;
            }
            match node.payload() {
                Item::Function {
                    form:
                        FunctionForm::Explicit {
                            body: FunctionBody::Block(body),
                            ..
                        },
                    ..
                }
                | Item::Function {
                    form: FunctionForm::ImplicitUnitBlock(body),
                    ..
                } => Some(*body),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    let source = one(unit.names().names().index().source_units(), "one source");
    UnitStatementId::new(source.id(), *one(&bodies, "original function block"))
}

fn iteration(unit: &UnitSource, function: Span) -> &UnitIterationOwnershipPlan {
    let plans = unit
        .ownership()
        .iterations()
        .iter()
        .filter(|plan| {
            within(
                statement_span(unit, plan.descriptor().statement()),
                function,
            )
        })
        .collect::<Vec<_>>();
    let plan = *one(&plans, "one actual provider per original function");
    let descriptor = plan.descriptor();
    assert_eq!(
        unit.types().sequential_iteration(descriptor.statement()),
        Some(descriptor)
    );
    let node = unit
        .parsed()
        .ast()
        .statements()
        .get(descriptor.statement().statement())
        .unwrap();
    let Statement::For { source, .. } = node.payload() else {
        panic!("published provider belongs to the original for")
    };
    assert_eq!(descriptor.source().expression(), *source);
    assert_eq!(
        descriptor.source().source_unit(),
        descriptor.statement().source_unit()
    );
    plan
}

fn symbol_span(unit: &UnitSource, root: UnitSymbolId, function: Span) -> Span {
    let resolutions = unit
        .names()
        .names()
        .source_units()
        .iter()
        .filter(|resolution| resolution.source_unit() == root.source_unit())
        .collect::<Vec<_>>();
    let symbols = one(&resolutions, "same source names")
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.id() == root.symbol())
        .collect::<Vec<_>>();
    let symbol = one(&symbols, "real qualified symbol");
    assert!(!symbol.is_synthetic());
    assert!(within(symbol.span(), function));
    symbol.span()
}

fn reference_root(unit: &UnitSource, expression: UnitExpressionId) -> UnitSymbolId {
    let span = expression_span(unit, expression);
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
            reference.source_unit() == expression.source_unit() && reference.span() == span
        })
        .collect::<Vec<_>>();
    let UnitReferenceTarget::Symbol(root) = one(&references, "actual named reference").target()
    else {
        panic!("named owner must resolve to the original source symbol")
    };
    *root
}

fn named_drop(unit: &UnitSource, root: UnitSymbolId, function: Span) -> UnitDropFact {
    let drops = unit
        .ownership()
        .drops()
        .iter()
        .filter(|fact| fact.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let fact = **one(&drops, "one exact Named owner cleanup");
    assert_eq!(fact.value_origin(), symbol_span(unit, root, function));
    fact
}

fn initializer(unit: &UnitSource, name: Span) -> UnitExpressionId {
    let expressions = unit
        .parsed()
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Variable {
                name: NameMarker::Present(span),
                initializer,
                ..
            } if *span == name => Some(*initializer),
            _ => None,
        })
        .collect::<Vec<_>>();
    let source = one(unit.names().names().index().source_units(), "one source");
    UnitExpressionId::new(
        source.id(),
        *one(&expressions, "original named initializer"),
    )
}

fn edges(block: &Block) -> Vec<&Edge> {
    match &block.terminator.as_ref().unwrap().kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        _ => Vec::new(),
    }
}

// Follow definitions backwards, never union an undirected alias family. Every
// incoming column is visited; every component must be grounded at a requested
// exact entity. A foreign leaf or a rootless side cycle makes the proof fail.
fn transport_roots(
    function: &Function,
    current: EntityId,
    stops: &BTreeSet<EntityId>,
) -> Option<BTreeSet<EntityId>> {
    let mut pending = vec![current];
    let mut dependencies = BTreeMap::<EntityId, Vec<EntityId>>::new();
    let mut roots = BTreeSet::new();
    while let Some(entity) = pending.pop() {
        if dependencies.contains_key(&entity) {
            continue;
        }
        if stops.contains(&entity) {
            roots.insert(entity);
            dependencies.insert(entity, Vec::new());
            continue;
        }
        let data = function.entity(entity)?;
        let inputs = match data.definition {
            Definition::BlockParameter { block, index } => {
                if function.block(block)?.parameters.get(index) != Some(&entity) {
                    return None;
                }
                let incoming = function
                    .blocks
                    .iter()
                    .flat_map(edges)
                    .filter(|edge| edge.target == block)
                    .collect::<Vec<_>>();
                if incoming.is_empty() {
                    return None;
                }
                incoming
                    .iter()
                    .map(|edge| edge.arguments.get(index).copied())
                    .collect::<Option<Vec<_>>>()?
            }
            Definition::InstructionResult { instruction, index } => {
                let instruction = function.instruction(instruction)?;
                if instruction.results.get(index) != Some(&entity) {
                    return None;
                }
                match instruction.operation {
                    Operation::RootPlace { owner } if index == 0 => vec![EntityId::Value(owner)],
                    Operation::BorrowBegin { place, .. } if index == 0 => {
                        vec![EntityId::Place(place)]
                    }
                    Operation::SharedReborrow { source } if index == 0 => {
                        vec![EntityId::Loan(source)]
                    }
                    _ => return None,
                }
            }
        };
        if inputs.is_empty() {
            return None;
        }
        pending.extend(inputs.iter().copied());
        dependencies.insert(entity, inputs);
    }
    let mut grounded = roots.clone();
    loop {
        let before = grounded.len();
        for (entity, inputs) in &dependencies {
            if inputs.iter().any(|input| grounded.contains(input)) {
                grounded.insert(*entity);
            }
        }
        if grounded.len() == before {
            break;
        }
    }
    dependencies
        .keys()
        .all(|entity| grounded.contains(entity))
        .then_some(roots)
}

fn transported(function: &Function, current: EntityId, expected: EntityId) {
    let roots = BTreeSet::from([expected]);
    assert_eq!(transport_roots(function, current, &roots), Some(roots));
}

fn instruction_at(
    function: &Function,
    span: Span,
    accepts: fn(&Operation) -> bool,
) -> &Instruction {
    let instructions = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(span) && accepts(&instruction.operation)
        })
        .collect::<Vec<_>>();
    one::<&Instruction>(&instructions, "one exact operation at its original source")
}

fn result(instruction: &Instruction, index: usize) -> EntityId {
    *instruction
        .results
        .get(index)
        .expect("actual operation result")
}

struct LoopEdges<'a> {
    preheader: &'a Block,
    header: &'a Block,
    entry: &'a Edge,
    body: &'a Edge,
    exhaustion: &'a Edge,
    backedges: Vec<(&'a Block, &'a Edge)>,
}

fn loop_edges(function: &Function, span: Span, backedges: usize) -> LoopEdges<'_> {
    let snapshots = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(span)
                && matches!(instruction.operation, Operation::ContainerLength { .. })
        })
        .collect::<Vec<_>>();
    let snapshot = one(&snapshots, "one fixed provider length snapshot");
    let preheader = function.block(snapshot.block).unwrap();
    let TerminatorKind::Branch(entry) = &preheader.terminator.as_ref().unwrap().kind else {
        panic!("provider entry edge")
    };
    let header = function.block(entry.target).unwrap();
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("provider header guard")
    };
    let incoming = function
        .blocks
        .iter()
        .flat_map(|block| {
            edges(block)
                .into_iter()
                .filter(move |edge| edge.target == header.id)
                .map(move |edge| (block, edge))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        incoming.len(),
        backedges + 1,
        "entry and every actual re-entry"
    );
    let actual = incoming
        .into_iter()
        .filter(|(block, _)| block.id != preheader.id)
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), backedges);
    LoopEdges {
        preheader,
        header,
        entry,
        body: when_true,
        exhaustion: when_false,
        backedges: actual,
    }
}

fn column(function: &Function, edge: &Edge, original: EntityId) -> usize {
    let columns = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, entity)| {
            (std::mem::discriminant(entity) == std::mem::discriminant(&original)
                && transport_roots(function, *entity, &BTreeSet::from([original]))
                    == Some(BTreeSet::from([original])))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    *one(&columns, "one exact transported identity column")
}

fn position(function: &Function, instruction: &Instruction) -> usize {
    function
        .block(instruction.block)
        .unwrap()
        .instructions
        .iter()
        .position(|id| *id == instruction.id)
        .unwrap()
}

fn reachable(function: &Function, start: BlockId, target: BlockId) -> bool {
    let mut pending = vec![start];
    let mut seen = BTreeSet::new();
    while let Some(block) = pending.pop() {
        if block == target {
            return true;
        }
        if seen.insert(block) {
            pending.extend(
                edges(function.block(block).unwrap())
                    .iter()
                    .map(|edge| edge.target),
            );
        }
    }
    false
}

fn precedes(function: &Function, first: &Instruction, second: &Instruction) {
    if first.block == second.block {
        assert!(position(function, first) < position(function, second));
    } else {
        assert!(reachable(function, first.block, second.block));
        assert!(!reachable(function, second.block, first.block));
    }
}
