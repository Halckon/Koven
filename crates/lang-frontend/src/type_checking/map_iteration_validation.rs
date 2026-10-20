//! 仅验证既有 Map 类型事实；required roots 由消费方真实 For 访问提供。
use super::*;
use crate::{
    ast::StatementId,
    name_resolution::{
        CompilationUnitNames, NameResolution, Namespace, SourceUnitInput, SymbolKind,
        ValidatedCompilationUnitNames,
    },
    parser::{ForBinding, NameMarker, ParsedFile, Statement},
    source::Span,
};
use std::collections::BTreeSet;

fn markers(binding: &ForBinding) -> Option<[Span; 2]> {
    let ForBinding::Destructuring { names, .. } = binding else {
        return None;
    };
    match names.as_slice() {
        [NameMarker::Present(key), NameMarker::Present(value)] if key != value => {
            Some([*key, *value])
        }
        _ => None,
    }
}
fn file_type_complete(typed: &TypedFile, root: TypeId) -> bool {
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty) {
            continue;
        }
        match typed.types().get(ty) {
            None | Some(TypeKind::Error | TypeKind::Deferred(_) | TypeKind::IntegerLiteral(_)) => {
                return false;
            }
            Some(
                TypeKind::Nullable(inner)
                | TypeKind::StaticSelf(inner)
                | TypeKind::EnumCase { root: inner, .. },
            ) => pending.push(*inner),
            Some(TypeKind::Nominal { arguments, .. } | TypeKind::Intrinsic { arguments, .. }) => {
                pending.extend(arguments);
            }
            Some(TypeKind::Function {
                parameters,
                return_type,
                ..
            }) => {
                pending.extend(parameters.iter().map(|parameter| parameter.ty));
                pending.push(*return_type);
            }
            _ => {}
        }
    }
    true
}
fn unit_type_complete(typed: &CompilationUnitTypes, root: UnitTypeId) -> bool {
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty) {
            continue;
        }
        match typed.types().get(ty) {
            None
            | Some(
                UnitTypeKind::Error | UnitTypeKind::Deferred(_) | UnitTypeKind::IntegerLiteral(_),
            ) => return false,
            Some(
                UnitTypeKind::Nullable(inner)
                | UnitTypeKind::StaticSelf(inner)
                | UnitTypeKind::EnumCase { root: inner, .. },
            ) => pending.push(*inner),
            Some(
                UnitTypeKind::Nominal { arguments, .. } | UnitTypeKind::Intrinsic { arguments, .. },
            ) => {
                pending.extend(arguments);
            }
            Some(UnitTypeKind::Function {
                parameters,
                return_type,
                ..
            }) => {
                pending.extend(parameters.iter().map(|parameter| parameter.ty()));
                pending.push(*return_type);
            }
            _ => {}
        }
    }
    true
}

/// 实际 For 消费点的双向合同：真实 Map 必有计划，非 Map 不可带 Map 计划。
pub(crate) fn validate_file_map_iteration(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    statement: StatementId,
) -> bool {
    let Ok(node) = parsed.ast().statements().get(statement) else {
        return false;
    };
    let Statement::For {
        source, binding, ..
    } = node.payload()
    else {
        return false;
    };
    let source_type = typed.expression_type(*source);
    let is_map = source_type
        .and_then(|ty| typed.types().get(ty))
        .is_some_and(|ty| {
            matches!(
                ty,
                TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Map
                        | IntrinsicTypeConstructor::MutableMap,
                    ..
                }
            )
        });
    let Some(plan) = typed.map_iteration(statement) else {
        return !is_map;
    };
    if !is_map
        || plan.source() != *source
        || Some(plan.source_type()) != source_type
        || typed.sequential_iteration(statement).is_some()
        || plan.key_binding() == plan.value_binding()
        || !file_type_complete(typed, plan.source_type())
    {
        return false;
    }
    if !matches!(typed.types().get(plan.source_type()),
        Some(TypeKind::Intrinsic { constructor, arguments })
        if *constructor == plan.kind().constructor() && arguments.as_slice() == [plan.key_type(), plan.value_type()])
    {
        return false;
    }
    let Some(spans) = markers(binding) else {
        return false;
    };
    for ((symbol, ty), span) in plan
        .bindings()
        .into_iter()
        .zip([plan.key_type(), plan.value_type()])
        .zip(spans)
    {
        if typed.symbol_type(symbol) != Some(ty)
            || names.symbols().get(symbol.index()).is_none_or(|binding| {
                binding.id() != symbol
                    || binding.kind() != SymbolKind::ForBinding
                    || binding.namespace() != Namespace::Value
                    || binding.span() != span
                    || binding.name() == "_"
                    || binding.is_synthetic()
            })
        {
            return false;
        }
    }
    true
}

/// 全表已有事实检查，不从裸 AST 增加 required roots。
pub(crate) fn file_map_iteration_rows_error(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
) -> Option<(StatementId, &'static str)> {
    for pair in typed.map_iterations().windows(2) {
        if pair[0].statement().index() >= pair[1].statement().index() {
            return Some((
                pair[1].statement(),
                "duplicate or unsorted Map typed descriptor",
            ));
        }
    }
    typed.map_iterations().iter().find_map(|plan| {
        (!validate_file_map_iteration(parsed, names, typed, plan.statement()))
            .then_some((plan.statement(), "invalid Map source/binding/type identity"))
    })
}

/// unit 实际 For 消费点的真实 AST/name/type 身份验证。
pub(crate) fn validate_unit_map_iteration(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    statement: UnitStatementId,
) -> bool {
    validate_unit_map_iteration_facts(
        parsed,
        names.names(),
        typed,
        statement,
        typed.map_iteration(statement),
        typed.sequential_iteration(statement),
    )
}

/// 两个入口仅选择同 owner 的实际表，AST/name/type 核验始终只有此实现。
pub(crate) fn validate_unit_map_iteration_facts(
    parsed: &ParsedFile,
    names: &CompilationUnitNames,
    typed: &CompilationUnitTypes,
    statement: UnitStatementId,
    plan: Option<&UnitMapIterationDescriptor>,
    sequential: Option<&UnitSequentialIterationDescriptor>,
) -> bool {
    let Some(source_names) = names
        .source_units()
        .iter()
        .find(|source| source.source_unit() == statement.source_unit())
    else {
        return false;
    };
    if source_names.resolution().source_id() != parsed.source_id() {
        return false;
    }
    let Ok(node) = parsed.ast().statements().get(statement.statement()) else {
        return false;
    };
    let Statement::For {
        source, binding, ..
    } = node.payload()
    else {
        return false;
    };
    let source = UnitExpressionId::new(statement.source_unit(), *source);
    let source_type = typed.expression_type(source);
    let is_map = source_type
        .and_then(|ty| typed.types().get(ty))
        .is_some_and(|ty| {
            matches!(
                ty,
                UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Map
                        | IntrinsicTypeConstructor::MutableMap,
                    ..
                }
            )
        });
    let Some(plan) = plan else {
        return !is_map;
    };
    if !is_map
        || plan.statement() != statement
        || plan.source() != source
        || Some(plan.source_type()) != source_type
        || !unit_map_iteration_row_valid_with_sequential(typed, plan, sequential)
    {
        return false;
    }
    let Some(spans) = markers(binding) else {
        return false;
    };
    for (symbol, span) in plan.bindings().into_iter().zip(spans) {
        if source_names
            .resolution()
            .symbols()
            .get(symbol.symbol().index())
            .is_none_or(|binding| {
                binding.id() != symbol.symbol()
                    || binding.kind() != SymbolKind::ForBinding
                    || binding.namespace() != Namespace::Value
                    || binding.span() != span
                    || binding.name() == "_"
                    || binding.is_synthetic()
            })
        {
            return false;
        }
    }
    true
}
fn unit_map_iteration_row_valid(
    typed: &CompilationUnitTypes,
    plan: &UnitMapIterationDescriptor,
) -> bool {
    unit_map_iteration_row_valid_with_sequential(
        typed,
        plan,
        typed.sequential_iteration(plan.statement()),
    )
}

fn unit_map_iteration_row_valid_with_sequential(
    typed: &CompilationUnitTypes,
    plan: &UnitMapIterationDescriptor,
    sequential: Option<&UnitSequentialIterationDescriptor>,
) -> bool {
    let source = plan.statement().source_unit();
    plan.source().source_unit() == source
        && plan
            .bindings()
            .into_iter()
            .all(|symbol| symbol.source_unit() == source)
        && plan.key_binding() != plan.value_binding()
        && sequential.is_none()
        && typed.expression_type(plan.source()) == Some(plan.source_type())
        && typed.symbol_type(plan.key_binding()) == Some(plan.key_type())
        && typed.symbol_type(plan.value_binding()) == Some(plan.value_type())
        && unit_type_complete(typed, plan.source_type())
        && matches!(typed.types().get(plan.source_type()),
            Some(UnitTypeKind::Intrinsic { constructor, arguments })
            if *constructor == plan.kind().constructor() && arguments.as_slice() == [plan.key_type(), plan.value_type()])
}
impl CompilationUnitTypes {
    pub(crate) fn map_iterations_are_valid(&self) -> bool {
        self.map_iterations()
            .windows(2)
            .all(|pair| pair[0].statement() < pair[1].statement())
            && self
                .map_iterations()
                .iter()
                .all(|plan| unit_map_iteration_row_valid(self, plan))
    }
}

/// complete 复用同一次 final walker 的实际 For 集合，返回实际失败身份。
pub(super) fn complete_file_map_iterations(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    visited: &[StatementId],
) -> Result<(), StatementId> {
    for pair in typed.map_iterations().windows(2) {
        if pair[0].statement().index() >= pair[1].statement().index() {
            return Err(pair[1].statement());
        }
    }
    for plan in typed.map_iterations() {
        if !validate_file_map_iteration(parsed, names, typed, plan.statement())
            || visited
                .binary_search_by_key(&plan.statement().index(), |statement| statement.index())
                .is_err()
        {
            return Err(plan.statement());
        }
    }
    for &statement in visited {
        if !validate_file_map_iteration(parsed, names, typed, statement) {
            return Err(statement);
        }
    }
    Ok(())
}
pub(super) fn complete_unit_map_iterations(
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    visited: &[Vec<StatementId>],
) -> Result<(), UnitStatementId> {
    for pair in typed.map_iterations().windows(2) {
        if pair[0].statement() >= pair[1].statement() {
            return Err(pair[1].statement());
        }
    }
    for plan in typed.map_iterations() {
        if !unit_map_iteration_row_valid(typed, plan) {
            return Err(plan.statement());
        }
        let Some((index, source)) = names
            .names()
            .index()
            .source_units()
            .iter()
            .enumerate()
            .find(|(_, source)| source.id() == plan.statement().source_unit())
        else {
            return Err(plan.statement());
        };
        let Some(parsed) = inputs
            .iter()
            .find(|input| input.source_id() == source.source_id())
            .map(|input| input.parsed())
        else {
            return Err(plan.statement());
        };
        if visited.get(index).is_none_or(|statements| {
            statements
                .binary_search_by_key(&plan.statement().statement().index(), |statement| {
                    statement.index()
                })
                .is_err()
        }) || !validate_unit_map_iteration(parsed, names, typed, plan.statement())
        {
            return Err(plan.statement());
        }
    }
    for (index, source) in names.names().index().source_units().iter().enumerate() {
        if let Some(statements) = visited.get(index) {
            for &statement in statements {
                let statement = UnitStatementId::new(source.id(), statement);
                let parsed = inputs
                    .iter()
                    .find(|input| input.source_id() == source.source_id())
                    .map(|input| input.parsed());
                if parsed.is_none_or(|parsed| {
                    !validate_unit_map_iteration(parsed, names, typed, statement)
                }) {
                    return Err(statement);
                }
            }
        }
    }
    Ok(())
}
