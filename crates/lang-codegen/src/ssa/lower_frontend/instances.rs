//! 从 SPEC-0177 callable key 建立确定的具体泛型实例图。

use std::{cmp::Ordering, collections::BTreeMap};

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    ownership_checking::OwnershipCheckedFile,
    parser::ParsedFile,
    source::Span,
    type_checking::{FunctionParameterType, IntrinsicTypeConstructor, TypeId, TypeKind, TypedFile},
};

use super::{LoweringError, LoweringErrorKind, error};
use crate::ssa::lowering_support::callable_instances::{
    CallableArena, CallableKey, CallableToken, SourceIdentity, SourceToken,
};

mod callable_planner;

/// 防止合法但病态的源码让单次标量 lowering 无界扩张。
pub(super) const MAX_GENERIC_INSTANCES: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FunctionInstanceKey {
    symbol: SymbolId,
    type_arguments: Vec<TypeId>,
    callable_arguments: Vec<(usize, CallableToken)>,
}

impl FunctionInstanceKey {
    pub(super) fn new(symbol: SymbolId, type_arguments: Vec<TypeId>) -> Self {
        Self {
            symbol,
            type_arguments,
            callable_arguments: Vec::new(),
        }
    }

    pub(super) const fn symbol(&self) -> SymbolId {
        self.symbol
    }

    pub(super) fn type_arguments(&self) -> &[TypeId] {
        &self.type_arguments
    }

    pub(super) fn callable_arguments(&self) -> &[(usize, CallableToken)] {
        &self.callable_arguments
    }

    fn is_specialized(&self) -> bool {
        !self.type_arguments.is_empty() || !self.callable_arguments.is_empty()
    }
}

impl SourceIdentity for FunctionInstanceKey {
    fn callable_arguments(&self) -> &[(usize, CallableToken)] {
        self.callable_arguments()
    }
}

/// AST IDs compare by index within this planner's already validated single-file chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OrderedExpressionId(ExpressionId);

impl OrderedExpressionId {
    pub(super) const fn expression(self) -> ExpressionId {
        self.0
    }
}

impl Ord for OrderedExpressionId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.index().cmp(&other.0.index())
    }
}

impl PartialOrd for OrderedExpressionId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(super) struct FunctionTemplate {
    pub(super) symbol: SymbolId,
    pub(super) type_parameters: Vec<SymbolId>,
    pub(super) span: Span,
}

pub(super) struct PlannedInstance {
    pub(super) source: SourceToken,
    pub(super) key: FunctionInstanceKey,
    pub(super) template_index: usize,
    pub(super) substitutions: BTreeMap<SymbolId, TypeId>,
}

/// Frozen source routes retain complete callback slots independently of unfinished closure ABI.
#[derive(Default)]
pub(super) struct FunctionInstancePlan {
    instances: Vec<PlannedInstance>,
    arena: CallableArena<FunctionInstanceKey, OrderedExpressionId>,
    routes: BTreeMap<(SourceToken, OrderedExpressionId), FunctionInstanceKey>,
    pointer_returns: BTreeMap<SourceToken, Option<CallableToken>>,
}

impl FunctionInstancePlan {
    pub(super) fn instances(&self) -> &[PlannedInstance] {
        &self.instances
    }

    pub(super) fn source(&self, source: SourceToken) -> Option<&FunctionInstanceKey> {
        self.arena.source(source)
    }

    pub(super) fn callable(
        &self,
        callable: CallableToken,
    ) -> Option<&CallableKey<OrderedExpressionId>> {
        self.arena.callable(callable)
    }

    pub(super) fn call_site(
        &self,
        caller: SourceToken,
        expression: ExpressionId,
    ) -> Option<&FunctionInstanceKey> {
        self.routes.get(&(caller, OrderedExpressionId(expression)))
    }

    pub(super) fn pointer_return(&self, source: SourceToken) -> Option<CallableToken> {
        self.pointer_returns.get(&source).copied().flatten()
    }
}

/// The private caller must validate Names -> Typed -> Owned analysis identity first.
/// Orchestrate's validate_inputs is authoritative; matching source IDs alone is not a witness.
pub(super) fn plan_instances(
    parsed: &ParsedFile,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    templates: &[FunctionTemplate],
    deinit_spans: &[Span],
) -> Result<FunctionInstancePlan, LoweringError> {
    callable_planner::plan(parsed, typed, owned, templates, deinit_spans)
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
        Some(TypeKind::Function {
            move_only,
            parameters,
            return_type,
        }) => {
            let parameters = parameters
                .iter()
                .map(|parameter| {
                    Ok(FunctionParameterType {
                        mode: parameter.mode,
                        ty: resolve_concrete_type(typed, parameter.ty, substitutions, span)?,
                    })
                })
                .collect::<Result<Vec<_>, LoweringError>>()?;
            let return_type = resolve_concrete_type(typed, *return_type, substitutions, span)?;
            typed
                .types()
                .find(&TypeKind::Function {
                    move_only: *move_only,
                    parameters,
                    return_type,
                })
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        }
        Some(TypeKind::Intrinsic {
            constructor,
            arguments,
        }) if matches!(
            constructor,
            IntrinsicTypeConstructor::View
                | IntrinsicTypeConstructor::Array
                | IntrinsicTypeConstructor::List
                | IntrinsicTypeConstructor::MutableList
        ) =>
        {
            let arguments = arguments
                .iter()
                .map(|argument| resolve_concrete_type(typed, *argument, substitutions, span))
                .collect::<Result<Vec<_>, _>>()?;
            typed
                .types()
                .find(&TypeKind::Intrinsic {
                    constructor: *constructor,
                    arguments,
                })
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        }
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

#[cfg(test)]
#[path = "instances/canonical_callable_tests.rs"]
mod canonical_callable_tests;

#[cfg(test)]
#[path = "instances/callable_plan_tests.rs"]
mod callable_plan_tests;
