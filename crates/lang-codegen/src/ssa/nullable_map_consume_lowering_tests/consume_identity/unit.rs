//! Source-qualified unit facts; local expression and symbol numbers never cross files.
use super::*;
use lang_frontend::{
    name_resolution::{
        SourceUnitId, Symbol, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipBindingKind, UnitCallArgumentOwnershipKind,
        UnitDropPoint, UnitDropTarget, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
        UnitValueDeliveryKind,
    },
    parser::LiteralKind,
    type_checking::{
        CompilationUnitTypes, Copyability, ExpressionCategory, IntrinsicCallable,
        IntrinsicTypeConstructor, ParameterMode, UnitCallReceiverOrigin, UnitCallTarget,
        UnitCallableTarget, UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

struct Fe<'a> {
    sources: &'a SourceMap,
    files: &'a [(SourceUnitId, &'a ParsedFile)],
    names: &'a ValidatedCompilationUnitNames,
    typed: &'a CompilationUnitTypes,
    owned: &'a CompilationUnitOwnership,
}

struct SourceInput {
    initializer: UnitExpressionId,
    declaration: Span,
    callable: Span,
    callable_name: Span,
    storage: UnitTypeId,
    inner: UnitTypeId,
    scalar: UnitTypeId,
}

fn file<'a>(
    files: &[(SourceUnitId, &'a ParsedFile)],
    source: SourceUnitId,
) -> Outcome<&'a ParsedFile> {
    let matches: Vec<_> = files
        .iter()
        .filter(|(id, _)| *id == source)
        .map(|(_, file)| *file)
        .collect();
    one(&matches, "unit unique source-qualified AST").copied()
}

fn symbol(names: &ValidatedCompilationUnitNames, root: UnitSymbolId) -> Outcome<&Symbol> {
    let resolutions: Vec<_> = names
        .names()
        .source_units()
        .iter()
        .filter(|value| value.source_unit() == root.source_unit())
        .collect();
    let resolution = *one(&resolutions, "unit exact source resolution")?;
    let symbols: Vec<_> = resolution
        .resolution()
        .symbols()
        .iter()
        .filter(|value| value.id() == root.symbol())
        .collect();
    one(&symbols, "unit exact source-qualified symbol").copied()
}

fn reference(
    names: &ValidatedCompilationUnitNames,
    parsed: &ParsedFile,
    expression: UnitExpressionId,
) -> Outcome<UnitSymbolId> {
    let leaf = transparent(parsed, expression.expression())?;
    let node = parsed
        .ast()
        .expressions()
        .get(leaf)
        .map_err(|error| missing("unit reference node", error))?;
    gate(
        "unit reference is Name",
        matches!(node.payload(), Expression::Name),
        node,
    )?;
    let roots: Vec<_> = names
        .names()
        .references()
        .iter()
        .filter_map(|reference| match reference.target() {
            UnitReferenceTarget::Symbol(root)
                if reference.source_unit() == expression.source_unit()
                    && reference.span() == node.span() =>
            {
                Some(*root)
            }
            _ => None,
        })
        .collect();
    let root = *one(&roots, "unit unique source-qualified Name target")?;
    gate(
        "unit reference stays in its source",
        root.source_unit() == expression.source_unit(),
        root,
    )?;
    Ok(root)
}

fn map_shape(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    constructor: IntrinsicTypeConstructor,
    scalar: UnitTypeId,
) -> Outcome<()> {
    gate(
        "unit exact Map kind and K/V identity",
        matches!(typed.types().get(ty),
        Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
        if *actual == constructor && arguments.as_slice() == [scalar, scalar]),
        (ty, typed.types().get(ty)),
    )
}

fn member_call(
    sources: &SourceMap,
    parsed: &ParsedFile,
    call: UnitExpressionId,
    receiver: UnitExpressionId,
    name: &str,
    count: usize,
) -> Outcome<()> {
    let node = parsed
        .ast()
        .expressions()
        .get(call.expression())
        .map_err(|error| missing("unit actual member call", error))?;
    let Expression::Call {
        callee, arguments, ..
    } = node.payload()
    else {
        return Err(missing("unit Call AST", node));
    };
    let member = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|error| missing("unit actual member callee", error))?;
    gate(
        "unit descriptor preserves source, original receiver and member",
        call.source_unit() == receiver.source_unit()
            && matches!(member.payload(),
        Expression::Member { receiver: actual, name_span, safe: false, .. }
        if *actual == receiver.expression() && sources.slice(*name_span).ok() == Some(name))
            && arguments.len() == count,
        (node, member),
    )
}

fn construction(
    parsed: &ParsedFile,
    typed: &CompilationUnitTypes,
    initializer: UnitExpressionId,
    inner: UnitTypeId,
    scalar: UnitTypeId,
) -> Outcome<Span> {
    let expression = UnitExpressionId::new(
        initializer.source_unit(),
        transparent(parsed, initializer.expression())?,
    );
    let candidates: Vec<_> = typed
        .map_constructions()
        .iter()
        .filter(|value| value.expression() == expression)
        .collect();
    let descriptor = *one(&candidates, "unit initializer's actual MapConstruction")?;
    gate(
        "unit construction exact mutable K/V source",
        descriptor.callable() == IntrinsicCallable::MutableMapOf
            && descriptor.map_type() == inner
            && descriptor.key_type() == scalar
            && descriptor.value_type() == scalar
            && typed.expression_type(initializer) == Some(inner),
        descriptor,
    )?;
    span(parsed, expression.expression())
}

fn local_source(case: &Case, fe: &Fe<'_>, source: &SourceInput) -> Outcome<Source> {
    let Fe {
        sources,
        files,
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
    let parsed = file(files, initializer.source_unit())?;

    if case.id == "C1" {
        return Ok(Source::Local {
            construction: construction(parsed, typed, initializer, inner, scalar)?,
            puts: Vec::new(),
        });
    }
    gate(
        "unit local provenance is original C2",
        case.id == "C2",
        case.id,
    )?;
    let seed = reference(names, parsed, initializer)?;
    let seed_symbol = symbol(names, seed)?;
    let (_, seed_declaration, seed_initializer) = declaration(parsed, seed_symbol.span())?;
    gate(
        "unit C2 seed moves into source without separate cleanup",
        seed_symbol.kind() == SymbolKind::Variable
            && typed.symbol_type(seed) == Some(inner)
            && typed.expression_type(initializer) == Some(inner)
            && typed.expression_category(initializer) == Some(ExpressionCategory::Place)
            && owned.formed_owned_locals().contains(&seed)
            && owned
                .drops()
                .iter()
                .all(|drop| drop.target() != UnitDropTarget::Named(seed))
            && seed_declaration.end() < declaration_span.start(),
        (seed, seed_symbol, initializer),
    )?;
    let construction = construction(
        parsed,
        typed,
        UnitExpressionId::new(seed.source_unit(), seed_initializer),
        inner,
        scalar,
    )?;
    let mut puts = Vec::new();
    for put in typed.map_puts() {
        gate(
            "unit C2 put retains exact source, seed and scalar arguments",
            put.expression().source_unit() == initializer.source_unit()
                && put.receiver().source_unit() == initializer.source_unit()
                && put.key().source_unit() == initializer.source_unit()
                && put.value().source_unit() == initializer.source_unit()
                && reference(names, parsed, put.receiver())? == seed
                && typed.expression_type(put.receiver()) == Some(inner)
                && put.key_type() == scalar
                && put.value_type() == scalar
                && typed.expression_type(put.key()) == Some(scalar)
                && typed.expression_type(put.value()) == Some(scalar)
                && call_argument(parsed, put.expression().expression(), 0)?
                    == put.key().expression()
                && call_argument(parsed, put.expression().expression(), 1)?
                    == put.value().expression(),
            put,
        )?;
        member_call(sources, parsed, put.expression(), put.receiver(), "put", 2)?;
        puts.push(span(parsed, put.expression().expression())?);
    }
    puts.sort_by_key(|value| value.start());
    gate(
        "unit C2 actual two put sequence before source move",
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
        files,
        names,
        typed,
        owned,
        ..
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

    let parsed = file(files, initializer.source_unit())?;
    let input = reference(names, parsed, initializer)?;
    let parameter_symbol = symbol(names, input)?;
    let declarations: Vec<_> = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|value| {
            value.source_unit() == input.source_unit()
                && value.name_span() == callable_name
                && value.kind() == SymbolKind::Function
        })
        .collect();
    let function_declaration = *one(&declarations, "unit consume callable declaration identity")?;
    let signature = typed
        .signatures()
        .declaration(function_declaration.id())
        .ok_or_else(|| missing("unit callable declaration signature", function_declaration))?;
    let callable = signature
        .callable()
        .ok_or_else(|| missing("unit concrete callable signature", signature))?;
    let parameters: Vec<_> = callable
        .parameters()
        .iter()
        .enumerate()
        .filter(|(_, parameter)| parameter.symbol() == Some(input))
        .collect();
    let (index, parameter) = *one(&parameters, "unit own input parameter column")?;
    gate(
        "unit local source comes from the real provider's own nullable input",
        names.names().declaration_symbol(function_declaration.id()) == Some(signature.symbol())
            && signature.symbol().source_unit() == input.source_unit()
            && callable.target() == UnitCallableTarget::Declaration(function_declaration.id())
            && callable.name_span() == callable_name
            && callable.parameters().len() == 1
            && parameter_symbol.kind() == SymbolKind::ValueParameter
            && contains(callable_span, parameter_symbol.span())
            && contains(parameter.span(), parameter_symbol.span())
            && parameter.mode() == ParameterMode::Value
            && parameter.ty() == storage
            && owned.binding_kind(input) == Some(OwnershipBindingKind::Owned)
            && typed.symbol_type(input) == Some(storage)
            && typed.expression_type(initializer) == Some(storage)
            && typed.expression_category(initializer) == Some(ExpressionCategory::Place)
            && typed.non_null_use(initializer).is_none()
            && owned
                .drops()
                .iter()
                .all(|drop| drop.target() != UnitDropTarget::Named(input)),
        (input, parameter_symbol, signature, parameter),
    )?;
    let mut calls = Vec::new();
    for call in typed
        .calls()
        .iter()
        .filter(|call| call.target() == UnitCallTarget::Declaration(function_declaration.id()))
    {
        let parsed = file(files, call.expression().source_unit())?;
        calls.push((span(parsed, call.expression().expression())?, call));
    }
    calls.sort_by_key(|(origin, call)| (call.expression().source_unit(), origin.start()));
    gate(
        "unit C3 has exactly two actual target calls",
        calls.len() == 2,
        &calls,
    )?;
    let mut sites = Vec::new();
    let mut roots = Vec::new();
    for (ordinal, (_, call)) in calls.into_iter().enumerate() {
        let source = call.expression().source_unit();
        let caller_file = file(files, source)?;
        let argument = one(call.arguments(), "unit C3 unique argument mapping")?;
        gate(
            "unit C3 exact own parameter mapping",
            source != input.source_unit()
                && argument.parameter_index() == index
                && argument.argument_index() == 0
                && argument.mode() == ParameterMode::Value
                && argument.parameter_type() == storage
                && !argument.crosses_thread()
                && argument.category() == ExpressionCategory::Place
                && call.receiver().is_none()
                && !call.aborts(),
            call,
        )?;
        let operand = UnitExpressionId::new(
            source,
            call_argument(
                caller_file,
                call.expression().expression(),
                argument.argument_index(),
            )?,
        );
        let contracts: Vec<_> = owned
            .call_argument_contracts_for(call.expression())
            .collect();
        let contract = *one(&contracts, "unit C3 actual ownership argument contract")?;
        gate(
            "unit C3 ownership contract keeps argument and provider parameter identity",
            contract.call() == call.expression()
                && contract.argument() == operand
                && contract.parameter_index() == index
                && contract.parameter_type() == storage
                && contract.category() == ExpressionCategory::Place
                && contract.kind() == UnitCallArgumentOwnershipKind::Value
                && contract.parameter_span() == Some(parameter.span())
                && !contract.crosses_thread()
                && contract.argument_span() == span(caller_file, operand.expression())?
                && contract.call_span() == span(caller_file, call.expression().expression())?,
            contract,
        )?;
        let root = reference(names, caller_file, operand)?;
        let deliveries: Vec<_> = owned
            .value_deliveries()
            .iter()
            .filter(|delivery| {
                delivery.call() == call.expression() && delivery.argument() == operand
            })
            .collect();
        let delivery = *one(&deliveries, "unit C3 actual own argument delivery")?;
        gate(
            "unit C3 actual delivery moves the exact caller root",
            delivery.kind() == UnitValueDeliveryKind::Move
                && delivery
                    .place()
                    .is_some_and(|place| place.is_root() && place.root() == root)
                && delivery.span() == span(caller_file, operand.expression())?
                && delivery.parameter_span() == Some(parameter.span()),
            delivery,
        )?;
        let root_symbol = symbol(names, root)?;
        let (_, declaration_span, initializer) = declaration(caller_file, root_symbol.span())?;
        let call_span = span(caller_file, call.expression().expression())?;
        let (caller, caller_name) = super::callable(caller_file, call_span)?;
        let caller_declarations: Vec<_> = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|value| {
                value.source_unit() == source
                    && value.name_span() == caller_name
                    && value.name() == "entry"
                    && value.kind() == SymbolKind::Function
            })
            .collect();
        let caller_declaration = *one(&caller_declarations, "unit C3 actual q.entry caller")?;
        let caller_units: Vec<_> = names
            .names()
            .index()
            .source_units()
            .iter()
            .filter(|value| value.id() == source)
            .collect();
        let caller_unit = *one(&caller_units, "unit C3 caller source identity")?;
        gate(
            "unit C3 caller owns and transfers its distinct source once",
            caller_unit.source_id() == caller_file.source_id()
                && caller_unit
                    .key()
                    .logical_path()
                    .expect("rooted source")
                    .as_str()
                    == "q/consumer.ko"
                && caller_unit.package() == caller_declaration.package()
                && names.names().index().packages().iter().any(|package| {
                    package.id() == caller_unit.package()
                        && package.name().expect("named package").segments() == ["q"]
                })
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
                    .all(|drop| drop.target() != UnitDropTarget::Named(root)),
            (caller_unit, caller_declaration, root, call),
        )?;
        let initializer_id = UnitExpressionId::new(source, initializer);
        let initializer = if ordinal == 0 {
            CallerInit::Present {
                construction: construction(caller_file, typed, initializer_id, inner, scalar)?,
            }
        } else {
            let leaf = transparent(caller_file, initializer)?;
            let node = caller_file
                .ast()
                .expressions()
                .get(leaf)
                .map_err(|error| missing("unit C3 absent initializer", error))?;
            gate(
                "unit C3 second caller actually initializes null",
                matches!(node.payload(), Expression::Literal(LiteralKind::Null))
                    && typed.expression_type(initializer_id) == Some(storage),
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
        "unit C3 ordered calls belong to the same real caller",
        sites.windows(2).all(|pair| {
            pair[0].callable == pair[1].callable && pair[0].call.end() < pair[1].declaration.start()
        }),
        &sites,
    )?;
    Ok(Source::Parameter {
        parameter: parameter_symbol.span(),
        index,
        calls: sites,
    })
}

pub(in crate::ssa::nullable_map_consume_lowering_tests) fn facts(
    case: &Case,
    sources: &SourceMap,
    files: &[(SourceUnitId, &ParsedFile)],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    trace: &mut Trace,
) -> Outcome<Facts> {
    let descriptor = one(typed.map_consumes(), "unit unique real consume descriptor")?;
    let source = descriptor.expression().source_unit();
    let parsed = file(files, source)?;
    let units: Vec<_> = names
        .names()
        .index()
        .source_units()
        .iter()
        .filter(|value| value.id() == source)
        .collect();
    let unit = *one(&units, "unit consume source identity")?;
    gate(
        "unit consume belongs to actual p/provider source and package",
        unit.source_id() == parsed.source_id()
            && unit.key().logical_path().expect("rooted source").as_str() == "p/provider.ko"
            && descriptor.receiver().source_unit() == source
            && names.names().index().packages().iter().any(|package| {
                package.id() == unit.package()
                    && package.name().expect("named package").segments() == ["p"]
            }),
        unit,
    )?;
    let (root, storage) = descriptor
        .source()
        .ok_or_else(|| missing("unit descriptor source root/storage", descriptor))?;
    let root_symbol = symbol(names, root)?;
    let (_, declaration_span, initializer) = declaration(parsed, root_symbol.span())?;
    let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(storage) else {
        return Err(missing("unit nullable storage", storage));
    };
    let scalar = typed
        .types()
        .builtin(case.scalar)
        .ok_or_else(|| missing("unit concrete scalar", case.scalar))?;
    map_shape(typed, *inner, IntrinsicTypeConstructor::MutableMap, scalar)?;
    map_shape(
        typed,
        descriptor.result_type(),
        IntrinsicTypeConstructor::Map,
        scalar,
    )?;
    gate(
        "unit consume source/root/storage/read/result identity",
        root.source_unit() == source
            && reference(names, parsed, descriptor.receiver())? == root
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
                .all(|ty| typed.copyability(*ty) == Copyability::MoveOnly),
        (descriptor, root, root_symbol, storage),
    )?;
    member_call(
        sources,
        parsed,
        descriptor.expression(),
        descriptor.receiver(),
        "consume",
        0,
    )?;
    let moves: Vec<_> = owned
        .receiver_facts()
        .iter()
        .filter(|fact| fact.call() == descriptor.expression())
        .collect();
    let movement = *one(&moves, "unit actual consume receiver ownership fact")?;
    gate(
        "unit receiver actually moves this source-qualified whole root",
        movement.source() == UnitCallReceiverOrigin::Expression(descriptor.receiver())
            && movement.receiver_type() == descriptor.receiver_type()
            && movement.kind() == UnitReceiverOwnershipKind::Move
            && matches!(movement.target(), UnitReceiverOwnershipTarget::Place(place)
                if place.is_root() && place.root() == root),
        movement,
    )?;
    let mut receiver = descriptor.receiver();
    loop {
        let fact = typed
            .non_null_use(receiver)
            .ok_or_else(|| missing("unit every Name/Group NonNullUse", receiver))?;
        gate(
            "unit Name/Group source, root, storage, narrowed type and Place",
            receiver.source_unit() == source
                && fact.expression() == receiver
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
            .filter(|drop| drop.point() == UnitDropPoint::AfterExpression(receiver))
            .collect();
        gate(
            "unit receiver AfterExpression has no nonempty DropFact",
            drops.is_empty(),
            &drops,
        )?;
        trace.stages.push(format!("consume identity unit receiver point={receiver:?}; fact={fact:?}; DropFacts empty (no emitter-order claim)"));
        let node = parsed
            .ast()
            .expressions()
            .get(receiver.expression())
            .map_err(|error| missing("unit receiver node", error))?;
        match node.payload() {
            Expression::Group { expression } => {
                receiver = UnitExpressionId::new(source, *expression)
            }
            Expression::Name => break,
            other => return Err(missing("unit transparent consume receiver", other)),
        }
    }
    let comparison = one(typed.null_comparisons(), "unit actual comparison")?;
    gate(
        "unit comparison same source storage and fixture polarity",
        comparison.expression().source_unit() == source
            && comparison.symbol() == root
            && comparison.nullable_type() == storage
            && Some(comparison.non_null_when_true()) == case.non_null_when_true,
        comparison,
    )?;
    let consume = span(parsed, descriptor.expression().expression())?;
    let (control_id, control_span) = control(
        parsed,
        comparison.expression().expression(),
        consume,
        comparison.non_null_when_true(),
    )?;
    let control_id = UnitExpressionId::new(source, control_id);
    let (callable_span, callable_name) = callable(parsed, consume)?;
    gate(
        "unit source declaration and branch belong to actual consume callable",
        contains(callable_span, declaration_span)
            && contains(callable_span, control_span)
            && declaration_span.end() < control_span.start(),
        (callable_span, declaration_span, control_span),
    )?;
    let root_drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(root))
        .collect();
    let root_drop = *one(&root_drops, "unit unique null-path source DropFact")?;
    gate(
        "unit source null branch follows descriptor polarity",
        root_drop.point()
            == UnitDropPoint::BranchExit {
                control: control_id,
                branch: usize::from(comparison.non_null_when_true()),
            }
            && root_drop.value_origin() == root_symbol.span(),
        root_drop,
    )?;
    let cleanup = if case.id == "C2" {
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(descriptor.expression()))
            .collect();
        let drop = *one(&drops, "unit actual discarded consume result DropFact")?;
        gate(
            "unit temporary consume AfterExpression cleanup",
            typed.map_sizes().is_empty()
                && drop.point() == UnitDropPoint::AfterExpression(descriptor.expression())
                && drop.value_origin() == consume,
            drop,
        )?;
        Cleanup {
            origin: drop.value_origin(),
            read: None,
        }
    } else {
        gate(
            "unit named result fixture",
            matches!(case.id, "C1" | "C3"),
            case.id,
        )?;
        let read = one(typed.map_sizes(), "unit result's actual size descriptor")?;
        gate(
            "unit size source-qualified identity",
            read.expression().source_unit() == source && read.receiver().source_unit() == source,
            read,
        )?;
        let read_node = parsed
            .ast()
            .expressions()
            .get(read.expression().expression())
            .map_err(|error| missing("unit actual size member", error))?;
        gate(
            "unit size descriptor matches actual AST receiver and member",
            matches!(read_node.payload(), Expression::Member { receiver, name_span, safe: false, .. }
                if *receiver == read.receiver().expression() && sources.slice(*name_span).ok() == Some("size"))
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
            "unit size reads the declared consume result",
            transparent(parsed, result_initializer)? == descriptor.expression().expression()
                && result != root
                && typed.symbol_type(result) == Some(descriptor.result_type())
                && read.map_type() == descriptor.result_type()
                && typed.expression_type(read.receiver()) == Some(descriptor.result_type())
                && typed.expression_category(read.receiver()) == Some(ExpressionCategory::Place)
                && contains(control_span, result_declaration)
                && result_declaration.end() < read_node.span().start()
                && owned.formed_owned_locals().contains(&result),
            (read, result, result_symbol),
        )?;
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(result))
            .collect();
        let drop = *one(&drops, "unit actual named consume result DropFact")?;
        gate(
            "unit named result uses actual size CallReturn cleanup",
            drop.point() == UnitDropPoint::CallReturn(read.expression())
                && drop.value_origin() == result_symbol.span(),
            drop,
        )?;
        Cleanup {
            origin: drop.value_origin(),
            read: Some(span(parsed, read.expression().expression())?),
        }
    };
    let initializer = UnitExpressionId::new(source, initializer);
    let fe = Fe {
        sources,
        files,
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
    trace.stages.push(format!("consume identity real unit input: unit={unit:#?}; path={:?}; descriptor={descriptor:#?}; comparison={comparison:#?}; receiver Move={movement:#?}; provenance={source:#?}; cleanup={cleanup:#?}; drops={:#?}",
        sources.source_name(parsed.source_id()), owned.drops()));
    Ok(Facts {
        callable: callable_span,
        root: root_symbol.span(),
        declaration: declaration_span,
        receiver: span(parsed, descriptor.receiver().expression())?,
        consume,
        control: control_span,
        scalar: case.scalar,
        source,
        cleanup,
    })
}
