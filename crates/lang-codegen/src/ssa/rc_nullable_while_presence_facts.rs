//! Read-only source/FE identities for the four real SPEC-0332 presence fixtures.
//! Drop points are printed as published; execution ordering is checked separately in SSA.
use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{
        DeclarationId, NameResolution, ReferenceTarget, SymbolId, SymbolKind, UnitReferenceTarget,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropTarget, OwnershipBindingKind, OwnershipCheckedFile,
        UnitDropTarget,
    },
    parser::{
        AssignmentOperator, BinaryOperator, Expression, FunctionBody, FunctionForm, Item,
        LiteralKind, NameMarker, ParameterModeMarker, ParsedFile, Statement, VariableKind,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, ConstructionTarget, Copyability,
        IntrinsicTypeConstructor, ParameterMode, TypeKind, TypedFile, UnitCallTarget,
        UnitConstructionTarget, UnitExpressionId, UnitTypeKind, ValidatedCompilationUnitTypes,
    },
};

type Query<T> = Result<T, String>;

#[derive(Clone, Copy, Debug)]
pub(super) struct CallSpans {
    pub(super) inspect: Span,
    pub(super) reader: Span,
}

#[derive(Debug)]
pub(super) struct Facts {
    pub(super) inspect: Span,
    pub(super) reader: Span,
    pub(super) entry: Span,
    pub(super) guard: Span,
    pub(super) while_span: Span,
    pub(super) tail_return: Span,
    pub(super) reader_comparison: Span,
    pub(super) reader_return: Span,
    pub(super) reader_drop_origin: Span,
    pub(super) entry_present_statement: Span,
    pub(super) entry_construction: Span,
    pub(super) entry_payload: Span,
    pub(super) entry_null: Span,
    pub(super) body_return: Option<Span>,
    pub(super) body_null: Option<Span>,
    pub(super) backedge_origin: Option<Span>,
    pub(super) inspect_drop_origin: Option<Span>,
    pub(super) calls: [CallSpans; 2],
}

#[derive(Clone, Copy)]
struct FunctionSource {
    root: ItemId,
    span: Span,
    name: Span,
    parameter: Option<Span>,
    body: StatementId,
}

impl FunctionSource {
    fn parameter(self) -> Query<Span> {
        self.parameter
            .ok_or_else(|| "source parameter span absent".to_owned())
    }
}

#[derive(Clone, Copy)]
struct VariableSource {
    statement: Span,
    name: Span,
    initializer: ExpressionId,
}

#[derive(Clone, Copy)]
struct CallSource {
    id: ExpressionId,
    span: Span,
    callee: Span,
    argument: ExpressionId,
}

#[derive(Clone, Copy)]
struct NestedCall {
    inspect: CallSource,
    reader: CallSource,
    input_name: Span,
}

struct Source {
    inspect: FunctionSource,
    reader: FunctionSource,
    entry: FunctionSource,
    local: Option<VariableSource>,
    guard: ExpressionId,
    guard_span: Span,
    guard_name: Span,
    while_span: Span,
    tail_return: Span,
    tail_name: Span,
    reader_comparison: ExpressionId,
    reader_comparison_span: Span,
    reader_name: Span,
    reader_return: Span,
    assignment_name: Option<Span>,
    body_return: Option<Span>,
    body_null: Option<Span>,
    backedge_origin: Option<Span>,
    present: VariableSource,
    absent: VariableSource,
    construction: CallSource,
    payload: Span,
    null: Span,
    calls: [NestedCall; 2],
}

fn require(valid: bool, label: &str) -> Query<()> {
    if valid { Ok(()) } else { Err(label.to_owned()) }
}

fn one<T>(mut values: impl Iterator<Item = T>, label: &str) -> Query<T> {
    let value = values.next().ok_or_else(|| format!("{label}: absent"))?;
    require(values.next().is_none(), &format!("{label}: not unique"))?;
    Ok(value)
}

fn text(sources: &SourceMap, span: Span) -> Query<&str> {
    sources
        .slice(span)
        .map_err(|error| format!("source slice: {error:?}"))
}

fn expression(file: &ParsedFile, id: ExpressionId) -> Query<(&Expression, Span)> {
    let node = file
        .ast()
        .expressions()
        .get(id)
        .map_err(|e| format!("expression: {e:?}"))?;
    Ok((node.payload(), node.span()))
}

fn statement(file: &ParsedFile, id: StatementId) -> Query<(&Statement, Span)> {
    let node = file
        .ast()
        .statements()
        .get(id)
        .map_err(|e| format!("statement: {e:?}"))?;
    Ok((node.payload(), node.span()))
}

fn elements(file: &ParsedFile, id: StatementId) -> Query<&[StatementId]> {
    let (Statement::Block { elements }, _) = statement(file, id)? else {
        return Err("expected the actual ordinary block".into());
    };
    Ok(elements)
}

fn statement_expression(file: &ParsedFile, id: StatementId) -> Query<ExpressionId> {
    let (Statement::Expression { expression }, _) = statement(file, id)? else {
        return Err("expected expression statement".into());
    };
    Ok(*expression)
}

fn name(file: &ParsedFile, id: ExpressionId) -> Query<Span> {
    let (Expression::Name, span) = expression(file, id)? else {
        return Err("expected direct name expression".into());
    };
    Ok(span)
}

fn null(file: &ParsedFile, id: ExpressionId) -> Query<Span> {
    let (Expression::Literal(LiteralKind::Null), span) = expression(file, id)? else {
        return Err("expected actual null literal".into());
    };
    Ok(span)
}

fn returned(file: &ParsedFile, id: StatementId) -> Query<(Span, ExpressionId)> {
    let id = statement_expression(file, id)?;
    let (
        Expression::Return {
            value: Some(value), ..
        },
        span,
    ) = expression(file, id)?
    else {
        return Err("expected source return with a value".into());
    };
    Ok((span, *value))
}

fn function(sources: &SourceMap, file: &ParsedFile, wanted: &str) -> Query<FunctionSource> {
    let mut found = Vec::new();
    for root in file.roots().iter().copied() {
        let node = file
            .ast()
            .items()
            .get(root)
            .map_err(|e| format!("root: {e:?}"))?;
        let Item::Function {
            name: NameMarker::Present(name),
            parameters,
            form,
            ..
        } = node.payload()
        else {
            return Err("presence source root is not a named function".into());
        };
        if text(sources, *name)? != wanted {
            continue;
        }
        let FunctionForm::Explicit {
            result_source: None,
            body: FunctionBody::Block(body),
            ..
        } = form
        else {
            return Err("presence function lacks an ordinary owned-result block".into());
        };
        let parameter = if wanted == "entry" {
            require(parameters.is_empty(), "entry has no source parameters")?;
            None
        } else {
            let parameter = one(parameters.iter(), "sole source parameter")?;
            require(
                matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_))),
                "source parameter is explicit own",
            )?;
            let NameMarker::Present(name) = parameter.name else {
                return Err("source parameter name absent".into());
            };
            Some(name)
        };
        found.push(FunctionSource {
            root,
            span: node.span(),
            name: *name,
            parameter,
            body: *body,
        });
    }
    one(found.into_iter(), wanted)
}

fn variable(
    file: &ParsedFile,
    id: StatementId,
    expected_kind: VariableKind,
) -> Query<VariableSource> {
    let (Statement::LocalVariable { declaration }, statement) = statement(file, id)? else {
        return Err("expected local variable statement".into());
    };
    let node = file
        .ast()
        .items()
        .get(*declaration)
        .map_err(|e| format!("local: {e:?}"))?;
    let Item::Variable {
        kind,
        name: NameMarker::Present(name),
        initializer,
        ..
    } = node.payload()
    else {
        return Err("expected named variable declaration".into());
    };
    require(
        *kind == expected_kind,
        "actual local declaration mutability",
    )?;
    Ok(VariableSource {
        statement,
        name: *name,
        initializer: *initializer,
    })
}

fn comparison(file: &ParsedFile, id: ExpressionId, expected: BinaryOperator) -> Query<Span> {
    let (
        Expression::Binary {
            left,
            operator,
            right,
            ..
        },
        _,
    ) = expression(file, id)?
    else {
        return Err("expected source null comparison".into());
    };
    require(*operator == expected, "source null comparison polarity")?;
    null(file, *right)?;
    name(file, *left)
}

fn call(file: &ParsedFile, id: ExpressionId) -> Query<CallSource> {
    let (
        Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        },
        span,
    ) = expression(file, id)?
    else {
        return Err("expected source call".into());
    };
    let callee = name(file, *callee)?;
    let argument = one(arguments.iter(), "sole source call argument")?;
    require(
        type_arguments.is_empty()
            && argument.named_prefix.is_none()
            && argument.mode_marker.is_none(),
        "plain direct source call",
    )?;
    Ok(CallSource {
        id,
        span,
        callee,
        argument: argument.value,
    })
}

fn nested_call(file: &ParsedFile, variable: VariableSource) -> Query<NestedCall> {
    let reader = call(file, variable.initializer)?;
    let inspect = call(file, reader.argument)?;
    let input_name = name(file, inspect.argument)?;
    Ok(NestedCall {
        inspect,
        reader,
        input_name,
    })
}

fn source(sources: &SourceMap, file: &ParsedFile, case: &str) -> Query<Source> {
    require(file.roots().len() == 3, "exact three source function roots")?;
    let inspect = function(sources, file, "inspect")?;
    let reader = function(sources, file, "readResult")?;
    let entry = function(sources, file, "entry")?;
    let changing = matches!(case, "normal-presence" | "continue-presence");
    let body = elements(file, inspect.body)?;
    require(
        body.len() == if changing { 3 } else { 2 },
        "inspect top-level source shape",
    )?;
    let local = if changing {
        Some(variable(file, body[0], VariableKind::Var)?)
    } else {
        None
    };
    let control_index = usize::from(changing);
    let (
        Statement::While {
            condition,
            body: loop_body,
            ..
        },
        while_span,
    ) = statement(file, body[control_index])?
    else {
        return Err("inspect source while absent".into());
    };
    let guard = *condition;
    let guard_span = expression(file, guard)?.1;
    let guard_name = comparison(file, guard, BinaryOperator::NotEqual)?;
    let (tail_return, tail_value) = returned(file, body[control_index + 1])?;
    let tail_name = name(file, tail_value)?;
    let loop_body = elements(file, *loop_body)?;
    require(
        loop_body.len() == if case == "continue-presence" { 3 } else { 2 },
        "exact loop source shape",
    )?;
    let mut body_return = None;
    let mut body_null = None;
    let mut backedge_origin = None;
    let mut assignment_name = None;
    let action = statement_expression(file, loop_body[1])?;
    let (action_payload, action_span) = expression(file, action)?;
    match case {
        "normal-presence" | "continue-presence" => {
            let Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } = action_payload
            else {
                return Err("loop lacks plain source assignment".into());
            };
            assignment_name = Some(name(file, *target)?);
            body_null = Some(null(file, *value)?);
            backedge_origin = Some(if case == "normal-presence" {
                while_span
            } else {
                let next = statement_expression(file, loop_body[2])?;
                let (Expression::Continue { .. }, span) = expression(file, next)? else {
                    return Err("source continue absent".into());
                };
                span
            });
        }
        "break-presence" => require(
            matches!(action_payload, Expression::Break { .. }),
            "source break absent",
        )?,
        "return-presence" => {
            let Expression::Return {
                value: Some(value), ..
            } = action_payload
            else {
                return Err("body return absent".into());
            };
            body_return = Some(action_span);
            body_null = Some(null(file, *value)?);
        }
        _ => return Err("unrecognized presence case".into()),
    }
    let reader_body = elements(file, reader.body)?;
    let reader_statement = one(reader_body.iter().copied(), "sole reader statement")?;
    let (reader_return, reader_comparison) = returned(file, reader_statement)?;
    let reader_comparison_span = expression(file, reader_comparison)?.1;
    let reader_name = comparison(file, reader_comparison, BinaryOperator::Equal)?;
    let entry_body = elements(file, entry.body)?;
    require(entry_body.len() == 6, "exact entry source statement count")?;
    let present = variable(file, entry_body[0], VariableKind::Val)?;
    let present_result = variable(file, entry_body[1], VariableKind::Val)?;
    let absent = variable(file, entry_body[3], VariableKind::Val)?;
    let absent_result = variable(file, entry_body[4], VariableKind::Val)?;
    let construction = call(file, present.initializer)?;
    let (Expression::Literal(LiteralKind::Integer(_)), payload) =
        expression(file, construction.argument)?
    else {
        return Err("entry Rc payload is not an integer literal".into());
    };
    require(
        text(sources, payload)? == "41",
        "actual entry payload is 41",
    )?;
    let null = null(file, absent.initializer)?;
    let calls = [
        nested_call(file, present_result)?,
        nested_call(file, absent_result)?,
    ];
    Ok(Source {
        inspect,
        reader,
        entry,
        local,
        guard,
        guard_span,
        guard_name,
        while_span,
        tail_return,
        tail_name,
        reader_comparison,
        reader_comparison_span,
        reader_name,
        reader_return,
        assignment_name,
        body_return,
        body_null,
        backedge_origin,
        present,
        absent,
        construction,
        payload,
        null,
        calls,
    })
}

fn finish(source: Source, inspect_drop_origin: Option<Span>, reader_drop_origin: Span) -> Facts {
    Facts {
        inspect: source.inspect.span,
        reader: source.reader.span,
        entry: source.entry.span,
        guard: source.guard_span,
        while_span: source.while_span,
        tail_return: source.tail_return,
        reader_comparison: source.reader_comparison_span,
        reader_return: source.reader_return,
        reader_drop_origin,
        entry_present_statement: source.present.statement,
        entry_construction: source.construction.span,
        entry_payload: source.payload,
        entry_null: source.null,
        body_return: source.body_return,
        body_null: source.body_null,
        backedge_origin: source.backedge_origin,
        inspect_drop_origin,
        calls: source.calls.map(|call| CallSpans {
            inspect: call.inspect.span,
            reader: call.reader.span,
        }),
    }
}

fn inspect_drop(
    origins: impl Iterator<Item = Span>,
    expected: Span,
    case: &str,
) -> Query<Option<Span>> {
    if case == "break-presence" {
        require(
            origins.count() == 0,
            "break moves parameter without a Named drop",
        )?;
        return Ok(None);
    }
    let origin = one(origins, "unique inspect Named drop")?;
    require(
        origin == expected,
        "inspect drop origin is the actual root declaration",
    )?;
    Ok(Some(origin))
}

fn symbol(names: &NameResolution, span: Span, kind: SymbolKind) -> Query<SymbolId> {
    Ok(one(
        names
            .symbols()
            .iter()
            .filter(|symbol| symbol.span() == span && symbol.kind() == kind),
        "exact declaration symbol",
    )?
    .id())
}

fn reference(names: &NameResolution, span: Span) -> Query<SymbolId> {
    let reference = one(
        names
            .references()
            .iter()
            .filter(|reference| reference.span() == span),
        "exact source reference",
    )?;
    match reference.target() {
        ReferenceTarget::Symbol(symbol) => Ok(*symbol),
        ReferenceTarget::OverloadSet(symbols) => {
            one(symbols.iter().copied(), "sole source callable reference")
        }
        _ => Err("source reference does not identify one source symbol".into()),
    }
}

pub(super) fn single(
    sources: &SourceMap,
    file: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    case: &str,
) -> Query<Facts> {
    let source = source(sources, file, case)?;
    let named_drops = |target| {
        owned
            .drops()
            .iter()
            .filter(move |drop| drop.target() == DropTarget::Named(target))
    };
    require(
        file.source_id() == names.source_id() && owned.is_compatible_with(names, typed),
        "single same source and FE analysis chain",
    )?;
    let inspect = symbol(names, source.inspect.name, SymbolKind::Function)?;
    let reader = symbol(names, source.reader.name, SymbolKind::Function)?;
    let signature = |id| {
        one(
            typed.callables().iter().filter(|s| s.symbol() == id),
            "single callable signature",
        )
    };
    let inspect_signature = signature(inspect)?;
    let reader_signature = signature(reader)?;
    let comparison = typed
        .null_comparison(source.guard)
        .ok_or("single guard descriptor absent")?;
    let reader_comparison = typed
        .null_comparison(source.reader_comparison)
        .ok_or("single reader descriptor absent")?;
    let root = comparison.symbol();
    let input = symbol(
        names,
        source.inspect.parameter()?,
        SymbolKind::ValueParameter,
    )?;
    let reader_input = symbol(
        names,
        source.reader.parameter()?,
        SymbolKind::ValueParameter,
    )?;
    for drop in owned.drops().iter().filter(|drop| match drop.target() {
        DropTarget::Named(symbol) => [root, input, reader_input].contains(&symbol),
        _ => false,
    }) {
        println!("SPEC0332 PRESENCE DROP case={case} unit=false actual={drop:?}");
    }
    let nullable = comparison.nullable_type();
    for (signature, input) in [(inspect_signature, input), (reader_signature, reader_input)] {
        let parameter = one(signature.parameters().iter(), "single sole Value parameter")?;
        require(
            signature.parameter_symbols() == [Some(input)]
                && parameter.mode == ParameterMode::Value
                && parameter.ty == nullable
                && typed.parameter_mode(input) == Some(ParameterMode::Value)
                && typed.symbol_type(input) == Some(nullable)
                && owned.binding_kind(input) == Some(OwnershipBindingKind::Owned)
                && signature.receiver().is_none()
                && signature.result_source() == CallableResultSource::Owned,
            "single exact callable owns its sole nullable Value parameter",
        )?;
    }
    require(
        inspect_signature.return_type() == nullable
            && typed.types().get(reader_signature.return_type())
                == Some(&TypeKind::Builtin(BuiltinType::Boolean)),
        "single exact function result contracts",
    )?;
    require(
        comparison.non_null_when_true()
            && reference(names, source.guard_name)? == root
            && reference(names, source.tail_name)? == root
            && reader_comparison.expression() == source.reader_comparison
            && !reader_comparison.non_null_when_true()
            && reader_comparison.symbol() == reader_input
            && reader_comparison.nullable_type() == nullable
            && reference(names, source.reader_name)? == reader_input
            && typed.expression_type(source.reader_comparison)
                == Some(reader_signature.return_type()),
        "single guard and Boolean return reference identities",
    )?;
    let origin = if let Some(local) = source.local {
        require(
            symbol(names, local.name, SymbolKind::Variable)? == root
                && reference(names, name(file, local.initializer)?)? == input
                && reference(
                    names,
                    source.assignment_name.ok_or("assignment name absent")?,
                )? == root
                && typed.symbol_type(root) == Some(nullable)
                && owned.formed_owned_locals().contains(&root)
                && named_drops(input).next().is_none(),
            "single local initializes from sole input and is the assigned/returned root",
        )?;
        local.name
    } else {
        require(root == input, "single guard/tail return use sole parameter")?;
        source.inspect.parameter()?
    };
    let Some(TypeKind::Nullable(inner)) = typed.types().get(nullable) else {
        return Err("single actual nullable type absent".into());
    };
    let Some(TypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::Rc,
        arguments,
    }) = typed.types().get(*inner)
    else {
        return Err("single nullable inner is not intrinsic Rc".into());
    };
    let payload = one(arguments.iter().copied(), "single intrinsic Rc argument")?;
    require(
        typed.types().get(payload) == Some(&TypeKind::Builtin(BuiltinType::Int))
            && typed.copyability(nullable) == Some(Copyability::MoveOnly),
        "single actual MoveOnly Rc<Int>?",
    )?;
    let construction = typed
        .construction(source.construction.id)
        .ok_or("single Rc construction descriptor absent")?;
    require(
        construction.target() == ConstructionTarget::IntrinsicRc
            && construction.result_type() == *inner,
        "single entry actual intrinsic Rc<Int> construction",
    )?;
    let mut entry_inputs = Vec::new();
    for (call, variable) in source.calls.iter().zip([source.present, source.absent]) {
        let input = symbol(names, variable.name, SymbolKind::Variable)?;
        entry_inputs.push(input);
        require(
            reference(names, call.input_name)? == input
                && typed.symbol_type(input) == Some(nullable),
            "single entry input is the actual present/absent binding",
        )?;
        for (call, target, result) in [
            (call.inspect, inspect, nullable),
            (call.reader, reader, reader_signature.return_type()),
        ] {
            let descriptor = typed
                .call(call.id)
                .ok_or("single direct call descriptor absent")?;
            let argument = one(descriptor.arguments().iter(), "single direct call argument")?;
            require(
                reference(names, call.callee)? == target
                    && descriptor.target() == CallableTarget::Source(target)
                    && descriptor.return_type() == result
                    && descriptor.result_source() == CallableResultSource::Owned
                    && argument.argument_index() == 0
                    && argument.parameter_index() == 0
                    && argument.mode() == ParameterMode::Value
                    && argument.parameter_type() == nullable,
                "single source callee, call target and owned Value delivery identity",
            )?;
        }
    }
    let inspect_drops = named_drops(root);
    let inspect_drop_origin =
        inspect_drop(inspect_drops.map(|drop| drop.value_origin()), origin, case)?;
    let reader_drop = one(named_drops(reader_input), "single unique reader Named drop")?;
    require(
        Some(reader_drop.value_origin()) == source.reader.parameter,
        "single reader drop origin belongs to its own parameter",
    )?;
    require(
        entry_inputs
            .iter()
            .all(|input| named_drops(*input).next().is_none()),
        "single entry bindings move into inspect without another Named drop",
    )?;
    Ok(finish(
        source,
        inspect_drop_origin,
        reader_drop.value_origin(),
    ))
}

fn declaration(
    names: &ValidatedCompilationUnitNames,
    source: FunctionSource,
) -> Query<DeclarationId> {
    Ok(one(
        names.names().index().declarations().iter().filter(|d| {
            d.root() == source.root
                && d.name_span() == source.name
                && d.kind() == SymbolKind::Function
        }),
        "exact unit source declaration",
    )?
    .id())
}

pub(super) fn unit(
    sources: &SourceMap,
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &ValidatedCompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    case: &str,
) -> Query<Facts> {
    require(
        owned.is_compatible_with(typed),
        "unit same typed ownership analysis",
    )?;
    let typed = typed.types();
    let source = source(sources, file, case)?;
    let named_drops = |target| {
        owned
            .drops()
            .iter()
            .filter(move |drop| drop.target() == UnitDropTarget::Named(target))
    };
    let inspect = declaration(names, source.inspect)?;
    let reader = declaration(names, source.reader)?;
    let entry = declaration(names, source.entry)?;
    let source_unit = one(
        names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.id() == inspect),
        "unit inspect declaration",
    )?
    .source_unit();
    require(
        names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| [reader, entry].contains(&d.id()))
            .all(|d| d.source_unit() == source_unit),
        "unit three functions share source unit",
    )?;
    let resolution = one(
        names
            .names()
            .source_units()
            .iter()
            .filter(|r| r.source_unit() == source_unit),
        "actual unit local resolution",
    )?
    .resolution();
    require(
        resolution.source_id() == file.source_id(),
        "unit local resolution matches parsed source",
    )?;
    let local_symbol = |span, kind| symbol(resolution, span, kind);
    let reference = |span| {
        one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.source_unit() == source_unit && r.span() == span),
            "exact source-qualified reference",
        )
    };
    let symbol_reference = |span| match reference(span)?.target() {
        UnitReferenceTarget::Symbol(symbol) => Ok(*symbol),
        _ => Err("unit reference does not identify one actual local symbol".to_owned()),
    };
    let callable_reference = |span, expected| -> Query<()> {
        let actual = match reference(span)?.target() {
            UnitReferenceTarget::Declaration(id) => *id,
            UnitReferenceTarget::OverloadSet(ids) => {
                one(ids.iter().copied(), "sole unit callable reference")?
            }
            _ => return Err("unit callee reference is not a source declaration".into()),
        };
        require(
            actual == expected,
            "unit callee names the exact source function",
        )
    };
    let signature = |id| {
        typed
            .signatures()
            .declaration(id)
            .and_then(|s| s.callable())
            .ok_or_else(|| "unit callable signature absent".to_owned())
    };
    let inspect_signature = signature(inspect)?;
    let reader_signature = signature(reader)?;
    let guard = UnitExpressionId::new(source_unit, source.guard);
    let reader_expression = UnitExpressionId::new(source_unit, source.reader_comparison);
    let comparison = typed
        .null_comparison(guard)
        .ok_or("unit guard descriptor absent")?;
    let reader_comparison = typed
        .null_comparison(reader_expression)
        .ok_or("unit reader descriptor absent")?;
    let root = comparison.symbol();
    let input = one(
        inspect_signature.parameters().iter(),
        "unit sole inspect parameter",
    )?
    .symbol()
    .ok_or("unit inspect parameter symbol absent")?;
    let reader_input = one(
        reader_signature.parameters().iter(),
        "unit sole reader parameter",
    )?
    .symbol()
    .ok_or("unit reader parameter symbol absent")?;
    for drop in owned.drops().iter().filter(|drop| match drop.target() {
        UnitDropTarget::Named(symbol) => [root, input, reader_input].contains(&symbol),
        _ => false,
    }) {
        println!("SPEC0332 PRESENCE DROP case={case} unit=true actual={drop:?}");
    }
    let nullable = comparison.nullable_type();
    for (signature, input, function) in [
        (inspect_signature, input, source.inspect),
        (reader_signature, reader_input, source.reader),
    ] {
        let parameter = one(signature.parameters().iter(), "unit sole Value parameter")?;
        require(
            parameter.symbol() == Some(input)
                && input.source_unit() == source_unit
                && input.symbol()
                    == local_symbol(function.parameter()?, SymbolKind::ValueParameter)?
                && parameter.mode() == ParameterMode::Value
                && parameter.ty() == nullable
                && typed.symbol_type(input) == Some(nullable)
                && owned.binding_kind(input) == Some(OwnershipBindingKind::Owned)
                && signature.receiver().is_none()
                && signature.result_source() == CallableResultSource::Owned,
            "unit exact callable owns its sole source-qualified nullable Value parameter",
        )?;
    }
    require(
        inspect_signature.return_type() == nullable
            && typed.types().get(reader_signature.return_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean)),
        "unit exact function result contracts",
    )?;
    require(
        comparison.non_null_when_true()
            && root.source_unit() == source_unit
            && symbol_reference(source.guard_name)? == root
            && symbol_reference(source.tail_name)? == root
            && reader_comparison.expression() == reader_expression
            && !reader_comparison.non_null_when_true()
            && reader_comparison.symbol() == reader_input
            && reader_comparison.nullable_type() == nullable
            && symbol_reference(source.reader_name)? == reader_input
            && typed.expression_type(reader_expression) == Some(reader_signature.return_type()),
        "unit guard and Boolean return reference identities",
    )?;
    let origin = if let Some(local) = source.local {
        require(
            local_symbol(local.name, SymbolKind::Variable)? == root.symbol()
                && symbol_reference(name(file, local.initializer)?)? == input
                && symbol_reference(source.assignment_name.ok_or("assignment name absent")?)?
                    == root
                && typed.symbol_type(root) == Some(nullable)
                && owned.formed_owned_locals().contains(&root)
                && named_drops(input).next().is_none(),
            "unit local initializes from sole input and is the assigned/returned root",
        )?;
        local.name
    } else {
        require(root == input, "unit guard/tail return use sole parameter")?;
        source.inspect.parameter()?
    };
    let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(nullable) else {
        return Err("unit actual nullable type absent".into());
    };
    let Some(UnitTypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::Rc,
        arguments,
    }) = typed.types().get(*inner)
    else {
        return Err("unit nullable inner is not intrinsic Rc".into());
    };
    let payload = one(arguments.iter().copied(), "unit intrinsic Rc argument")?;
    require(
        typed.types().get(payload) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            && typed.copyability(nullable) == Copyability::MoveOnly,
        "unit actual MoveOnly Rc<Int>?",
    )?;
    let construction_id = UnitExpressionId::new(source_unit, source.construction.id);
    let construction = typed
        .construction(construction_id)
        .ok_or("unit Rc construction descriptor absent")?;
    require(
        construction.target() == UnitConstructionTarget::IntrinsicRc
            && construction.result_type() == *inner,
        "unit entry actual intrinsic Rc<Int> construction",
    )?;
    let mut entry_inputs = Vec::new();
    for (call, variable) in source.calls.iter().zip([source.present, source.absent]) {
        let input = symbol_reference(call.input_name)?;
        entry_inputs.push(input);
        require(
            input.source_unit() == source_unit
                && input.symbol() == local_symbol(variable.name, SymbolKind::Variable)?
                && typed.symbol_type(input) == Some(nullable),
            "unit entry input is the actual present/absent binding",
        )?;
        for (call, target, result) in [
            (call.inspect, inspect, nullable),
            (call.reader, reader, reader_signature.return_type()),
        ] {
            callable_reference(call.callee, target)?;
            let call_id = UnitExpressionId::new(source_unit, call.id);
            let descriptor = typed
                .call(call_id)
                .ok_or("unit direct call descriptor absent")?;
            let argument = one(descriptor.arguments().iter(), "unit direct call argument")?;
            require(
                descriptor.target() == UnitCallTarget::Declaration(target)
                    && descriptor.return_type() == result
                    && descriptor.result_source() == CallableResultSource::Owned
                    && argument.argument_index() == 0
                    && argument.parameter_index() == 0
                    && argument.mode() == ParameterMode::Value
                    && argument.parameter_type() == nullable,
                "unit source callee, call target and owned Value delivery identity",
            )?;
        }
    }
    let inspect_drops = named_drops(root);
    let inspect_drop_origin =
        inspect_drop(inspect_drops.map(|drop| drop.value_origin()), origin, case)?;
    let reader_drop = one(named_drops(reader_input), "unit unique reader Named drop")?;
    require(
        Some(reader_drop.value_origin()) == source.reader.parameter,
        "unit reader drop origin belongs to its own parameter",
    )?;
    require(
        entry_inputs
            .iter()
            .all(|input| named_drops(*input).next().is_none()),
        "unit entry bindings move into inspect without another Named drop",
    )?;
    Ok(finish(
        source,
        inspect_drop_origin,
        reader_drop.value_origin(),
    ))
}
