//! Actual provider graph and Guard cleanup; no production proof helpers are consulted.
use super::*;
pub(super) struct Evidence<'a> {
    pub first: &'a Instruction,
    pub body: BlockId,
    pub body_owner: EntityId,
    pub normal_owner: EntityId,
    pub ends: Vec<&'a Instruction>,
    pub key: &'a Instruction,
    pub val: &'a Instruction,
    pub read: &'a Instruction,
    pub guard_drop: &'a Instruction,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn check<'a>(
    case: &Case,
    g: &Graph<'a>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    site: &Site,
    owners: &BTreeSet<EntityId>,
    types: (SsaTypeId, SsaTypeId),
    uses: &mut Uses,
    unit: bool,
) -> Result<Evidence<'a>, Failure> {
    let op = |p: fn(&Operation) -> bool| {
        one(
            g.instructions.iter().copied().filter(|i| p(&i.operation)),
            "actual unique provider operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let has = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    need(
        [first, has, key, val]
            .iter()
            .all(|i| i.origin == Origin::Source(site.for_span)),
        "actual For provider origins",
        site.for_span,
    )?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
    let birth = g.definition(EntityId::Loan(source))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = birth.operation
    else {
        return Err(failure("owned source lease birth", birth));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("source actual RootPlace", root));
    };
    need(
        owners.contains(&EntityId::Value(owner))
            && root.block == first.block
            && birth.block == first.block
            && g.position(root)? < g.position(birth)?
            && g.position(birth)? < g.position(first)?,
        "source borrows actual current factory owner",
        root,
    )?;
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), birth);
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = has.operation
    else {
        unreachable!()
    };
    let Operation::MapIterationKeyLoan {
        source: bs,
        cursor: bc,
    } = key.operation
    else {
        unreachable!()
    };
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("actual HasNext condition", has));
    };
    need(
        *condition == value(has)?
            && when_true.target == key.block
            && val.block == key.block
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: bs,
                    cursor: bc,
                },
        "actual guard and K/V provider operands",
        (has, key, val),
    )?;
    let entry = g.branch(first.block)?;
    need(
        entry.target == has.block
            && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "actual provider incoming columns",
        (entry, when_true),
    )?;
    let ho = g.transported(entry, EntityId::Value(owner))?;
    let bo = g.transported(when_true, ho)?;
    let xo = g.transported(when_false, ho)?;
    need(
        [ho, bo, xo].iter().all(|v| owners.contains(v)),
        "every current owner column retains immutable factory origin",
        (ho, bo, xo),
    )?;
    g.incoming_from(first.block, &[])?;
    let header_inputs = if case.exit < 2 {
        vec![first.block, key.block]
    } else {
        vec![first.block]
    };
    g.incoming_from(has.block, &header_inputs)?;
    g.incoming_from(key.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let body_span = parsed.ast().statements().get(site.body).unwrap().span();
    let transfer = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            (inside(n.span(), body_span)
                && matches!(
                    n.payload(),
                    Expression::Continue { .. }
                        | Expression::Break { .. }
                        | Expression::Return { .. }
                ))
            .then_some(n)
        })
        .collect::<Vec<_>>();
    need(
        transfer.len() == usize::from(case.exit != 0),
        "actual original body transfer count",
        &transfer,
    )?;
    let body_origin = if case.exit == 0 {
        site.for_span
    } else {
        transfer[0].span()
    };
    if case.exit != 0 {
        need(
            matches!(
                (case.exit, transfer[0].payload()),
                (1, Expression::Continue { .. })
                    | (2, Expression::Break { .. })
                    | (3, Expression::Return { .. })
            ),
            "actual transfer kind",
            transfer[0],
        )?;
    }
    // Unit cleanup is emitted by the original For action dispatcher; single
    // cleanup uses the original transfer expression. Next retains its transfer span.
    let cleanup_origin = if unit { site.for_span } else { body_origin };
    let mut normal_owner = xo;
    let nexts = g
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
        .collect::<Vec<_>>();
    need(
        nexts.len() == usize::from(case.exit < 2),
        "Next only for normal and Continue",
        &nexts,
    )?;
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let xc = g.transported(when_false, EntityId::Value(hc))?;
    let leases = family(g, EntityId::Loan(source))?;
    let ends=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if leases.contains(&EntityId::Loan(loan)))).collect::<Vec<_>>();
    need(
        ends.len() == if case.exit >= 2 { 2 } else { 1 },
        "one source End per actual terminal edge",
        &ends,
    )?;
    let exhausted = one(
        ends.iter().copied().filter(
            |i| matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==xs),
        ),
        "actual false-edge source End",
    )?;
    need(
        exhausted.block == when_false.target,
        "false edge ends only its current source",
        exhausted,
    )?;
    for &lease in &leases {
        uses.protect(lease);
    }
    uses.permit(EntityId::Loan(source), first);
    uses.permit(EntityId::Loan(hs), has);
    for component in [key, val] {
        uses.permit(EntityId::Loan(bs), component);
    }
    for end in &ends {
        if let Operation::BorrowEnd { loan } = end.operation {
            uses.permit(EntityId::Loan(loan), end);
        }
    }
    let mut cursors =
        BTreeSet::from([result(first)?, EntityId::Value(hc), EntityId::Value(bc), xc]);
    for &cursor in &cursors {
        uses.protect(cursor);
        need(g.function.entity(cursor).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if module.type_kind(t)==Some(&SsaTypeKind::Integer{bits:64,signed:false}))),"actual provider cursor storage",cursor)?;
    }
    uses.permit(EntityId::Value(hc), has);
    for component in [key, val] {
        uses.permit(EntityId::Value(bc), component);
    }
    if case.exit < 2 {
        let next = nexts[0];
        let back = g.branch(key.block)?;
        need(
            next.block == key.block
                && next.origin == Origin::Source(body_origin)
                && next.operation
                    == Operation::MapIterationNext {
                        source: bs,
                        cursor: bc,
                    }
                && back.target == has.block
                && g.argument(back, ho)? == bo
                && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.argument(back, EntityId::Value(hc))? == result(next)?,
            "normal/Continue exact all-role backedge",
            (next, back),
        )?;
        uses.protect(result(next)?);
        cursors.insert(result(next)?);
        uses.permit(EntityId::Loan(bs), next);
        uses.permit(EntityId::Value(bc), next);
    } else if case.exit == 2 {
        let merge = g.branch(key.block)?.target;
        need(
            g.branch(when_false.target)?.target == merge,
            "Break and exhaustion share continuation",
            merge,
        )?;
        g.incoming_from(merge, &[key.block, when_false.target])?;
        normal_owner = g.transported(g.branch(key.block)?, bo)?;
        need(
            g.argument(g.branch(when_false.target)?, normal_owner)? == xo,
            "Break merge column receives exact body and exhaustion current owners",
            (normal_owner, bo, xo),
        )?;
    }
    for (from, edge) in &g.edges {
        for entity in &edge.arguments {
            if leases.contains(entity) || cursors.contains(entity) {
                need(
                    (*from == first.block && edge.target == has.block)
                        || (*from == has.block
                            && [key.block, when_false.target].contains(&edge.target))
                        || (case.exit < 2 && *from == key.block && edge.target == has.block),
                    "provider state only crosses its exact live edges",
                    (from, edge, entity),
                )?;
            }
        }
    }
    let reader = function(module, declaration(parsed, sources, "read")?)?;
    let read=one(g.instructions.iter().copied().filter(|i|i.block==key.block && matches!(&i.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==reader.id && arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])),"actual selected read K/V arguments")?;
    let mut component_ends = Vec::new();
    for (index, (component, target)) in [(key, types.0), (val, types.1)].into_iter().enumerate() {
        let entity = result(component)?;
        let ty = EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        };
        need(
            g.function.entity(entity).unwrap().ty == ty
                && reader
                    .block(reader.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .get(index)
                    .and_then(|e| reader.entity(*e))
                    .is_some_and(|e| e.ty == ty),
            "reader uses exact Shared represented K/V ABI",
            component,
        )?;
        let end = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowEnd {
                        loan: loan(component).unwrap(),
                    }
            }),
            "unique actual component End",
        )?;
        need(end.block == key.block, "component ends in actual body", end)?;
        if case.exit != 0 {
            need(
                end.origin == Origin::Source(cleanup_origin),
                "component cleanup exact entry-specific source span",
                end,
            )?;
        }
        uses.protect(entity);
        uses.permit(entity, read);
        uses.permit(entity, end);
        component_ends.push(end);
    }
    need(
        g.position(read)? < g.position(component_ends[1])?
            && g.position(component_ends[1])? < g.position(component_ends[0])?,
        "actual read/VEnd/KEnd order",
        &component_ends,
    )?;
    let local = guard(
        g,
        module,
        sources,
        parsed,
        site.body,
        (read, component_ends[1]),
        uses,
    )?;
    if case.exit >= 2 {
        let end = one(
            ends.iter().copied().filter(|i| i.block == key.block),
            "actual body terminal EndSource",
        )?;
        need(
            g.position(component_ends[0])? < g.position(end)?
                && end.origin == Origin::Source(cleanup_origin),
            "KEnd before exact entry-specific terminal EndSource",
            end,
        )?;
    } else {
        need(
            g.position(component_ends[0])? < g.position(nexts[0])?,
            "KEnd before Next",
            nexts[0],
        )?;
    }
    let resource = match module.type_kind(types.1) {
        Some(SsaTypeKind::NullableHandle { inner }) => *inner,
        _ => types.1,
    };
    need(!g.instructions.iter().any(|i|matches!(i.operation,Operation::Drop{owner} if [types.1, resource].contains(&g.function.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()))),"provider never materializes/drops component owner",g.function.id)?;
    Ok(Evidence {
        first,
        body: key.block,
        body_owner: bo,
        normal_owner,
        ends,
        key,
        val,
        read,
        guard_drop: local,
    })
}
fn guard<'a>(
    g: &Graph<'a>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    body: StatementId,
    (read, value_end): (&Instruction, &Instruction),
    uses: &mut Uses,
) -> Result<&'a Instruction, Failure> {
    let span = parsed.ast().statements().get(body).unwrap().span();
    let (name, init) = one(
        parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if inside(n.span(), span) && sources.slice(*name) == Ok("local") => {
                    Some((*name, *initializer))
                }
                _ => None,
            }),
        "original local Guard",
    )?;
    let ast = parsed.ast().expressions().get(init).unwrap();
    need(
        sources.slice(ast.span()) == Ok("Guard()"),
        "actual Guard constructor AST",
        ast,
    )?;
    let allocated = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(ast.span())
                && matches!(i.operation, Operation::HeapAllocate { .. })
        }),
        "actual Guard allocation",
    )?;
    let Operation::HeapAllocate { owner: ty, payload } = allocated.operation else {
        unreachable!()
    };
    let class = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Classifier(c) = n.payload() else {
                return None;
            };
            matches!(c.name,NameMarker::Present(name) if sources.slice(name)==Ok("Guard"))
                .then_some(c)
        }),
        "original Guard class",
    )?;
    let deinit = one(
        class
            .body
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                matches!(n.payload(), Item::Deinit { .. }).then_some(n.span())
            }),
        "original Guard deinit",
    )?;
    need(
        module
            .deinits
            .get(&ty)
            .and_then(|id| module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(deinit)),
        "Guard actual type deinit",
        ty,
    )?;
    let payload = g.definition(EntityId::Value(payload))?;
    need(
        payload.block == read.block
            && payload.origin == Origin::Source(ast.span())
            && matches!(&payload.operation,Operation::AggregateConstruct{fields,..} if fields.is_empty()),
        "actual empty Guard payload",
        payload,
    )?;
    let owner = value(allocated)?;
    let dropped = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner }),
        "Guard unique body Drop",
    )?;
    need(
        allocated.block == read.block
            && dropped.block == read.block
            && dropped.origin == Origin::Source(name)
            && g.position(allocated)? < g.position(read)?
            && g.position(read)? < g.position(dropped)?
            && g.position(dropped)? < g.position(value_end)?,
        "Guard allocation/read/Drop/VEnd order",
        dropped,
    )?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), dropped);
    Ok(dropped)
}

pub(in crate::ssa) mod unit_20 {
//! Normalize source-qualified unit facts; never equate provider and consumer local IDs.
use crate::ssa::nullable_map_consume_lowering_tests::read_identity::*;
use crate::ssa::nullable_map_consume_lowering_tests::unit::Context;
use lang_frontend::{
    name_resolution::{
        SourceUnitId, SymbolKind, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, UnitCallArgumentOwnershipKind, UnitDropPoint, UnitDropTarget,
    },
    type_checking::{
        BuiltinType, CompilationUnitTypes, ExpressionCategory, ParameterMode, UnitCallTarget,
        UnitCallableTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn reference(
    names: &ValidatedCompilationUnitNames,
    parsed: &ParsedFile,
    source: SourceUnitId,
    expression: ExpressionId,
) -> Outcome<UnitSymbolId> {
    let span = expression_span(parsed, transparent(parsed, expression)?)?;
    let roots: Vec<_> = names
        .names()
        .references()
        .iter()
        .filter_map(|reference| {
            if reference.source_unit() != source || reference.span() != span {
                return None;
            }
            match reference.target() {
                UnitReferenceTarget::Symbol(root) => Some(*root),
                _ => None,
            }
        })
        .collect();
    one(&roots, "unique source-qualified unit reference").copied()
}

pub(in crate::ssa::nullable_map_consume_lowering_tests) fn inspect(
    context: &Context<'_>,
    files: &[(SourceUnitId, &ParsedFile)],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    program: &Program,
    trace: &mut Trace,
) -> Outcome<()> {
    let Context {
        case,
        layout,
        sources,
    } = *context;
    let descriptor = one(typed.map_sizes(), "unit real size descriptor")?;
    let source = descriptor.expression().source_unit();
    let matches: Vec<_> = files.iter().filter(|(id, _)| *id == source).collect();
    let (_, parsed) = **one(&matches, "real unit source file")?;
    let source_units: Vec<_> = names
        .names()
        .index()
        .source_units()
        .iter()
        .filter(|unit| unit.id() == source)
        .collect();
    let source_unit = *one(&source_units, "real provider source identity")?;
    gate(
        "read identity: actual provider source path",
        source_unit.source_id() == parsed.source_id()
            && layout.provider_matches(case, sources, names, source_unit)
            && descriptor.receiver().source_unit() == source,
        source_unit,
    )?;
    let root = reference(names, parsed, source, descriptor.receiver().expression())?;
    let resolutions: Vec<_> = names
        .names()
        .source_units()
        .iter()
        .filter(|resolution| resolution.source_unit() == source)
        .collect();
    let resolution = *one(&resolutions, "real unit resolution")?;
    let symbols: Vec<_> = resolution
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.id() == root.symbol())
        .collect();
    let symbol = *one(&symbols, "unit read root declaration")?;
    let storage = typed
        .symbol_type(root)
        .ok_or_else(|| missing("unit root storage", root))?;
    let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(storage) else {
        return Err(missing("unit nullable storage", storage));
    };
    gate(
        "read identity: unit descriptor root and storage",
        root.source_unit() == source
            && symbol.span().source_id() == parsed.source_id()
            && symbol.kind() == SymbolKind::Variable
            && !symbol.is_synthetic()
            && owned.formed_owned_locals().contains(&root)
            && *inner == descriptor.map_type(),
        (root, symbol, descriptor, storage),
    )?;
    let mut receiver = descriptor.receiver();
    loop {
        let fact = typed
            .non_null_use(receiver)
            .ok_or_else(|| missing("unit current read fact", receiver))?;
        gate(
            "read identity: each source-qualified Name/Group read fact",
            fact.expression() == receiver
                && fact.symbol() == root
                && fact.declared_type() == storage
                && fact.narrowed_type() == *inner
                && typed.expression_category(receiver) == Some(ExpressionCategory::Place),
            fact,
        )?;
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
            other => return Err(missing("unit transparent receiver", other)),
        }
    }
    let comparison = one(typed.null_comparisons(), "unit real null comparison")?;
    gate(
        "read identity: unit comparison and read same source root storage",
        comparison.expression().source_unit() == source
            && comparison.symbol() == root
            && comparison.nullable_type() == storage,
        comparison,
    )?;
    let read = expression_span(parsed, descriptor.expression().expression())?;
    let (control_id, control_span) = control(
        parsed,
        comparison.expression().expression(),
        read,
        comparison.non_null_when_true(),
    )?;
    let control_id = UnitExpressionId::new(source, control_id);
    let (_, declaration_span, initializer) = declaration(parsed, symbol.span())?;
    let construction = one(
        typed.map_constructions(),
        "real root MapConstruction descriptor",
    )?;
    let int = typed
        .types()
        .builtin(lang_frontend::type_checking::BuiltinType::Int)
        .ok_or_else(|| missing("real Int builtin", "absent"))?;
    gate(
        "read identity: real declaration construction and MutableMap Int pair",
        construction.expression().source_unit() == source
            && transparent(parsed, initializer)? == construction.expression().expression()
            && construction.map_type() == *inner
            && construction.key_type() == int
            && construction.value_type() == int,
        construction,
    )?;
    let construction_span = expression_span(parsed, construction.expression().expression())?;
    // Named(root) is the qualified binding owned by this actual callable, not a
    // type/count surrogate for the removed Legacy OwnerId. The unchanged SSA query
    // below follows this declaration's real construction/wrap through every incoming
    // owner column, both successor owners and exact cleanup/closed-use lineage.
    let callables: Vec<_> = parsed
        .roots()
        .iter()
        .filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            match node.payload() {
                Item::Function {
                    name: NameMarker::Present(name),
                    ..
                } if contains(node.span(), read)
                    && contains(node.span(), declaration_span)
                    && contains(node.span(), control_span) =>
                {
                    Some((*id, node.span(), *name))
                }
                _ => None,
            }
        })
        .collect();
    let (callable_root, callable_span, callable_name) =
        *one(&callables, "actual read/root/branch callable")?;
    let declarations: Vec<_> = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|value| {
            value.source_unit() == source
                && value.package() == source_unit.package()
                && value.root() == callable_root
                && value.name_span() == callable_name
                && value.kind() == SymbolKind::Function
        })
        .collect();
    let callable = *one(&declarations, "source-qualified read callable declaration")?;
    let signature = typed
        .signatures()
        .declaration(callable.id())
        .ok_or_else(|| missing("actual read callable signature", callable))?;
    gate(
        "read identity: qualified root belongs to the exact source callable",
        names.names().declaration_symbol(callable.id()) == Some(signature.symbol())
            && signature.symbol().source_unit() == source
            && signature.package() == source_unit.package()
            && signature.callable().is_some_and(|signature| {
                signature.target() == UnitCallableTarget::Declaration(callable.id())
                    && signature.name_span() == callable_name
                    && signature.receiver().is_none()
                    && signature.parameters().is_empty()
            })
            && declaration_span.end() < control_span.start()
            && owned.conditional_receiver_drops().is_empty(),
        (callable, signature, owned.conditional_receiver_drops()),
    )?;
    let functions: Vec<_> = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == Origin::Source(callable_span))
        .collect();
    let function = *one(&functions, "actual source callable SSA function")?;
    gate(
        "read identity: same callable contains the exact read and owner birth",
        function.instructions.iter().any(|instruction| {
            instruction.origin == Origin::Source(read)
                && matches!(instruction.operation, Operation::MapSize { .. })
        }) && function.instructions.iter().any(|instruction| {
            instruction.origin == Origin::Source(declaration_span)
                && matches!(instruction.operation, Operation::NullableWrap { .. })
        }),
        (callable_span, function.id),
    )?;
    let root_drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(root))
        .collect();
    let cleanup = if root_drops.is_empty() {
        let kept_symbols: Vec<_> = resolution
            .resolution()
            .symbols()
            .iter()
            .filter(|candidate| {
                candidate.name() == "kept"
                    && candidate.span().source_id() == parsed.source_id()
                    && !candidate.is_synthetic()
            })
            .collect();
        let kept = *one(&kept_symbols, "real unit kept declaration")?;
        let kept_roots: Vec<_> = typed
            .body_symbol_types()
            .keys()
            .copied()
            .filter(|id| id.source_unit() == source && id.symbol() == kept.id())
            .collect();
        let kept_root = *one(&kept_roots, "source-qualified kept root")?;
        let (statement, kept_statement, initializer) = declaration(parsed, kept.span())?;
        let initializer_id = UnitExpressionId::new(source, initializer);
        gate(
            "read identity: unit kept moves whole nullable storage after If",
            reference(names, parsed, source, initializer)? == root
                && owned.formed_owned_locals().contains(&kept_root)
                && contains(callable_span, kept_statement)
                && typed.symbol_type(kept_root) == Some(storage)
                && typed.expression_type(initializer_id) == Some(storage)
                && typed.non_null_use(initializer_id).is_none()
                && kept_statement.start() > control_span.end(),
            (kept_root, kept, initializer_id, storage),
        )?;
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(kept_root))
            .collect();
        let drop = *one(&drops, "real unit kept DropFact")?;
        gate(
            "read identity: kept actual source-qualified AfterStatement cleanup",
            drop.point() == UnitDropPoint::AfterStatement(UnitStatementId::new(source, statement))
                && drop.value_origin() == kept.span()
                && owned.drops().len() == 1,
            drop,
        )?;
        Cleanup::AfterJoin {
            kept: drop.value_origin(),
        }
    } else {
        let null_branch = usize::from(comparison.non_null_when_true());
        gate(
            "read identity: original unit CallReturn and null BranchExit facts",
            root_drops.len() == 2
                && root_drops
                    .iter()
                    .filter(|drop| {
                        drop.point() == UnitDropPoint::CallReturn(descriptor.expression())
                    })
                    .count()
                    == 1
                && root_drops
                    .iter()
                    .filter(|drop| {
                        drop.point()
                            == UnitDropPoint::BranchExit {
                                control: control_id,
                                branch: null_branch,
                            }
                    })
                    .count()
                    == 1
                && root_drops
                    .iter()
                    .all(|drop| drop.value_origin() == symbol.span()),
            &root_drops,
        )?;
        print_temporary(
            context,
            parsed,
            names,
            typed,
            owned,
            root,
            (callable_span, control_span),
        )?;
        Cleanup::InBranches
    };
    trace.stages.push(format!("read identity real unit input: source={source:?}; path={:?}; root={root:?}; descriptor={descriptor:#?}; comparison={comparison:#?}; drops={:#?}", sources.source_name(parsed.source_id()), owned.drops()));
    check(
        program,
        ReadFacts {
            root: symbol.span(),
            declaration: declaration_span,
            construction: construction_span,
            control: control_span,
            read,
            cleanup,
        },
        trace,
    )
}

// B1/B2 also borrow their original stdout String temporary after the If. Check
// its complete source-qualified identity, not a global count or an arbitrary Drop.
fn print_temporary(
    context: &Context<'_>,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    root: UnitSymbolId,
    (callable, control): (Span, Span),
) -> Outcome<()> {
    let expected_literal = match context.case.id {
        "B1" => "\"read done\"",
        "B2" => "\"else read done\"",
        other => return Err(missing("original branch-cleanup stdout fixture", other)),
    };
    let calls: Vec<_> = typed
        .calls()
        .iter()
        .filter(|call| call.prints_line())
        .collect();
    let call = *one(&calls, "one actual compiler-bound stdout call")?;
    let UnitCallTarget::External(external) = call.target() else {
        return Err(missing("actual stdout external target", call));
    };
    gate(
        "read identity: stdout call is in the original source",
        call.expression().source_unit() == root.source_unit(),
        call,
    )?;
    let node = parsed
        .ast()
        .expressions()
        .get(call.expression().expression())
        .map_err(|error| missing("actual stdout AST call", error))?;
    let Expression::Call {
        callee, arguments, ..
    } = node.payload()
    else {
        return Err(missing("actual stdout Call node", node));
    };
    let argument = one(arguments, "original stdout sole argument")?;
    let literal = parsed
        .ast()
        .expressions()
        .get(argument.value)
        .map_err(|error| missing("actual stdout String literal", error))?;
    let callee = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|error| missing("actual stdout callee", error))?;
    let references: Vec<_> = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == root.source_unit() && reference.span() == callee.span()
        })
        .collect();
    let reference = *one(&references, "actual stdout resolved callee")?;
    let argument_id = UnitExpressionId::new(root.source_unit(), argument.value);
    let string = typed
        .types()
        .builtin(BuiltinType::String)
        .ok_or_else(|| missing("actual stdout String type", "absent"))?;
    let mapping = one(call.arguments(), "actual stdout argument mapping")?;
    gate(
        "read identity: original post-If println borrows its exact String literal",
        contains(callable, node.span())
            && control.end() < node.span().start()
            && matches!(callee.payload(), Expression::Name)
            && context.sources.slice(callee.span()).ok() == Some("println")
            && matches!(reference.target(), UnitReferenceTarget::ExternalOverloadSet(candidates)
                if candidates.as_slice() == [external])
            && matches!(literal.payload(), Expression::String { .. })
            && context.sources.slice(literal.span()).ok() == Some(expected_literal)
            && argument.named_prefix.is_none()
            && argument.mode_marker.is_none()
            && argument.span == literal.span()
            && typed.expression_type(argument_id) == Some(string)
            && typed.expression_category(argument_id) == Some(ExpressionCategory::Temporary)
            && mapping.argument_index() == 0
            && mapping.parameter_index() == 0
            && mapping.mode() == ParameterMode::Borrow
            && mapping.parameter_type() == string
            && mapping.category() == ExpressionCategory::Temporary
            && !mapping.crosses_thread()
            && call.receiver().is_none()
            && !call.aborts()
            && Some(call.return_type()) == typed.types().builtin(BuiltinType::Unit),
        (node, literal, reference, call),
    )?;
    let contracts: Vec<_> = owned
        .call_argument_contracts_for(call.expression())
        .collect();
    let contract = *one(&contracts, "actual stdout ownership argument contract")?;
    gate(
        "read identity: stdout shared-loan contract keeps exact call and temporary",
        contract.call() == call.expression()
            && contract.argument() == argument_id
            && contract.parameter_index() == 0
            && contract.parameter_type() == string
            && contract.category() == ExpressionCategory::Temporary
            && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
            && !contract.crosses_thread()
            && contract.argument_span() == literal.span()
            && contract.call_span() == node.span()
            && contract.parameter_span().is_none()
            && contract.loan_begin_span() == literal.span(),
        contract,
    )?;
    let other_drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() != UnitDropTarget::Named(root))
        .collect();
    let drop = *one(
        &other_drops,
        "sole non-root Drop is the original stdout temporary",
    )?;
    gate(
        "read identity: exact stdout String temporary CallReturn cleanup",
        drop.point() == UnitDropPoint::CallReturn(call.expression())
            && drop.target() == UnitDropTarget::Temporary(argument_id)
            && drop.value_origin() == literal.span(),
        drop,
    )
}
}
