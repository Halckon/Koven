//! Normalize only the real single-source FE and ownership products for identity queries.
use super::*;
use lang_frontend::{
    name_resolution::{NameResolution, ReferenceTarget, SymbolId},
    ownership_checking::{DropPoint, DropTarget, OwnershipCheckedFile},
    type_checking::{ExpressionCategory, TypeKind, TypedFile},
};

fn reference(
    names: &NameResolution,
    parsed: &ParsedFile,
    expression: ExpressionId,
) -> Outcome<SymbolId> {
    let span = expression_span(parsed, transparent(parsed, expression)?)?;
    let roots: Vec<_> = names
        .references()
        .iter()
        .filter_map(|reference| {
            if reference.span() != span {
                return None;
            }
            match reference.target() {
                ReferenceTarget::Symbol(root) => Some(*root),
                _ => None,
            }
        })
        .collect();
    one(&roots, "unique source reference").copied()
}

pub(in crate::ssa::nullable_map_consume_lowering_tests) fn inspect(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    program: &Program,
    trace: &mut Trace,
) -> Outcome<()> {
    let descriptor = one(typed.map_sizes(), "single real size descriptor")?;
    let root = reference(names, parsed, descriptor.receiver())?;
    let symbols: Vec<_> = names
        .symbols()
        .iter()
        .filter(|symbol| symbol.id() == root)
        .collect();
    let symbol = *one(&symbols, "single size root")?;
    let storage = typed
        .symbol_type(root)
        .ok_or_else(|| missing("single root storage", root))?;
    let Some(TypeKind::Nullable(inner)) = typed.types().get(storage) else {
        return Err(missing("single nullable storage", storage));
    };
    gate(
        "read identity: single descriptor root and storage",
        symbol.span().source_id() == parsed.source_id() && *inner == descriptor.map_type(),
        (symbol, descriptor, storage),
    )?;
    let mut receiver = descriptor.receiver();
    loop {
        let fact = typed
            .non_null_use(receiver)
            .ok_or_else(|| missing("single current read fact", receiver))?;
        gate(
            "read identity: each real Name/Group read fact",
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
            .get(receiver)
            .map_err(|error| missing("single receiver node", error))?;
        match node.payload() {
            Expression::Group { expression } => receiver = *expression,
            Expression::Name => break,
            other => return Err(missing("single transparent receiver", other)),
        }
    }
    let comparisons: Vec<_> = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, _)| typed.null_comparison(id))
        .collect();
    let comparison = *one(&comparisons, "single real null comparison")?;
    gate(
        "read identity: comparison and read same root storage",
        comparison.symbol() == root && comparison.nullable_type() == storage,
        comparison,
    )?;
    let read = expression_span(parsed, descriptor.expression())?;
    let (control_id, control_span) = control(
        parsed,
        comparison.expression(),
        read,
        comparison.non_null_when_true(),
    )?;
    let (_, declaration_span, initializer) = declaration(parsed, symbol.span())?;
    let construction = one(
        typed.map_constructions(),
        "real root MapConstruction descriptor",
    )?;
    let int = typed
        .types()
        .builtin(lang_frontend::type_checking::BuiltinType::Int);
    gate(
        "read identity: real declaration construction and MutableMap Int pair",
        transparent(parsed, initializer)? == construction.expression()
            && construction.map_type() == *inner
            && construction.key_type() == int
            && construction.value_type() == int,
        construction,
    )?;
    let construction_span = expression_span(parsed, construction.expression())?;
    let root_drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == DropTarget::Named(root))
        .collect();
    let cleanup = if root_drops.is_empty() {
        let kept_symbols: Vec<_> = names
            .symbols()
            .iter()
            .filter(|candidate| {
                candidate.name() == "kept"
                    && candidate.span().source_id() == parsed.source_id()
                    && !candidate.is_synthetic()
            })
            .collect();
        let kept = *one(&kept_symbols, "real kept binding")?;
        let (statement, kept_statement, initializer) = declaration(parsed, kept.span())?;
        gate(
            "read identity: kept moves whole original nullable storage after If",
            reference(names, parsed, initializer)? == root
                && typed.symbol_type(kept.id()) == Some(storage)
                && typed.expression_type(initializer) == Some(storage)
                && typed.non_null_use(initializer).is_none()
                && kept_statement.start() > control_span.end(),
            (kept, initializer, storage),
        )?;
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == DropTarget::Named(kept.id()))
            .collect();
        let drop = *one(&drops, "real kept DropFact")?;
        gate(
            "read identity: kept actual AfterStatement cleanup",
            drop.point() == DropPoint::AfterStatement(statement)
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
            "read identity: original single CallReturn and null BranchExit facts",
            root_drops.len() == 2
                && root_drops
                    .iter()
                    .filter(|drop| drop.point() == DropPoint::CallReturn(descriptor.expression()))
                    .count()
                    == 1
                && root_drops
                    .iter()
                    .filter(|drop| {
                        drop.point()
                            == DropPoint::BranchExit {
                                control: control_id,
                                branch: null_branch,
                            }
                    })
                    .count()
                    == 1
                && root_drops
                    .iter()
                    .all(|drop| drop.value_origin() == symbol.span())
                && root_drops[0].owner().is_some()
                && root_drops[0].owner() == root_drops[1].owner(),
            &root_drops,
        )?;
        Cleanup::InBranches
    };
    trace.stages.push(format!("read identity real single input: source={:?}; root={root:?}; descriptor={descriptor:#?}; comparison={comparison:#?}; drops={:#?}", sources.source_name(parsed.source_id()), owned.drops()));
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
