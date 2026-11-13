//! Normalize source-qualified unit facts; never equate provider and consumer local IDs.
use super::*;
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
