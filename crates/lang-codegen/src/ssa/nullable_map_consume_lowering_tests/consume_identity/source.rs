//! Source-origin boundaries: local construction/put chain, or real own-parameter
//! plus its two ordinary callers. SSA identities never cross function contexts.
use super::*;

pub(super) fn map_types(
    module: &Module,
    mutable: SsaTypeId,
    readonly: SsaTypeId,
    scalar: BuiltinType,
) -> Outcome<(SsaTypeId, SsaTypeId)> {
    let Some(SsaTypeKind::MapContainer {
        kind: MapContainerKind::MutableMap,
        key,
        value,
    }) = module.type_kind(mutable)
    else {
        return Err(missing("MutableMap storage", mutable));
    };
    gate(
        "consume identity: same concrete K/V on readonly result",
        module.type_kind(readonly)
            == Some(&SsaTypeKind::MapContainer {
                kind: MapContainerKind::Map,
                key: *key,
                value: *value,
            }),
        (mutable, readonly),
    )?;
    let expected = match scalar {
        BuiltinType::Int => SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        },
        BuiltinType::String => SsaTypeKind::StringOwner,
        other => return Err(missing("C scalar domain", other)),
    };
    gate(
        "consume identity: exact frontend scalar K/V storage",
        module.type_kind(*key) == Some(&expected) && module.type_kind(*value) == Some(&expected),
        (module.type_kind(*key), module.type_kind(*value)),
    )?;
    Ok((*key, *value))
}

fn function(program: &Program, id: FunctionId) -> Outcome<(&Module, &Function)> {
    let candidates: Vec<_> = program
        .modules
        .iter()
        .flat_map(|module| {
            module
                .functions
                .iter()
                .filter(move |function| function.id == id)
                .map(move |function| (module, function))
        })
        .collect();
    one(&candidates, "full FunctionId lookup").copied()
}

fn by_origin(program: &Program, origin: Span) -> Outcome<(&Module, &Function)> {
    let candidates: Vec<_> = program
        .modules
        .iter()
        .flat_map(|module| {
            module
                .functions
                .iter()
                .filter(move |function| function.origin == Origin::Source(origin))
                .map(move |function| (module, function))
        })
        .collect();
    one(&candidates, "source-qualified caller callable").copied()
}

fn exact_uses(
    function: &Function,
    values: &BTreeSet<ValueId>,
    allowed: &BTreeSet<InstructionId>,
) -> Outcome<()> {
    for instruction in &function.instructions {
        let uses: Vec<_> = instruction
            .operation
            .entities()
            .into_iter()
            .filter(|entity| matches!(entity, EntityId::Value(value) if values.contains(value)))
            .collect();
        if !uses.is_empty() {
            gate(
                "consume identity: only exact source transfer instructions",
                allowed.contains(&instruction.id) && uses.len() == 1,
                instruction,
            )?;
        }
    }
    for block in &function.blocks {
        let term = block
            .terminator
            .as_ref()
            .ok_or_else(|| missing("source transfer terminator", block.id))?;
        let outside_edges = match &term.kind {
            TerminatorKind::Branch(_) => Vec::new(),
            TerminatorKind::Conditional { condition, .. } => vec![EntityId::Value(*condition)],
            TerminatorKind::NullableBranch { owner, .. } => vec![EntityId::Value(*owner)],
            _ => term.kind.entities(),
        };
        gate(
            "consume identity: source never tested or returned after transfer",
            !outside_edges
                .iter()
                .any(|entity| matches!(entity, EntityId::Value(value) if values.contains(value))),
            term,
        )?;
        for edge in graph::edges(block)? {
            let target = graph::target(function, edge)?;
            let mut seen = BTreeSet::new();
            for (column, entity) in edge.arguments.iter().enumerate() {
                let EntityId::Value(value) = entity else {
                    continue;
                };
                if !values.contains(value) {
                    continue;
                }
                gate(
                    "consume identity: exact source transport once and inside closed chain",
                    seen.insert(*value)
                        && matches!(target.parameters.get(column), Some(EntityId::Value(parameter)) if values.contains(parameter)),
                    (edge, column),
                )?;
            }
        }
    }
    Ok(())
}

fn mutable_chain(
    function: &Function,
    mutable: SsaTypeId,
    last: ValueId,
    construction: Span,
    expected_puts: &[Span],
    wrap: &Instruction,
) -> Outcome<()> {
    let construct = at(function, construction, |op| {
        matches!(op, Operation::MapConstruct { .. })
    })?;
    gate(
        "consume identity: original construction exact type",
        construct.operation == Operation::MapConstruct { map_type: mutable },
        construct,
    )?;
    let birth = value(function, construct)?;
    let mut dependencies = BTreeMap::<ValueId, Vec<ValueId>>::new();
    let mut pending = vec![last];
    let mut puts = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if dependencies.contains_key(&current) {
            continue;
        }
        gate(
            "consume identity: finite construction lineage",
            dependencies.len() < 4096 && ty(function, current)? == mutable,
            current,
        )?;
        let inputs = if current == birth {
            Vec::new()
        } else {
            let data = function
                .entity(EntityId::Value(current))
                .ok_or_else(|| missing("construction lineage entity", current))?;
            match data.definition {
                Definition::BlockParameter { block, index } => {
                    graph::incoming(function, block, index)?
                        .into_iter()
                        .map(|(_, value)| value)
                        .collect()
                }
                Definition::InstructionResult {
                    instruction,
                    index: 0,
                } => {
                    let instruction = function
                        .instruction(instruction)
                        .ok_or_else(|| missing("put lineage instruction", instruction))?;
                    let Operation::MapPut { owner, .. } = instruction.operation else {
                        return Err(missing("foreign mutable source terminal", instruction));
                    };
                    gate(
                        "consume identity: exact source put owner/result",
                        expected_puts.contains(&instruction.origin.span())
                            && instruction.origin == Origin::Source(instruction.origin.span())
                            && value(function, instruction)? == current,
                        instruction,
                    )?;
                    puts.insert(instruction.id);
                    vec![owner]
                }
                _ => return Err(missing("unexpected mutable source definition", data)),
            }
        };
        pending.extend(inputs.iter().copied());
        dependencies.insert(current, inputs);
    }
    let mut closed = BTreeSet::from([birth]);
    loop {
        let previous = closed.len();
        for (&current, inputs) in &dependencies {
            if inputs.iter().any(|value| closed.contains(value)) {
                closed.insert(current);
            }
        }
        if previous == closed.len() {
            break;
        }
    }
    gate(
        "consume identity: construction SCC and complete actual put set",
        dependencies.keys().all(|value| closed.contains(value))
            && puts.len() == expected_puts.len()
            && expected_puts.iter().all(|origin| {
                puts.iter().any(|id| {
                    function
                        .instruction(*id)
                        .is_some_and(|instruction| instruction.origin == Origin::Source(*origin))
                })
            }),
        (&dependencies, &puts, expected_puts),
    )?;
    let aliases: BTreeSet<_> = dependencies.into_keys().collect();
    puts.insert(wrap.id);
    exact_uses(function, &aliases, &puts)
}

pub(super) struct SourceQuery<'a> {
    pub program: &'a Program,
    pub entry: FunctionId,
    pub module: &'a Module,
    pub consumer: &'a Function,
    pub header: &'a Block,
    pub owner: ValueId,
}

pub(super) fn check(query: SourceQuery<'_>, facts: &Facts, trace: &mut Trace) -> Outcome<()> {
    let SourceQuery {
        program,
        entry,
        module,
        consumer,
        header,
        owner,
    } = query;
    let nullable = ty(consumer, owner)?;
    let birth = match &facts.source {
        Source::Local { construction, puts } => {
            let wrap = at(consumer, facts.declaration, |op| {
                matches!(op, Operation::NullableWrap { .. })
            })?;
            let Operation::NullableWrap {
                nullable: actual,
                owner: mutable_owner,
            } = wrap.operation
            else {
                return Err(missing("source NullableWrap", wrap));
            };
            gate(
                "consume identity: source declaration registered wrapper",
                actual == nullable
                    && module.nullable_maps.get(&nullable) == Some(&ty(consumer, mutable_owner)?),
                wrap,
            )?;
            mutable_chain(
                consumer,
                ty(consumer, mutable_owner)?,
                mutable_owner,
                *construction,
                puts,
                wrap,
            )?;
            if entry != consumer.id {
                let (_, selected) = function(program, entry)?;
                let calls: Vec<_> = selected.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::DirectCall { callee, receiver: None, ref arguments } if callee == consumer.id && arguments.is_empty())).collect();
                let call = *one(&calls, "selected entry reaches actual consuming callable")?;
                gate(
                    "consume identity: real entry call source",
                    matches!(call.origin, Origin::Source(_)),
                    call,
                )?;
            }
            value(consumer, wrap)?
        }
        Source::Parameter {
            parameter,
            index,
            calls,
        } => {
            gate(
                "consume identity: input and local source belong to real consuming callable",
                contains(facts.callable, *parameter)
                    && contains(facts.callable, facts.root)
                    && contains(facts.callable, facts.declaration),
                facts,
            )?;
            let block = consumer
                .entry_block()
                .and_then(|id| consumer.block(id))
                .ok_or_else(|| missing("own input entry block", consumer.id))?;
            let Some(EntityId::Value(birth)) = block.parameters.get(*index).copied() else {
                return Err(missing("real own input parameter column", (block, index)));
            };
            gate(
                "consume identity: exact entry parameter definition/storage",
                consumer.entity(EntityId::Value(birth)).is_some_and(|data| {
                    data.definition
                        == Definition::BlockParameter {
                            block: block.id,
                            index: *index,
                        }
                }) && ty(consumer, birth)? == nullable,
                birth,
            )?;
            gate(
                "consume identity: present and absent caller observations",
                calls.len() == 2
                    && calls
                        .iter()
                        .filter(|call| matches!(call.initializer, CallerInit::Present { .. }))
                        .count()
                        == 1
                    && calls
                        .iter()
                        .filter(|call| matches!(call.initializer, CallerInit::Absent { .. }))
                        .count()
                        == 1,
                calls,
            )?;
            for call in calls {
                caller(program, entry, consumer.id, *index, call, facts.scalar)?;
            }
            birth
        }
    };
    let aliases = graph::lineage(consumer, owner, birth)?;
    graph::before_branch(consumer, &aliases, header.id, owner)?;
    trace.stages.push(format!("consume identity source lineage: consumer={:?}; exact birth={birth:?}; complete pre-branch aliases={aliases:?}", consumer.id));
    Ok(())
}

fn caller(
    program: &Program,
    entry: FunctionId,
    target: FunctionId,
    column: usize,
    facts: &CallSite,
    scalar: BuiltinType,
) -> Outcome<()> {
    let (module, function) = by_origin(program, facts.callable)?;
    gate(
        "consume identity: C3 caller is actual selected entry",
        function.id == entry && contains(facts.callable, facts.root),
        (function.id, entry, facts),
    )?;
    let call = at(function, facts.call, |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    let Operation::DirectCall {
        callee,
        receiver: None,
        arguments,
    } = &call.operation
    else {
        return Err(missing("real own argument call", call));
    };
    let Some(EntityId::Value(argument)) = arguments.get(column) else {
        return Err(missing("real own argument column", call));
    };
    gate(
        "consume identity: full callee FunctionId and actual argument mapping",
        *callee == target && arguments.len() == 1,
        call,
    )?;
    let nullable = ty(function, *argument)?;
    let mutable = *module
        .nullable_maps
        .get(&nullable)
        .ok_or_else(|| missing("caller local registered N/M", nullable))?;
    // Caller storage is checked inside its own module; never compare bare type IDs
    // with the callee's module-local table.
    let Some(SsaTypeKind::MapContainer {
        kind: MapContainerKind::MutableMap,
        key,
        value: value_type,
    }) = module.type_kind(mutable)
    else {
        return Err(missing("caller mutable storage", mutable));
    };
    let component = match scalar {
        BuiltinType::Int => SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        },
        BuiltinType::String => SsaTypeKind::StringOwner,
        _ => return Err(missing("caller scalar", scalar)),
    };
    gate(
        "consume identity: caller concrete pair and ownership",
        module.type_kind(*key) == Some(&component)
            && module.type_kind(*value_type) == Some(&component)
            && module.type_ownership(nullable) == Some(Ownership::MoveOnly),
        (nullable, mutable),
    )?;
    let birth = match facts.initializer {
        CallerInit::Present { construction } => {
            let wrap = at(function, facts.declaration, |op| {
                matches!(op, Operation::NullableWrap { .. })
            })?;
            let Operation::NullableWrap {
                nullable: actual,
                owner,
            } = wrap.operation
            else {
                return Err(missing("caller present wrap", wrap));
            };
            gate(
                "consume identity: caller present actual wrapper storage",
                actual == nullable,
                wrap,
            )?;
            mutable_chain(function, mutable, owner, construction, &[], wrap)?;
            value(function, wrap)?
        }
        CallerInit::Absent { null } => {
            let instruction = at(function, null, |op| {
                matches!(op, Operation::NullableNull { .. })
            })?;
            gate(
                "consume identity: caller actual null storage",
                instruction.operation == Operation::NullableNull { nullable },
                instruction,
            )?;
            value(function, instruction)?
        }
    };
    let aliases = graph::lineage(function, *argument, birth)?;
    exact_uses(function, &aliases, &BTreeSet::from([call.id]))?;
    // Every instruction-level use is counted as well as classified. In particular
    // two arguments or repeated calls cannot reuse a once-delivered caller owner.
    let uses = function
        .instructions
        .iter()
        .flat_map(|instruction| instruction.operation.entities())
        .filter(|entity| matches!(entity, EntityId::Value(value) if aliases.contains(value)))
        .count();
    gate(
        "consume identity: caller owner delivered exactly once without post-call Drop",
        uses == 1,
        (call, aliases, uses),
    )
}

pub(in crate::ssa) mod checks {
//! 真实工厂、返回 operand/caller 链与两个独立 For 的源码结构校验。
use crate::ssa::map_provider_exits_pipeline_tests::temporary_exits::ast::{
    Call, Exit, ExpressionId, Failure, Function, Guard, Input, Read, ReturnCheck, Site, Span,
    StatementId, contains, failure, function, need,
};
use lang_frontend::parser::{BinaryOperator, Expression, Statement, VariableKind};

pub(in crate::ssa) struct Caller {
    pub scans: Vec<Call>,
    pub return_checks: Vec<ReturnCheck>,
}

pub(in crate::ssa) fn scanner(f: &Input, scanner: &Function) -> Result<StatementId, Failure> {
    let [site, exhausted, tail] = f.elements(scanner.body)? else {
        return Err(failure(
            "temporary exits AST exact scanner body",
            f.elements(scanner.body)?,
        ));
    };
    need(
        scanner.parameters.is_empty(),
        "temporary exits AST zeroarg scanner",
        scanner,
    )?;
    f.marker(*exhausted, "\"exhausted\"")?;
    f.scalar_return(*tail, "0")?;
    Ok(*site)
}

pub(in crate::ssa) fn operand(f: &Input, operand: &Function) -> Result<Read, Failure> {
    let [marker, read, tail] = f.elements(operand.body)? else {
        return Err(failure(
            "temporary exits AST exact operand body",
            f.elements(operand.body)?,
        ));
    };
    f.marker(*marker, "\"operand\"")?;
    f.scalar_return(*tail, "7")?;
    let [key, value] = operand.parameters.as_slice() else {
        return Err(failure(
            "temporary exits AST two actual operand parameters",
            operand,
        ));
    };
    let call = f.call(f.statement_expression(*read)?, "read", &["key", "value"])?;
    let (_, tail_span) = f.expression(f.statement_expression(*tail)?)?;
    need(
        contains(operand.span, call.span) && call.span.end() <= tail_span.start(),
        "temporary exits AST real read before operand scalar return",
        (&call, tail_span),
    )?;
    Ok(Read {
        call,
        binding_markers: [*key, *value],
    })
}

pub(in crate::ssa) fn read_body(f: &Input, read: &Function) -> Result<(), Failure> {
    let [key, value] = f.elements(read.body)? else {
        return Err(failure(
            "temporary exits AST exact read body",
            f.elements(read.body)?,
        ));
    };
    f.call(f.statement_expression(*key)?, "println", &["key"])?;
    f.call(f.statement_expression(*value)?, "println", &["value"])?;
    Ok(())
}

pub(in crate::ssa) fn return_entry(f: &Input, entry: &Function) -> Result<Caller, Failure> {
    let [
        nonempty,
        result,
        result_check,
        empty,
        empty_result,
        empty_check,
    ] = f.elements(entry.body)?
    else {
        return Err(failure(
            "temporary exits AST six return caller statements",
            f.elements(entry.body)?,
        ));
    };
    f.marker(*nonempty, "\"nonempty\"")?;
    f.marker(*empty, "\"empty\"")?;
    let result = f.local(*result, "result")?;
    let empty_result = f.local(*empty_result, "emptyResult")?;
    let scans = vec![
        f.call(result.1, "scanNonempty", &[])?,
        f.call(empty_result.1, "scanEmpty", &[])?,
    ];
    let return_checks = vec![
        return_check(
            f,
            *result_check,
            result,
            "result",
            "7",
            ("\"returned-7\"", "\"unexpected-nonempty-result\""),
        )?,
        return_check(
            f,
            *empty_check,
            empty_result,
            "emptyResult",
            "0",
            ("\"returned-0\"", "\"unexpected-empty-result\""),
        )?,
    ];
    need(
        result.0 != empty_result.0
            && result.1 != empty_result.1
            && scans[0].expression != scans[1].expression
            && scans[0].span.end() <= empty_result.0.start()
            && scans.iter().all(|scan| contains(entry.span, scan.span)),
        "temporary exits AST distinct ordered caller result roots and calls",
        (&scans, &return_checks),
    )?;
    for (check, call) in return_checks.iter().zip(&scans) {
        let (_, reference_span) = f.expression(check.reference)?;
        need(
            check.initializer == call.expression
                && check.marker.end() <= call.span.start()
                && call.span.end() <= reference_span.start()
                && contains(entry.span, reference_span),
            "temporary exits AST caller initializer and actual compared reference chain",
            (check, call),
        )?;
        exact_calls(f, call.target, &[call.expression])?;
    }
    Ok(Caller {
        scans,
        return_checks,
    })
}

fn return_check(
    f: &Input,
    statement: StatementId,
    local: (Span, ExpressionId),
    name: &str,
    expected: &'static str,
    markers: (&str, &str),
) -> Result<ReturnCheck, Failure> {
    let (expression, _) = f.expression(f.statement_expression(statement)?)?;
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = expression
    else {
        return Err(failure(
            "temporary exits AST actual caller if/else",
            expression,
        ));
    };
    let (condition, _) = f.expression(*condition)?;
    let Expression::Binary {
        left,
        operator: BinaryOperator::Equal,
        right,
        ..
    } = condition
    else {
        return Err(failure(
            "temporary exits AST actual caller equality",
            condition,
        ));
    };
    f.name(*left, name)?;
    f.literal(*right, expected, false)?;
    control_marker(f, *then_branch, markers.0)?;
    control_marker(f, *else_branch, markers.1)?;
    Ok(ReturnCheck {
        reference: *left,
        marker: local.0,
        initializer: local.1,
        expected,
    })
}

fn control_marker(f: &Input, id: StatementId, expected: &str) -> Result<(), Failure> {
    let node = f
        .parsed
        .ast()
        .statements()
        .get(id)
        .map_err(|e| failure("temporary exits AST comparison control body", e))?;
    let Statement::ControlBody { elements } = node.payload() else {
        return Err(failure("temporary exits AST actual ControlBody", node));
    };
    let [statement] = elements.as_slice() else {
        return Err(failure(
            "temporary exits AST sole comparison marker",
            elements,
        ));
    };
    f.marker(*statement, expected)
}

pub(in crate::ssa) fn factory_bodies(
    f: &Input,
    functions: &[Function],
    factories: [&'static str; 2],
) -> Result<(), Failure> {
    for (index, name) in factories.into_iter().enumerate() {
        let factory = function(functions, name)?;
        let elements = f.elements(factory.body)?;
        let (marker, source, puts, result, tail) = match (f.map, index, elements) {
            (true, 0, [marker, source, first, second, result, tail]) => (
                *marker,
                *source,
                vec![*first, *second],
                Some(*result),
                *tail,
            ),
            (true, 1, [marker, source, result, tail]) => {
                (*marker, *source, Vec::new(), Some(*result), *tail)
            }
            (false, 0, [marker, source, first, second, tail]) => {
                (*marker, *source, vec![*first, *second], None, *tail)
            }
            (false, 1, [marker, source, tail]) => (*marker, *source, Vec::new(), None, *tail),
            _ => {
                return Err(failure(
                    "temporary exits AST exact source factory body",
                    (name, elements),
                ));
            }
        };
        let marker_text = match (f.map, index) {
            (true, 0) => "\"make-map-nonempty\"",
            (true, _) => "\"make-map-empty\"",
            (false, 0) => "\"make-mutable-nonempty\"",
            (false, _) => "\"make-mutable-empty\"",
        };
        f.marker(marker, marker_text)?;
        let (_, initializer) = f.local_kind(source, "source", VariableKind::Var)?;
        empty_constructor(f, initializer)?;
        for (put, expected) in puts
            .iter()
            .zip([["\"z\"", "\"first\""], ["\"a\"", "\"second\""]])
        {
            let call = f.statement_expression(*put)?;
            let (receiver, arguments) = member_call(f, call, "put", 2)?;
            f.name(receiver, "source")?;
            for (argument, literal) in arguments.iter().zip(expected) {
                let (receiver, _) = member_call(f, *argument, "clone", 0)?;
                f.literal(receiver, literal, true)?;
            }
        }
        if let Some(result) = result {
            let (_, initializer) = f.local(result, "result")?;
            let (receiver, _) = member_call(f, initializer, "consume", 0)?;
            f.name(receiver, "source")?;
            f.name(f.return_value(tail)?, "result")?;
        } else {
            f.name(f.return_value(tail)?, "source")?;
        }
    }
    Ok(())
}

fn empty_constructor(f: &Input, id: ExpressionId) -> Result<(), Failure> {
    let (expression, _) = f.expression(id)?;
    let Expression::Call {
        callee,
        type_arguments,
        arguments,
        ..
    } = expression
    else {
        return Err(failure(
            "temporary exits AST actual mutable constructor",
            expression,
        ));
    };
    f.name(*callee, "mutableMapOf")?;
    need(
        arguments.is_empty() && type_arguments.len() == 2,
        "temporary exits AST empty constructor arity",
        expression,
    )?;
    for argument in type_arguments {
        let ty = f
            .parsed
            .ast()
            .type_refs()
            .get(*argument)
            .map_err(|e| failure("temporary exits AST constructor type argument", e))?;
        need(
            f.sources.slice(ty.span()) == Ok("String"),
            "temporary exits AST real constructor String component",
            ty,
        )?;
    }
    Ok(())
}

fn member_call(
    f: &Input,
    id: ExpressionId,
    member_name: &str,
    arity: usize,
) -> Result<(ExpressionId, Vec<ExpressionId>), Failure> {
    let (expression, _) = f.expression(id)?;
    let Expression::Call {
        callee,
        type_arguments,
        arguments,
        ..
    } = expression
    else {
        return Err(failure(
            "temporary exits AST actual member Call",
            expression,
        ));
    };
    let (member, _) = f.expression(*callee)?;
    let Expression::Member {
        receiver,
        name_span,
        safe: false,
        ..
    } = member
    else {
        return Err(failure(
            "temporary exits AST actual non-safe member",
            member,
        ));
    };
    need(
        f.sources.slice(*name_span) == Ok(member_name)
            && type_arguments.is_empty()
            && arguments.len() == arity
            && arguments
                .iter()
                .all(|a| a.named_prefix.is_none() && a.mode_marker.is_none()),
        "temporary exits AST exact positional member call",
        (expression, member),
    )?;
    Ok((*receiver, arguments.iter().map(|a| a.value).collect()))
}

pub(in crate::ssa) fn sites(
    f: &Input,
    functions: &[Function],
    sites: &[Site; 2],
    guards: &[Guard; 2],
    operand: Option<&Read>,
) -> Result<(), Failure> {
    let all_for = f
        .parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Statement::For { .. }).then_some(id))
        .collect::<Vec<_>>();
    need(
        all_for.len() == 2
            && sites.iter().all(|site| all_for.contains(&site.statement))
            && sites[0].statement != sites[1].statement
            && sites[0].body != sites[1].body
            && sites[0].source != sites[1].source
            && sites[0].source_span != sites[1].source_span
            && sites[0].read != sites[1].read
            && sites[0].transfer != sites[1].transfer
            && sites[0].span.end() <= sites[1].span.start()
            && sites[0]
                .bindings
                .iter()
                .all(|span| !sites[1].bindings.contains(span)),
        "temporary exits AST two independent ordered Call-source For sites",
        (&all_for, sites),
    )?;
    need(
        guards[0].marker != guards[1].marker
            && guards[0].initializer != guards[1].initializer
            && guards[0].span != guards[1].span
            && guards[0].class_item == guards[1].class_item
            && guards[0].class_marker == guards[1].class_marker
            && guards[0].deinit_item == guards[1].deinit_item
            && guards[0].deinit_span == guards[1].deinit_span
            && guards[0].deinit_body == guards[1].deinit_body,
        "temporary exits AST distinct body locals share exactly one Guard/deinit",
        guards,
    )?;
    for (index, (site, guard)) in sites.iter().zip(guards).enumerate() {
        let owner_name = match (f.exit, index) {
            (Exit::Return, 0) => "scanNonempty",
            (Exit::Return, _) => "scanEmpty",
            _ => "entry",
        };
        let owner = function(functions, owner_name)?;
        let (_, transfer_span) = f.expression(site.transfer)?;
        need(
            contains(owner.span, site.span)
                && contains(site.span, site.source_span)
                && contains(site.span, guard.marker)
                && contains(site.span, guard.span)
                && contains(site.span, site.read_span)
                && contains(site.span, transfer_span)
                && site.source_span.end() <= guard.marker.start()
                && guard.marker.end() <= guard.span.start()
                && guard.span.end() <= site.read_span.start()
                && if f.exit == Exit::Return {
                    contains(transfer_span, site.read_span) && site.consumer == "operand"
                } else {
                    site.read_span.end() <= transfer_span.start() && site.consumer == "read"
                },
            "temporary exits AST actual function, source, local, consumer and transfer order",
            (owner, site, guard, transfer_span),
        )?;
        exact_calls(f, site.factory, &[site.source])?;
    }
    exact_calls(f, "Guard", &[guards[0].initializer, guards[1].initializer])?;
    match (f.exit, operand) {
        (Exit::Continue | Exit::Break, None) => {
            exact_calls(f, "read", &[sites[0].read, sites[1].read])?;
            exact_calls(f, "operand", &[])?;
        }
        (Exit::Return, Some(read)) => {
            exact_calls(f, "operand", &[sites[0].read, sites[1].read])?;
            exact_calls(f, "read", &[read.call.expression])?;
            need(
                sites.iter().all(|site| {
                    !contains(site.span, read.call.span)
                        && site
                            .bindings
                            .iter()
                            .all(|span| !read.binding_markers.contains(span))
                }) && read.binding_markers[0] != read.binding_markers[1],
                "temporary exits AST operand read roots are real independent parameters",
                (sites, read),
            )?;
        }
        _ => {
            return Err(failure(
                "temporary exits AST exact exit/operand shape",
                (f.exit, operand),
            ));
        }
    }
    Ok(())
}

fn exact_calls(f: &Input, target: &str, expected: &[ExpressionId]) -> Result<(), Failure> {
    let mut found = Vec::new();
    for (id, node) in f.parsed.ast().expressions().iter() {
        let Expression::Call { callee, .. } = node.payload() else {
            continue;
        };
        let (callee, span) = f.expression(*callee)?;
        if matches!(callee, Expression::Name) && f.sources.slice(span) == Ok(target) {
            found.push(id);
        }
    }
    need(
        found.len() == expected.len() && expected.iter().all(|id| found.contains(id)),
        "temporary exits AST all actual direct calls accounted",
        (target, found, expected),
    )
}
}
