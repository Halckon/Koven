//! Read the single-file consume, provenance and cleanup facts without rebuilding them.
use super::*;
use lang_frontend::{
    name_resolution::{NameResolution, ReferenceTarget, Symbol, SymbolId, SymbolKind},
    ownership_checking::{DropPoint, DropTarget, OwnershipBindingKind, OwnershipCheckedFile},
    parser::LiteralKind,
    type_checking::{
        CallableTarget, Copyability, ExpressionCategory, IntrinsicCallable,
        IntrinsicTypeConstructor, ParameterMode, TypeId, TypeKind, TypedFile,
    },
};

struct Fe<'a> {
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
    names: &'a NameResolution,
    typed: &'a TypedFile,
    owned: &'a OwnershipCheckedFile,
}

struct SourceInput {
    initializer: ExpressionId,
    declaration: Span,
    callable: Span,
    callable_name: Span,
    storage: TypeId,
    inner: TypeId,
    scalar: TypeId,
}

fn symbol(names: &NameResolution, id: SymbolId) -> Outcome<&Symbol> {
    let matches: Vec<_> = names
        .symbols()
        .iter()
        .filter(|value| value.id() == id)
        .collect();
    one(&matches, "single symbol identity").copied()
}

fn reference(
    names: &NameResolution,
    parsed: &ParsedFile,
    expression: ExpressionId,
) -> Outcome<SymbolId> {
    let leaf = transparent(parsed, expression)?;
    let node = parsed
        .ast()
        .expressions()
        .get(leaf)
        .map_err(|error| missing("single reference node", error))?;
    gate(
        "single reference is Name",
        matches!(node.payload(), Expression::Name),
        node,
    )?;
    let roots: Vec<_> = names
        .references()
        .iter()
        .filter_map(|reference| match reference.target() {
            ReferenceTarget::Symbol(root) if reference.span() == node.span() => Some(*root),
            _ => None,
        })
        .collect();
    one(&roots, "single unique Name target").copied()
}

fn map_shape(
    typed: &TypedFile,
    ty: TypeId,
    constructor: IntrinsicTypeConstructor,
    scalar: TypeId,
) -> Outcome<()> {
    gate(
        "single exact Map kind and K/V identity",
        matches!(typed.types().get(ty),
        Some(TypeKind::Intrinsic { constructor: actual, arguments })
        if *actual == constructor && arguments.as_slice() == [scalar, scalar]),
        (ty, typed.types().get(ty)),
    )
}

fn member_call(
    sources: &SourceMap,
    parsed: &ParsedFile,
    call: ExpressionId,
    receiver: ExpressionId,
    name: &str,
    count: usize,
) -> Outcome<()> {
    let node = parsed
        .ast()
        .expressions()
        .get(call)
        .map_err(|error| missing("single actual member call", error))?;
    let Expression::Call {
        callee, arguments, ..
    } = node.payload()
    else {
        return Err(missing("single Call AST", node));
    };
    let member = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|error| missing("single actual member callee", error))?;
    gate(
        "single descriptor preserves actual receiver and member",
        matches!(member.payload(),
        Expression::Member { receiver: actual, name_span, safe: false, .. }
        if *actual == receiver && sources.slice(*name_span).ok() == Some(name))
            && arguments.len() == count,
        (node, member),
    )
}

fn construction(
    parsed: &ParsedFile,
    typed: &TypedFile,
    initializer: ExpressionId,
    inner: TypeId,
    scalar: TypeId,
) -> Outcome<Span> {
    let expression = transparent(parsed, initializer)?;
    let candidates: Vec<_> = typed
        .map_constructions()
        .iter()
        .filter(|value| value.expression() == expression)
        .collect();
    let descriptor = *one(&candidates, "single initializer's actual MapConstruction")?;
    gate(
        "single construction exact mutable K/V source",
        descriptor.callable() == IntrinsicCallable::MutableMapOf
            && descriptor.map_type() == inner
            && descriptor.key_type() == scalar
            && descriptor.value_type() == scalar
            && typed.expression_type(initializer) == Some(inner),
        descriptor,
    )?;
    span(parsed, expression)
}

fn local_source(case: &Case, fe: &Fe<'_>, source: &SourceInput) -> Outcome<Source> {
    let Fe {
        sources,
        parsed,
        names,
        typed,
        owned,
    } = *fe;
    let SourceInput {
        initializer,
        declaration: declaration_span,
        inner,
        scalar,
        ..
    } = *source;

    if case.id == "C1" {
        return Ok(Source::Local {
            construction: construction(parsed, typed, initializer, inner, scalar)?,
            puts: Vec::new(),
        });
    }
    gate(
        "single local provenance is original C2",
        case.id == "C2",
        case.id,
    )?;
    let seed = reference(names, parsed, initializer)?;
    let seed_symbol = symbol(names, seed)?;
    let (_, seed_declaration, seed_initializer) = declaration(parsed, seed_symbol.span())?;
    gate(
        "single C2 seed moves into source without separate cleanup",
        seed_symbol.kind() == SymbolKind::Variable
            && typed.symbol_type(seed) == Some(inner)
            && typed.expression_type(initializer) == Some(inner)
            && typed.expression_category(initializer) == Some(ExpressionCategory::Place)
            && owned.formed_owned_locals().contains(&seed)
            && owned
                .drops()
                .iter()
                .all(|drop| drop.target() != DropTarget::Named(seed))
            && seed_declaration.end() < declaration_span.start(),
        (seed_symbol, initializer),
    )?;
    let construction = construction(parsed, typed, seed_initializer, inner, scalar)?;
    let mut puts = Vec::new();
    for put in typed.map_puts() {
        gate(
            "single C2 put keeps exact seed and scalar arguments",
            reference(names, parsed, put.receiver())? == seed
                && typed.expression_type(put.receiver()) == Some(inner)
                && put.key_type() == scalar
                && put.value_type() == scalar
                && typed.expression_type(put.key()) == Some(scalar)
                && typed.expression_type(put.value()) == Some(scalar)
                && call_argument(parsed, put.expression(), 0)? == put.key()
                && call_argument(parsed, put.expression(), 1)? == put.value(),
            put,
        )?;
        member_call(sources, parsed, put.expression(), put.receiver(), "put", 2)?;
        puts.push(span(parsed, put.expression())?);
    }
    puts.sort_by_key(|value| value.start());
    gate(
        "single C2 actual two put sequence before source move",
        puts.len() == 2
            && puts.iter().all(|put| {
                seed_declaration.end() < put.start() && put.end() < declaration_span.start()
            })
            && puts.windows(2).all(|pair| pair[0].end() < pair[1].start()),
        &puts,
    )?;
    Ok(Source::Local { construction, puts })
}

fn parameter_source(fe: &Fe<'_>, source: &SourceInput) -> Outcome<Source> {
    let Fe {
        sources,
        parsed,
        names,
        typed,
        owned,
    } = *fe;
    let SourceInput {
        initializer,
        callable: callable_span,
        callable_name,
        storage,
        inner,
        scalar,
        ..
    } = *source;

    let input = reference(names, parsed, initializer)?;
    let parameter = symbol(names, input)?;
    let functions: Vec<_> = names
        .symbols()
        .iter()
        .filter(|value| value.span() == callable_name && value.kind() == SymbolKind::Function)
        .collect();
    let function = *one(&functions, "single consume callable symbol")?;
    let signatures: Vec<_> = typed
        .callables()
        .iter()
        .filter(|value| value.symbol() == function.id())
        .collect();
    let signature = *one(&signatures, "single consume callable signature")?;
    let positions: Vec<_> = signature
        .parameter_symbols()
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (*value == Some(input)).then_some(index))
        .collect();
    let index = *one(&positions, "single own input parameter column")?;
    let contract = signature
        .parameters()
        .get(index)
        .ok_or_else(|| missing("single own parameter type", index))?;
    gate(
        "single local source comes from this callable's own nullable input",
        parameter.kind() == SymbolKind::ValueParameter
            && contains(callable_span, parameter.span())
            && signature.parameter_symbols().len() == 1
            && signature.parameters().len() == 1
            && contract.mode == ParameterMode::Value
            && contract.ty == storage
            && typed.parameter_mode(input) == Some(ParameterMode::Value)
            && owned.binding_kind(input) == Some(OwnershipBindingKind::Owned)
            && typed.symbol_type(input) == Some(storage)
            && typed.expression_type(initializer) == Some(storage)
            && typed.expression_category(initializer) == Some(ExpressionCategory::Place)
            && typed.non_null_use(initializer).is_none()
            && owned
                .drops()
                .iter()
                .all(|drop| drop.target() != DropTarget::Named(input)),
        (parameter, signature, contract),
    )?;
    let mut calls = Vec::new();
    for call in typed
        .calls()
        .iter()
        .filter(|call| call.target() == CallableTarget::Source(function.id()))
    {
        calls.push((span(parsed, call.expression())?, call));
    }
    calls.sort_by_key(|(origin, _)| origin.start());
    gate(
        "single C3 has exactly two actual target calls",
        calls.len() == 2,
        &calls,
    )?;
    let mut sites = Vec::new();
    let mut roots = Vec::new();
    for (ordinal, (_, call)) in calls.into_iter().enumerate() {
        let argument = one(call.arguments(), "single C3 unique argument mapping")?;
        gate(
            "single C3 exact own parameter mapping",
            argument.parameter_index() == index
                && argument.argument_index() == 0
                && argument.mode() == ParameterMode::Value
                && argument.parameter_type() == storage
                && argument.category() == ExpressionCategory::Place
                && !argument.crosses_thread()
                && call.receiver().is_none()
                && !call.aborts(),
            call,
        )?;
        let operand = call_argument(parsed, call.expression(), argument.argument_index())?;
        let root = reference(names, parsed, operand)?;
        let root_symbol = symbol(names, root)?;
        let (_, declaration_span, initializer) = declaration(parsed, root_symbol.span())?;
        let call_span = span(parsed, call.expression())?;
        let (caller, caller_name) = callable(parsed, call_span)?;
        gate(
            "single C3 caller owns and transfers its distinct source once",
            sources.slice(caller_name).ok() == Some("main")
                && caller != callable_span
                && contains(caller, declaration_span)
                && declaration_span.end() < call_span.start()
                && root_symbol.kind() == SymbolKind::Variable
                && !roots.contains(&root)
                && typed.symbol_type(root) == Some(storage)
                && typed.expression_type(operand) == Some(storage)
                && typed.expression_category(operand) == Some(ExpressionCategory::Place)
                && typed.non_null_use(operand).is_none()
                && owned.formed_owned_locals().contains(&root)
                && owned
                    .drops()
                    .iter()
                    .all(|drop| drop.target() != DropTarget::Named(root)),
            (root_symbol, call),
        )?;
        let initializer = if ordinal == 0 {
            CallerInit::Present {
                construction: construction(parsed, typed, initializer, inner, scalar)?,
            }
        } else {
            let leaf = transparent(parsed, initializer)?;
            let node = parsed
                .ast()
                .expressions()
                .get(leaf)
                .map_err(|error| missing("single C3 absent initializer", error))?;
            gate(
                "single C3 second caller actually initializes null",
                matches!(node.payload(), Expression::Literal(LiteralKind::Null))
                    && typed.expression_type(initializer) == Some(storage),
                node,
            )?;
            CallerInit::Absent { null: node.span() }
        };
        roots.push(root);
        sites.push(CallSite {
            callable: caller,
            call: call_span,
            root: root_symbol.span(),
            declaration: declaration_span,
            initializer,
        });
    }
    gate(
        "single C3 ordered calls belong to the same real caller",
        sites.windows(2).all(|pair| {
            pair[0].callable == pair[1].callable && pair[0].call.end() < pair[1].declaration.start()
        }),
        &sites,
    )?;
    Ok(Source::Parameter {
        parameter: parameter.span(),
        index,
        calls: sites,
    })
}

pub(in crate::ssa::nullable_map_consume_lowering_tests) fn facts(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    trace: &mut Trace,
) -> Outcome<Facts> {
    let descriptor = one(
        typed.map_consumes(),
        "single unique real consume descriptor",
    )?;
    let (root, storage) = descriptor
        .source()
        .ok_or_else(|| missing("single descriptor source root/storage", descriptor))?;
    let root_symbol = symbol(names, root)?;
    let (_, declaration_span, initializer) = declaration(parsed, root_symbol.span())?;
    let Some(TypeKind::Nullable(inner)) = typed.types().get(storage) else {
        return Err(missing("single nullable storage", storage));
    };
    let scalar = typed
        .types()
        .builtin(case.scalar)
        .ok_or_else(|| missing("single concrete scalar", case.scalar))?;
    map_shape(typed, *inner, IntrinsicTypeConstructor::MutableMap, scalar)?;
    map_shape(
        typed,
        descriptor.result_type(),
        IntrinsicTypeConstructor::Map,
        scalar,
    )?;
    gate(
        "single consume root/storage/read/result identity",
        reference(names, parsed, descriptor.receiver())? == root
            && root_symbol.kind() == SymbolKind::Variable
            && !root_symbol.is_synthetic()
            && root_symbol.span().source_id() == parsed.source_id()
            && typed.symbol_type(root) == Some(storage)
            && owned.formed_owned_locals().contains(&root)
            && descriptor.receiver_type() == *inner
            && descriptor.key_type() == scalar
            && descriptor.value_type() == scalar
            && typed.expression_type(descriptor.expression()) == Some(descriptor.result_type())
            && typed.expression_category(descriptor.expression())
                == Some(ExpressionCategory::Temporary)
            && [storage, *inner, descriptor.result_type()]
                .iter()
                .all(|ty| typed.copyability(*ty) == Some(Copyability::MoveOnly)),
        (descriptor, root_symbol, storage),
    )?;
    member_call(
        sources,
        parsed,
        descriptor.expression(),
        descriptor.receiver(),
        "consume",
        0,
    )?;
    let mut receiver = descriptor.receiver();
    loop {
        let fact = typed
            .non_null_use(receiver)
            .ok_or_else(|| missing("single every Name/Group NonNullUse", receiver))?;
        gate(
            "single Name/Group root, storage, narrowed type and Place",
            fact.expression() == receiver
                && fact.symbol() == root
                && fact.declared_type() == storage
                && fact.narrowed_type() == *inner
                && typed.expression_type(receiver) == Some(*inner)
                && typed.expression_category(receiver) == Some(ExpressionCategory::Place),
            fact,
        )?;
        // Empty facts cannot prove emitter invocation or Name/Group point order.
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.point() == DropPoint::AfterExpression(receiver))
            .collect();
        gate(
            "single receiver AfterExpression has no nonempty DropFact",
            drops.is_empty(),
            &drops,
        )?;
        trace.stages.push(format!("consume identity single receiver point={receiver:?}; fact={fact:?}; DropFacts empty (no emitter-order claim)"));
        let node = parsed
            .ast()
            .expressions()
            .get(receiver)
            .map_err(|error| missing("single receiver node", error))?;
        match node.payload() {
            Expression::Group { expression } => receiver = *expression,
            Expression::Name => break,
            other => return Err(missing("single transparent consume receiver", other)),
        }
    }
    let comparisons: Vec<_> = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, _)| typed.null_comparison(id))
        .collect();
    let comparison = *one(&comparisons, "single actual comparison")?;
    gate(
        "single comparison same source storage and fixture polarity",
        comparison.symbol() == root
            && comparison.nullable_type() == storage
            && Some(comparison.non_null_when_true()) == case.non_null_when_true,
        comparison,
    )?;
    let consume = span(parsed, descriptor.expression())?;
    let (control_id, control_span) = control(
        parsed,
        comparison.expression(),
        consume,
        comparison.non_null_when_true(),
    )?;
    let (callable_span, callable_name) = callable(parsed, consume)?;
    gate(
        "single source declaration and branch belong to actual consume callable",
        contains(callable_span, declaration_span)
            && contains(callable_span, control_span)
            && declaration_span.end() < control_span.start(),
        (callable_span, declaration_span, control_span),
    )?;
    let root_drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == DropTarget::Named(root))
        .collect();
    let root_drop = *one(&root_drops, "single unique null-path source DropFact")?;
    gate(
        "single source null branch follows descriptor polarity",
        root_drop.point()
            == DropPoint::BranchExit {
                control: control_id,
                branch: usize::from(comparison.non_null_when_true()),
            }
            && root_drop.value_origin() == root_symbol.span()
            && root_drop.owner().is_some()
            && root_drop.condition().is_none(),
        root_drop,
    )?;
    let cleanup = if case.id == "C2" {
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == DropTarget::Temporary(descriptor.expression()))
            .collect();
        let drop = *one(&drops, "single actual discarded consume result DropFact")?;
        gate(
            "single temporary consume AfterExpression cleanup",
            typed.map_sizes().is_empty()
                && drop.point() == DropPoint::AfterExpression(descriptor.expression())
                && drop.value_origin() == consume
                && drop.condition().is_none(),
            drop,
        )?;
        Cleanup {
            origin: drop.value_origin(),
            read: None,
        }
    } else {
        gate(
            "single named result fixture",
            matches!(case.id, "C1" | "C3"),
            case.id,
        )?;
        let read = one(typed.map_sizes(), "single result's actual size descriptor")?;
        let read_node = parsed
            .ast()
            .expressions()
            .get(read.expression())
            .map_err(|error| missing("single actual size member", error))?;
        gate(
            "single size descriptor matches actual AST receiver and member",
            matches!(read_node.payload(), Expression::Member { receiver, name_span, safe: false, .. }
                if *receiver == read.receiver() && sources.slice(*name_span).ok() == Some("size"))
                && contains(control_span, read_node.span())
                && typed.expression_type(read.expression())
                    == typed.types().builtin(BuiltinType::Int),
            read_node,
        )?;
        let result = reference(names, parsed, read.receiver())?;
        let result_symbol = symbol(names, result)?;
        let (_, result_declaration, result_initializer) =
            declaration(parsed, result_symbol.span())?;
        gate(
            "single size reads the declared consume result",
            transparent(parsed, result_initializer)? == descriptor.expression()
                && result != root
                && typed.symbol_type(result) == Some(descriptor.result_type())
                && read.map_type() == descriptor.result_type()
                && typed.expression_type(read.receiver()) == Some(descriptor.result_type())
                && typed.expression_category(read.receiver()) == Some(ExpressionCategory::Place)
                && contains(control_span, result_declaration)
                && result_declaration.end() < read_node.span().start()
                && owned.formed_owned_locals().contains(&result),
            (read, result_symbol),
        )?;
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == DropTarget::Named(result))
            .collect();
        let drop = *one(&drops, "single actual named consume result DropFact")?;
        gate(
            "single named result uses actual size CallReturn cleanup",
            drop.point() == DropPoint::CallReturn(read.expression())
                && drop.value_origin() == result_symbol.span()
                && drop.owner().is_some()
                && drop.condition().is_none(),
            drop,
        )?;
        Cleanup {
            origin: drop.value_origin(),
            read: Some(span(parsed, read.expression())?),
        }
    };
    let fe = Fe {
        sources,
        parsed,
        names,
        typed,
        owned,
    };
    let source_input = SourceInput {
        initializer,
        declaration: declaration_span,
        callable: callable_span,
        callable_name,
        storage,
        inner: *inner,
        scalar,
    };
    let source = if case.id == "C3" {
        parameter_source(&fe, &source_input)?
    } else {
        local_source(case, &fe, &source_input)?
    };
    trace.stages.push(format!("consume identity real single input: source={:?}; descriptor={descriptor:#?}; comparison={comparison:#?}; provenance={source:#?}; cleanup={cleanup:#?}; drops={:#?}",
        sources.source_name(parsed.source_id()), owned.drops()));
    Ok(Facts {
        callable: callable_span,
        root: root_symbol.span(),
        declaration: declaration_span,
        receiver: span(parsed, descriptor.receiver())?,
        consume,
        control: control_span,
        scalar: case.scalar,
        source,
        cleanup,
    })
}
