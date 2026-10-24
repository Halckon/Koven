//! Read-only qualification of this fixed String fixture, on the harness's actual analysis.

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        SourceUnitId, SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    },
    parser::{Expression, ParsedFile},
    type_checking::{
        BuiltinType, CompilationUnitTypes, ExpressionCategory, IntrinsicTypeConstructor,
        UnitCallReceiverOrigin, UnitCallTarget, UnitExpressionId, UnitTypeKind,
    },
};

use super::super::map_consume_fixture_tests::{Failure, Outcome, gate};

fn failure(stage: &str, detail: impl std::fmt::Debug) -> Failure {
    Failure {
        stage: stage.to_owned(),
        detail: format!("{detail:#?}"),
    }
}

fn receiver_chain(
    parsed: &ParsedFile,
    call: ExpressionId,
    receiver: ExpressionId,
    stages: &mut Vec<String>,
) -> Outcome<[ExpressionId; 3]> {
    let node = parsed
        .ast()
        .expressions()
        .get(call)
        .map_err(|error| failure("real consume call AST", error))?;
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = node.payload()
    else {
        return Err(failure("real consume call AST", node));
    };
    let member = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|error| failure("real consume member AST", error))?;
    gate(
        "exact consume Call and original grouped receiver",
        arguments.is_empty()
            && type_arguments.is_empty()
            && matches!(member.payload(), Expression::Member { receiver: actual, safe: false, .. }
                if *actual == receiver),
        (node, member),
        stages,
    )?;
    let mut expression = receiver;
    let mut chain = Vec::new();
    loop {
        let node = parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|error| failure("real receiver chain AST", error))?;
        gate(
            "same-file receiver AST",
            node.span().source_id() == parsed.source_id(),
            node,
            stages,
        )?;
        chain.push(expression);
        match node.payload() {
            Expression::Group { expression: inner } => expression = *inner,
            Expression::Name => break,
            _ => return Err(failure("receiver must end at a Name", node)),
        }
    }
    match chain.as_slice() {
        [outer, inner, name] => Ok([*outer, *inner, *name]),
        _ => Err(failure("exact two Groups then Name", chain)),
    }
}

// The original single source now has real Unit facts. These are the same exact
// root/storage/KV, three Name/Group proofs, null comparison, and Map descriptor
// oracles as before, with source qualification and the receiver Move fact added.
pub(super) fn inspect_single(
    source: SourceUnitId,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    stages: &mut Vec<String>,
) -> Outcome<()> {
    let [consume] = typed.map_consumes() else {
        return Err(failure(
            "one real qualified nullable String consume",
            typed.map_consumes(),
        ));
    };
    let Some((root, storage)) = consume.source() else {
        return Err(failure(
            "real qualified source root and nullable storage",
            consume,
        ));
    };
    gate(
        "consume call/receiver/root belong to the actual fixture source",
        consume.expression().source_unit() == source
            && consume.receiver().source_unit() == source
            && root.source_unit() == source,
        (source, consume),
        stages,
    )?;
    let chain = receiver_chain(
        parsed,
        consume.expression().expression(),
        consume.receiver().expression(),
        stages,
    )?;
    let roots: Vec<_> = names
        .names()
        .source_units()
        .iter()
        .filter(|unit| unit.source_unit() == source)
        .flat_map(|unit| unit.resolution().symbols())
        .filter(|symbol| symbol.id() == root.symbol())
        .collect();
    let leaf = parsed
        .ast()
        .expressions()
        .get(chain[2])
        .map_err(|error| failure("qualified receiver Name AST", error))?;
    let references: Vec<_> = names
        .names()
        .references()
        .iter()
        .filter(|reference| reference.source_unit() == source && reference.span() == leaf.span())
        .collect();
    let string = typed.types().builtin(BuiltinType::String);
    let pair = [consume.key_type(), consume.value_type()];
    gate(
        "qualified nullable String source/root/storage/read/result/KV identity",
        matches!(roots.as_slice(), [symbol]
            if symbol.name() == "source" && symbol.kind() == SymbolKind::Variable
                && !symbol.is_synthetic() && symbol.span().source_id() == parsed.source_id())
            && matches!(references.as_slice(), [reference]
                if matches!(reference.target(), UnitReferenceTarget::Symbol(actual) if *actual == root))
            && typed.symbol_type(root) == Some(storage)
            && storage != consume.receiver_type()
            && typed.types().get(storage) == Some(&UnitTypeKind::Nullable(consume.receiver_type()))
            && string.is_some_and(|string| pair == [string, string])
            && matches!(typed.types().get(consume.receiver_type()),
                Some(UnitTypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::MutableMap, arguments })
                    if arguments.as_slice() == pair)
            && matches!(typed.types().get(consume.result_type()),
                Some(UnitTypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::Map, arguments })
                    if arguments.as_slice() == pair)
            && typed.expression_type(consume.expression()) == Some(consume.result_type()),
        (consume, &roots, &references, storage, string),
        stages,
    )?;
    for expression in chain {
        let expression = UnitExpressionId::new(source, expression);
        let proof = typed.non_null_use(expression);
        gate(
            "original qualified Name/Group non-null use and Place identity",
            proof.is_some_and(|proof| {
                proof.expression() == expression
                    && proof.symbol() == root
                    && proof.declared_type() == storage
                    && proof.narrowed_type() == consume.receiver_type()
            }) && typed.expression_type(expression) == Some(consume.receiver_type())
                && typed.expression_category(expression) == Some(ExpressionCategory::Place),
            (expression, proof),
            stages,
        )?;
    }
    gate(
        "real qualified non-null true edge for the same source",
        matches!(typed.null_comparisons(), [comparison]
            if comparison.expression().source_unit() == source && comparison.symbol() == root
                && comparison.nullable_type() == storage && comparison.non_null_when_true()),
        typed.null_comparisons(),
        stages,
    )?;
    let actual = owned.receiver_fact(consume.expression());
    gate(
        "real qualified consume receiver Move fact",
        actual.is_some_and(|fact| {
            fact.call() == consume.expression()
                && fact.source() == UnitCallReceiverOrigin::Expression(consume.receiver())
                && fact.receiver_type() == consume.receiver_type()
                && fact.kind() == UnitReceiverOwnershipKind::Move
                && matches!(fact.target(), UnitReceiverOwnershipTarget::Place(place)
                if place.is_root() && place.root() == root)
        }),
        (consume, actual),
        stages,
    )?;
    gate(
        "fixed qualified overwrite/size/two String borrow-read descriptors",
        typed.map_constructions().len() == 1
            && typed.map_puts().len() == 2
            && typed.map_removes().is_empty()
            && typed.map_sizes().len() == 1
            && typed.map_with_values().len() == 2
            && typed
                .map_constructions()
                .iter()
                .all(|construction| construction.expression().source_unit() == source)
            && typed
                .map_puts()
                .iter()
                .all(|put| put.expression().source_unit() == source)
            && typed.map_sizes().iter().all(|size| {
                size.expression().source_unit() == source
                    && size.receiver().source_unit() == source
                    && size.map_type() == consume.result_type()
            })
            && typed.map_with_values().iter().all(|read| {
                read.expression().source_unit() == source
                    && read.receiver().source_unit() == source
                    && [read.key_type(), read.value_type()] == pair
                    && typed.expression_type(read.receiver()) == Some(consume.result_type())
            }),
        (
            typed.map_constructions(),
            typed.map_puts(),
            typed.map_sizes(),
            typed.map_with_values(),
        ),
        stages,
    )
}

pub(super) fn inspect_unit(
    source: SourceUnitId,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    stages: &mut Vec<String>,
) -> Outcome<()> {
    let index = names.names().index();
    gate(
        "actual p/provider.ko source for the cross-file consume",
        index.source_units().iter().any(|unit| {
            unit.id() == source
                && unit.source_id() == parsed.source_id()
                && unit
                    .key()
                    .logical_path()
                    .is_some_and(|path| path.as_str() == "p/provider.ko")
        }),
        index.source_units(),
        stages,
    )?;
    let mains: Vec<_> = index
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "main"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == source
                && declaration.name_span().source_id() == parsed.source_id()
        })
        .collect();
    let consumers: Vec<_> = index
        .source_units()
        .iter()
        .filter(|unit| {
            unit.key()
                .logical_path()
                .is_some_and(|path| path.as_str() == "q/consumer.ko")
        })
        .collect();
    let ([main], [consumer]) = (mains.as_slice(), consumers.as_slice()) else {
        return Err(failure(
            "unique real p.main and q consumer",
            (&mains, &consumers),
        ));
    };
    let calls: Vec<_> = typed
        .calls()
        .iter()
        .filter(|call| call.expression().source_unit() == consumer.id())
        .collect();
    gate(
        "real q.entry call selects actual p.main",
        matches!(calls.as_slice(), [call]
            if call.target() == UnitCallTarget::Declaration(main.id())
                && call.arguments().is_empty() && call.receiver().is_none()),
        (&mains, &consumers, &calls),
        stages,
    )?;
    inspect_single(source, parsed, names, typed, owned, stages)
}
