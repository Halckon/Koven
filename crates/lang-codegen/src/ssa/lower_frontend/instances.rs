//! 从 SPEC-0177 callable key 建立确定的具体泛型实例图。

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    name_resolution::SymbolId,
    parser::ParsedFile,
    source::Span,
    type_checking::{CallableTarget, TypeId, TypeKind, TypedFile},
};

use super::{LoweringError, LoweringErrorKind, error};

/// 防止合法但病态的源码让单次标量 lowering 无界扩张。
pub(super) const MAX_GENERIC_INSTANCES: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FunctionInstanceKey {
    symbol: SymbolId,
    type_arguments: Vec<TypeId>,
}

impl FunctionInstanceKey {
    pub(super) fn new(symbol: SymbolId, type_arguments: Vec<TypeId>) -> Self {
        Self {
            symbol,
            type_arguments,
        }
    }

    pub(super) const fn symbol(&self) -> SymbolId {
        self.symbol
    }

    pub(super) fn type_arguments(&self) -> &[TypeId] {
        &self.type_arguments
    }
}

pub(super) struct FunctionTemplate {
    pub(super) symbol: SymbolId,
    pub(super) type_parameters: Vec<SymbolId>,
    pub(super) span: Span,
}

pub(super) struct PlannedInstance {
    pub(super) key: FunctionInstanceKey,
    pub(super) template_index: usize,
    pub(super) substitutions: BTreeMap<SymbolId, TypeId>,
}

pub(super) fn plan_instances(
    parsed: &ParsedFile,
    typed: &TypedFile,
    templates: &[FunctionTemplate],
) -> Result<Vec<PlannedInstance>, LoweringError> {
    if let Some(window) = templates.windows(2).find(|window| {
        window[0].span.source_id() != window[1].span.source_id()
            || window[0].span.start() > window[1].span.start()
    }) {
        return Err(error(LoweringErrorKind::MissingFact, window[1].span));
    }
    let template_by_symbol = templates
        .iter()
        .enumerate()
        .map(|(index, template)| (template.symbol, index))
        .collect::<BTreeMap<_, _>>();
    let mut pending = templates
        .iter()
        .filter(|template| template.type_parameters.is_empty())
        .map(|template| FunctionInstanceKey::new(template.symbol, Vec::new()))
        .collect::<BTreeSet<_>>();
    let mut planned: BTreeMap<FunctionInstanceKey, PlannedInstance> = BTreeMap::new();
    let calls_by_template = index_calls(parsed, typed, templates)?;
    let mut generic_instance_count = 0;

    while let Some(key) = pending.iter().next().cloned() {
        pending.remove(&key);
        if planned.contains_key(&key) {
            continue;
        }
        let template_index = *template_by_symbol.get(&key.symbol()).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let template = &templates[template_index];
        if template.type_parameters.len() != key.type_arguments().len() {
            return Err(error(LoweringErrorKind::MissingFact, template.span));
        }
        if !key.type_arguments().is_empty()
            && generic_instance_budget_exhausted(generic_instance_count)
        {
            return Err(error(
                LoweringErrorKind::InstanceLimitExceeded,
                template.span,
            ));
        }
        let substitutions = template
            .type_parameters
            .iter()
            .copied()
            .zip(key.type_arguments().iter().copied())
            .collect::<BTreeMap<_, _>>();

        for (call_index, span) in &calls_by_template[template_index] {
            let call = &typed.calls()[*call_index];
            let CallableTarget::Source(target) = call.target() else {
                continue;
            };
            let Some(target_index) = template_by_symbol.get(&target).copied() else {
                continue;
            };
            let target_template = &templates[target_index];
            let arguments = call
                .instance()
                .type_arguments()
                .iter()
                .map(|ty| resolve_concrete_type(typed, *ty, &substitutions, *span))
                .collect::<Result<Vec<_>, _>>()?;
            if arguments.len() != target_template.type_parameters.len() {
                return Err(error(LoweringErrorKind::MissingFact, *span));
            }
            pending.insert(FunctionInstanceKey::new(target, arguments));
        }

        if !key.type_arguments().is_empty() {
            generic_instance_count += 1;
        }
        planned.insert(
            key.clone(),
            PlannedInstance {
                key,
                template_index,
                substitutions,
            },
        );
    }

    Ok(planned.into_values().collect())
}

fn index_calls(
    parsed: &ParsedFile,
    typed: &TypedFile,
    templates: &[FunctionTemplate],
) -> Result<Vec<Vec<(usize, Span)>>, LoweringError> {
    let mut calls = vec![Vec::new(); templates.len()];
    for (call_index, call) in typed.calls().iter().enumerate() {
        let span = parsed
            .ast()
            .expressions()
            .get(call.expression())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let upper = templates.partition_point(|template| template.span.start() <= span.start());
        let Some(template_index) = upper.checked_sub(1) else {
            continue;
        };
        if span_contains(templates[template_index].span, span) {
            calls[template_index].push((call_index, span));
        }
    }
    Ok(calls)
}

pub(super) fn resolve_concrete_type(
    typed: &TypedFile,
    ty: TypeId,
    substitutions: &BTreeMap<SymbolId, TypeId>,
    span: Span,
) -> Result<TypeId, LoweringError> {
    match typed.types().get(ty) {
        Some(TypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span)),
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(error(LoweringErrorKind::MissingFact, span)),
    }
}

fn contains_type_parameter(typed: &TypedFile, kind: &TypeKind) -> bool {
    let contains = |ty| {
        typed
            .types()
            .get(ty)
            .is_some_and(|kind| contains_type_parameter(typed, kind))
    };
    match kind {
        TypeKind::TypeParameter(_) => true,
        TypeKind::Nullable(inner) | TypeKind::StaticSelf(inner) => contains(*inner),
        TypeKind::Function {
            parameters,
            return_type,
            ..
        } => parameters.iter().any(|parameter| contains(parameter.ty)) || contains(*return_type),
        TypeKind::Nominal { arguments, .. } | TypeKind::Intrinsic { arguments, .. } => {
            arguments.iter().copied().any(contains)
        }
        TypeKind::EnumCase { root, .. } => contains(*root),
        TypeKind::Builtin(_)
        | TypeKind::Capability(_)
        | TypeKind::IntegerLiteral(_)
        | TypeKind::Error
        | TypeKind::Deferred(_) => false,
    }
}

fn span_contains(owner: Span, child: Span) -> bool {
    owner.source_id() == child.source_id()
        && owner.start() <= child.start()
        && child.end() <= owner.end()
}

fn generic_instance_budget_exhausted(count: usize) -> bool {
    count >= MAX_GENERIC_INSTANCES
}

#[cfg(test)]
mod tests {
    use super::{MAX_GENERIC_INSTANCES, generic_instance_budget_exhausted};

    #[test]
    fn generic_instance_budget_has_an_explicit_boundary() {
        assert!(!generic_instance_budget_exhausted(
            MAX_GENERIC_INSTANCES - 1
        ));
        assert!(generic_instance_budget_exhausted(MAX_GENERIC_INSTANCES));
    }
}
