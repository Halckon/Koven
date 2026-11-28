//! ADR-0032 ownership refinement layer: typed and ownership facts become a slot-based
//! CFG with explicit moves, drops and loans; SSA construction is generic.
//!
//! During migration `lower_owned_unit_with_entry` routes a function instance here only
//! when [`accepts`] proves its whole body lies inside the migrated subset. Once routed,
//! a failure is an error; there is no fallback to the per-family lowering.
use lang_frontend::ownership_checking::UnitLoweringFacts;
pub(crate) mod arm_loop_receipt;
mod body;
pub(in crate::ssa) mod build;
mod child_cycle_receipt;
mod control_bridge_receipt;
#[cfg(test)]
pub(in crate::ssa) mod initializer_probe;
mod nested_body_receipt;
mod nested_exit_receipt;
mod nested_resume_receipt;
#[cfg(test)]
pub(crate) use crate::ssa::lower_frontend_tests::boxed_signature_storage_tests::owner_consumer_test_support::{
    OwnerFault, observe_owner_actions, with_owner_fault,
};
pub(crate) mod bounded_receipt;
pub(crate) mod branch_receipt;
pub(crate) mod child_receipt;
mod display;
mod element_swap;
mod emit;
mod field_operand;
pub(crate) mod forward_receipt;
pub(crate) mod fresh_loop_receipt;
#[cfg(feature = "d10-catalog-experiment")]
pub(crate) mod identity_return;
mod inout_field;
pub(crate) mod loan_receipt;
pub(crate) mod loop_receipt;
mod map_exit;
#[deny(dead_code)]
pub(crate) mod multi_receipt;
#[deny(dead_code)]
pub(crate) mod nested_receipt;
mod nullable_when;
mod receiver_field_replace;
pub(in crate::ssa) use nullable_when::owned_string_presence_only;
pub(crate) mod receiver_receipt;
#[deny(dead_code)]
pub(crate) mod single_receipt;
mod subset;
pub(in crate::ssa) use subset::integer as is_integer_builtin;

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{DeclarationId, SourceUnitId, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{
        ConstEnabledOwnedUnit, LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
    },
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitTypes, Copyability, ExpressionCategory, NominalKind,
        ParameterMode, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        UnitCallTarget, UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

use super::{
    lowering_support::{LoweringError, callable_instances::SourceToken},
    model::{BlockId, EntityId, Function, FunctionId, SsaTypeId},
    unit_plan::{UnitCallablePlan, UnitFunctionInstanceKey},
};

#[derive(Clone, Debug)]
pub(in crate::ssa) enum RefineBody {
    Expression(ExpressionId),
    Block(StatementId),
    /// A lambda thunk body (ADR-0034 §1).
    Lambda(RefineThunk),
}

/// One capture slot in a lambda thunk's borrowed environment (ADR-0034 §1).
#[derive(Clone, Copy, Debug)]
pub(in crate::ssa) struct RefineThunkCapture {
    pub(in crate::ssa) symbol: UnitSymbolId,
    pub(in crate::ssa) ty: UnitTypeId,
    pub(in crate::ssa) storage: SsaTypeId,
    pub(in crate::ssa) mode: lang_frontend::ownership_checking::ClosureCaptureMode,
    pub(in crate::ssa) span: Span,
}

/// A lambda thunk instance lowered by the refinement layer (ADR-0034 §1).
#[derive(Clone, Debug)]
pub(in crate::ssa) struct RefineThunk {
    pub(in crate::ssa) expression: lang_frontend::type_checking::UnitExpressionId,
    pub(in crate::ssa) body: StatementId,
    pub(in crate::ssa) result: Option<ExpressionId>,
    /// The closure's `.environment` aggregate type when `!captures.is_empty()`.
    pub(in crate::ssa) environment: Option<SsaTypeId>,
    pub(in crate::ssa) captures: Vec<RefineThunkCapture>,
}

/// The current receiver of a method instance; its entity precedes the parameters.
#[derive(Clone, Copy, Debug)]
pub(in crate::ssa) struct RefineReceiver {
    /// The classifier whose member this is; implicit-`this` projections name it.
    pub(in crate::ssa) owner: DeclarationId,
    pub(in crate::ssa) ty: UnitTypeId,
    /// Value, Borrow (a shared loan) or Inout (an exclusive loan).
    pub(in crate::ssa) mode: ParameterMode,
}

/// One capture of a lambda of this function instance (ADR-0034).
#[derive(Clone, Copy, Debug)]
pub(in crate::ssa) struct RefineCapture {
    pub(in crate::ssa) symbol: UnitSymbolId,
    /// The captured binding's SSA type.
    pub(in crate::ssa) ty: SsaTypeId,
    pub(in crate::ssa) mode: lang_frontend::ownership_checking::ClosureCaptureMode,
    pub(in crate::ssa) effect: lang_frontend::ownership_checking::ClosureCaptureEffect,
}

/// A lambda of this function instance: its callable value type, thunk and captures.
#[derive(Clone, Debug)]
pub(in crate::ssa) struct RefineCallable {
    /// Function pointer (no captures) or concrete closure type.
    pub(in crate::ssa) callable: SsaTypeId,
    pub(in crate::ssa) thunk: FunctionId,
    pub(in crate::ssa) captures: Vec<RefineCapture>,
}

/// Source-local lexical while facts, built at most once when an iteration route needs them.
#[derive(Default)]
pub(in crate::ssa) struct WhileScopes(
    std::cell::OnceCell<Option<std::collections::BTreeMap<(usize, usize), bool>>>,
);

/// Everything one function or lambda thunk instance needs; all facts come from the same unit.
pub(in crate::ssa) struct RefineInputs<'a> {
    pub(in crate::ssa) sources: &'a SourceMap,
    pub(in crate::ssa) parsed: &'a ParsedFile,
    pub(in crate::ssa) while_scopes: &'a WhileScopes,
    pub(in crate::ssa) source_unit: SourceUnitId,
    pub(in crate::ssa) names: &'a ValidatedCompilationUnitNames,
    pub(in crate::ssa) typed: &'a CompilationUnitTypes,
    pub(in crate::ssa) owned: &'a dyn UnitLoweringFacts,
    pub(in crate::ssa) two_receiver_facts: Option<&'a TwoReceiverFacts>,
    pub(in crate::ssa) single_receiver_receipts: Option<&'a SingleReceiverReceiptIndex>,
    pub(in crate::ssa) subset_drop_index: Option<&'a SubsetDropIndex>,
    pub(in crate::ssa) single_receiver_facts: Option<&'a SingleReceiverFacts>,
    /// Present for the constant capability; const uses stay on the existing path.
    pub(in crate::ssa) constant_owned: Option<&'a ConstEnabledOwnedUnit>,
    pub(in crate::ssa) function_ids: &'a BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    pub(in crate::ssa) source_plan: &'a UnitCallablePlan,
    pub(in crate::ssa) source_token: SourceToken,
    pub(in crate::ssa) references: &'a BTreeMap<(usize, usize), UnitSymbolId>,
    /// `Some` for a function instance, `None` for a lambda thunk.
    pub(in crate::ssa) function_item: Option<ItemId>,
    pub(in crate::ssa) body: RefineBody,
    pub(in crate::ssa) return_type: UnitTypeId,
    pub(in crate::ssa) receiver: Option<RefineReceiver>,
    /// Parameters in declaration order.
    pub(in crate::ssa) parameters: Vec<UnitSymbolId>,
    /// Per parameter: Value, Borrow (a shared loan) or Inout (an exclusive loan).
    pub(in crate::ssa) modes: Vec<ParameterMode>,
    pub(in crate::ssa) span: Span,
    /// The instance's type-parameter substitutions; empty for a non-generic function.
    pub(in crate::ssa) substitutions: &'a BTreeMap<UnitSymbolId, UnitTypeId>,
    /// The concrete `Self` of a static-self instance.
    pub(in crate::ssa) static_self: Option<UnitTypeId>,
    /// This instance's lambdas, by lambda expression (ADR-0034).
    pub(in crate::ssa) callables:
        &'a BTreeMap<lang_frontend::type_checking::UnitExpressionId, RefineCallable>,
    /// Callable view targets of Borrow function types, by concrete function type.
    pub(in crate::ssa) views: &'a BTreeMap<UnitTypeId, SsaTypeId>,
    /// Concrete callable return types of planned functions that return a callable.
    pub(in crate::ssa) callable_returns: &'a BTreeMap<FunctionId, SsaTypeId>,
    /// Sealed known-function targets mapped to their `(FunctionId, FunctionPointer SsaTypeId)`.
    pub(in crate::ssa) known_functions:
        &'a BTreeMap<lang_frontend::type_checking::UnitCallableTarget, (FunctionId, SsaTypeId)>,
    /// Source parameter index when the function returns a shared borrow.
    pub(in crate::ssa) borrow_return: Option<usize>,
    /// Source parameter/receiver index when the function returns a `RangeView` carrier.
    pub(in crate::ssa) carrier_return: Option<usize>,
}

impl RefineInputs<'_> {
    fn direct_unit_argument(
        &self,
        call: &lang_frontend::type_checking::UnitCallDescriptor,
        argument: UnitExpressionId,
        parameter: UnitTypeId,
    ) -> bool {
        self.typed.types().get(parameter) == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            && self
                .typed
                .expression_type(argument)
                .and_then(|ty| self.concrete(ty))
                == Some(parameter)
            && matches!(
                call.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            )
            && self.owned.call_argument_completed(argument)
    }

    fn is_nullable_string(&self, ty: UnitTypeId) -> bool {
        crate::ssa::lowering_support::is_nullable_string(self.typed, ty)
    }

    /// A bottom right operand can exit after the nullable left operand has completed.
    fn is_nullable_string_equality(&self, left: UnitTypeId, right: UnitTypeId) -> bool {
        if crate::ssa::lowering_support::nullable_string_presence_on_left(self.typed, left, right)
            .is_some()
        {
            return true;
        }
        let types = self.typed.types();
        let left_nullable = self.is_nullable_string(left);
        let right_nullable = self.is_nullable_string(right);
        if left_nullable {
            right_nullable
                || matches!(
                    types.get(right),
                    Some(UnitTypeKind::Builtin(
                        BuiltinType::String | BuiltinType::Nothing
                    ))
                )
        } else {
            right_nullable
                && matches!(
                    types.get(left),
                    Some(UnitTypeKind::Builtin(BuiltinType::String))
                )
        }
    }

    /// A negated, direct Rc null comparison; logical combinations keep their own edge rules.
    fn negated_rc_null_test(&self, mut expression: ExpressionId) -> Option<(UnitSymbolId, bool)> {
        use lang_frontend::{parser::PrefixOperator, type_checking::IntrinsicTypeConstructor};

        let mut negated = false;
        let mut saw_not = false;
        loop {
            match self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .ok()?
                .payload()
            {
                Expression::Group { expression: inner } => expression = *inner,
                Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand,
                    ..
                } => {
                    expression = *operand;
                    saw_not = true;
                    negated = !negated;
                }
                _ => break,
            }
        }
        if !saw_not {
            return None;
        }
        let fact = self
            .typed
            .null_comparison(UnitExpressionId::new(self.source_unit, expression))?;
        let UnitTypeKind::Nullable(inner) = self
            .typed
            .types()
            .get(self.concrete(fact.nullable_type())?)?
        else {
            return None;
        };
        matches!(
            self.typed.types().get(*inner),
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Rc,
                ..
            })
        )
        .then_some((fact.symbol(), fact.non_null_when_true() ^ negated))
    }

    fn string_entry_source(&self) -> Option<UnitSymbolId> {
        let index = self.borrow_return?;
        let source = *self.parameters.get(index)?;
        (self.modes.get(index) == Some(&ParameterMode::Borrow)
            && self.carrier_return.is_none()
            && matches!(
                self.typed.types().get(self.return_type),
                Some(UnitTypeKind::Builtin(BuiltinType::String))
            )
            && self
                .typed
                .symbol_type(source)
                .and_then(|ty| self.concrete(ty))
                == Some(self.return_type))
        .then_some(source)
    }

    fn abandoned_borrow_result_call(&self, id: UnitExpressionId) -> bool {
        // Only lower the call prefix when both frontend phases and dispatch planning
        // agree that execution never reaches the callee.
        id.source_unit() == self.source_unit
            && self.carrier_return.is_none()
            && (self.borrow_return.is_none() || self.string_entry_source().is_some())
            && self.typed.expression_falls_through(id) == Some(false)
            && self.owned.call_may_dispatch(id) == Some(false)
            && self.typed.call(id).is_some_and(|descriptor| {
                descriptor.borrow_return().is_some()
                    && descriptor.receiver().is_none()
                    && matches!(
                        descriptor.target(),
                        UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
                    )
            })
            && self
                .source_plan
                .call_site(self.source_token, id)
                .is_some_and(|site| site.dispatch_key().is_none())
    }

    /// The lexical span of this function or lambda thunk.
    pub(in crate::ssa) fn scope_span(&self) -> Option<Span> {
        match self.function_item {
            Some(item) => self
                .parsed
                .ast()
                .items()
                .get(item)
                .ok()
                .map(|node| node.span()),
            None => Some(self.span),
        }
    }

    /// The concrete identity of a type read from the (template) typed facts; the identity
    /// itself for a non-generic function.
    pub(in crate::ssa) fn concrete(&self, ty: UnitTypeId) -> Option<UnitTypeId> {
        super::unit_plan::resolve_concrete_type(
            self.typed,
            ty,
            self.substitutions,
            self.static_self,
            self.span,
        )
        .ok()
    }

    /// A source-qualified use already narrowed by frontend flow after this control.
    fn borrowed_narrowing_used_after(&self, symbol: UnitSymbolId, control: Span) -> bool {
        self.narrowing_used_after(symbol, control, |kind| {
            matches!(
                kind,
                UnitTypeKind::Builtin(BuiltinType::String)
                    | UnitTypeKind::Intrinsic {
                        constructor: lang_frontend::type_checking::IntrinsicTypeConstructor::Rc,
                        ..
                    }
            )
        })
    }

    fn rc_narrowing_used_after(&self, symbol: UnitSymbolId, control: Span) -> bool {
        self.narrowing_used_after(symbol, control, |kind| {
            matches!(
                kind,
                UnitTypeKind::Intrinsic {
                    constructor: lang_frontend::type_checking::IntrinsicTypeConstructor::Rc,
                    ..
                }
            )
        })
    }

    fn narrowing_used_after(
        &self,
        symbol: UnitSymbolId,
        control: Span,
        accepts: impl Fn(&UnitTypeKind) -> bool,
    ) -> bool {
        self.typed.non_null_uses().iter().any(|fact| {
            let use_id = fact.expression();
            fact.symbol() == symbol
                && use_id.source_unit() == self.source_unit
                && self
                    .concrete(fact.narrowed_type())
                    .and_then(|inner| self.typed.types().get(inner))
                    .is_some_and(&accepts)
                && self
                    .parsed
                    .ast()
                    .expressions()
                    .get(use_id.expression())
                    .is_ok_and(|use_node| use_node.span().start() >= control.end())
        })
    }

    /// The active proof may be consumed only for the frontend's exact whole-root move.
    fn narrowed_string_delivery(
        &self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        parameter: UnitTypeId,
    ) -> Option<UnitSymbolId> {
        use lang_frontend::ownership_checking::{
            UnitCallArgumentOwnershipKind, UnitValueDeliveryKind,
        };

        if call.source_unit() != self.source_unit
            || argument.source_unit() != self.source_unit
            || self.typed.types().get(parameter)
                != Some(&UnitTypeKind::Builtin(BuiltinType::String))
            || self.typed.nullable_when_use(argument).is_some()
        {
            return None;
        }
        let expressions = self.parsed.ast().expressions();
        let span = expressions.get(argument.expression()).ok()?.span();
        let root =
            crate::ssa::borrow_result_support::ungroup(self.parsed, argument.expression(), span)
                .ok()?;
        let node = expressions.get(root).ok()?;
        if !matches!(node.payload(), Expression::Name) {
            return None;
        }
        let symbol = *self
            .references
            .get(&(node.span().start(), node.span().end()))?;
        let proof = self
            .typed
            .non_null_use(UnitExpressionId::new(self.source_unit, root))?;
        if proof.symbol() != symbol
            || self.concrete(proof.narrowed_type()) != Some(parameter)
            || self.typed.symbol_type(symbol) != Some(proof.declared_type())
            || self
                .concrete(proof.declared_type())
                .is_none_or(|ty| !self.is_nullable_string(ty))
        {
            return None;
        }
        let mut contracts = self
            .owned
            .call_argument_contracts_for(call)
            .filter(|contract| contract.argument() == argument);
        let contract = contracts.next()?;
        if contracts.next().is_some()
            || contract.kind() != UnitCallArgumentOwnershipKind::Value
            || contract.category() != ExpressionCategory::Place
            || self.concrete(contract.parameter_type()) != Some(parameter)
            || contract.crosses_thread()
        {
            return None;
        }
        let mut deliveries = self
            .owned
            .value_deliveries()
            .iter()
            .filter(|delivery| delivery.call() == call && delivery.argument() == argument);
        let delivery = deliveries.next()?;
        (deliveries.next().is_none()
            && delivery.kind() == UnitValueDeliveryKind::Move
            && delivery.span() == span
            && delivery.parameter_span() == contract.parameter_span()
            && delivery
                .place()
                .is_some_and(|place| place.is_root() && place.root() == symbol))
        .then_some(symbol)
    }

    fn string_borrow_adaptation(&self, argument: UnitExpressionId, parameter: UnitTypeId) -> bool {
        let Some(UnitTypeKind::Nullable(inner)) = self.typed.types().get(parameter) else {
            return false;
        };
        matches!(
            self.typed.types().get(*inner),
            Some(UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::String
            ))
        ) && self
            .typed
            .expression_type(argument)
            .and_then(|ty| self.concrete(ty))
            == Some(*inner)
    }

    /// The selected call lends either the named root or a borrow binding's proven origin.
    fn string_nullable_place_borrow(
        &self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        symbol: UnitSymbolId,
        parameter: UnitTypeId,
    ) -> bool {
        self.string_borrow_adaptation(argument, parameter)
            && self.typed.expression_category(argument) == Some(ExpressionCategory::Place)
            && self.owned.loans().iter().any(|loan| {
                loan.call() == call
                    && loan.argument() == argument
                    && loan.kind() == LoanKind::Shared
                    && (matches!(loan.target(), UnitLoanTarget::Place(place)
                        if place.is_root() && place.root() == symbol)
                        || self
                            .owned
                            .borrow_results()
                            .bindings()
                            .iter()
                            .any(|binding| {
                                binding.binding() == symbol && binding.origin() == loan.target()
                            }))
            })
    }

    /// Qualifies the shared field loan and the complete temporary that the call must clean up.
    fn temporary_field_argument(
        &self,
        call: UnitExpressionId,
        operand: ExpressionId,
        parameter: UnitTypeId,
    ) -> Option<(ExpressionId, bool)> {
        if call.source_unit() != self.source_unit {
            return None;
        }
        let nodes = self.parsed.ast().expressions();
        let argument = UnitExpressionId::new(self.source_unit, operand);
        let mut field = operand;
        let parent = 'projection: loop {
            while let Expression::Group { expression } = nodes.get(field).ok()?.payload() {
                field = *expression;
            }
            let id = UnitExpressionId::new(self.source_unit, field);
            let projection = self.typed.aggregate_projection(id)?;
            let UnitAggregateProjectionReceiver::Expression(mut receiver) = projection.receiver()
            else {
                return None;
            };
            if projection.expression() != id
                || projection.kind() != UnitAggregateProjectionKind::Field
                || receiver.source_unit() != self.source_unit
            {
                return None;
            }
            let receiver_ty = self.concrete(self.typed.expression_type(receiver)?)?;
            let UnitTypeKind::Nominal { declaration, .. } = self.typed.types().get(receiver_ty)?
            else {
                return None;
            };
            let kind = self
                .typed
                .signatures()
                .declaration(*declaration)?
                .nominal()?
                .kind();
            if !matches!(kind, NominalKind::Class | NominalKind::ValueClass) {
                return None;
            }
            loop {
                if self.typed.expression_category(receiver) == Some(ExpressionCategory::Temporary) {
                    break 'projection receiver;
                }
                let Expression::Group { expression } =
                    nodes.get(receiver.expression()).ok()?.payload()
                else {
                    break;
                };
                receiver = UnitExpressionId::new(self.source_unit, *expression);
            }
            field = receiver.expression();
        };
        let loan = self.owned.loans().iter().find(|loan| {
            loan.call() == call && loan.argument() == argument && loan.kind() == LoanKind::Shared
        })?;
        let UnitLoanTarget::Temporary(target) = loan.target() else {
            return None;
        };
        let actual = self.concrete(self.typed.expression_type(argument)?)?;
        let copied = actual != parameter;
        if copied
            && (*target != argument
                || self.typed.copyability(actual) != Copyability::Copyable
                || !matches!(self.typed.types().get(parameter), Some(UnitTypeKind::Nullable(inner)) if *inner == actual))
        {
            return None;
        }
        if !copied && *target != parent {
            return None;
        }
        // A completed prefix still evaluates the parent. An Abort path has no drop fact.
        if self.owned.call_may_dispatch(call) == Some(false)
            && self.owned.call_argument_completed(argument)
        {
            return Some((parent.expression(), copied));
        }
        let call_span = nodes.get(call.expression()).ok()?.span();
        let operand_span = nodes.get(operand).ok()?.span();
        self.owned
            .drops()
            .iter()
            .any(|drop| {
                drop.target() == UnitDropTarget::Temporary(parent)
                    && match drop.point() {
                        UnitDropPoint::CallReturn(id) => id == call,
                        UnitDropPoint::ControlTransfer(id) => {
                            id.source_unit() == self.source_unit
                                && nodes.get(id.expression()).is_ok_and(|node| {
                                    let span = node.span();
                                    operand_span.end() <= span.start()
                                        && span.end() <= call_span.end()
                                })
                        }
                        _ => false,
                    }
            })
            .then_some((parent.expression(), copied))
    }
}

/// Deterministic subset precheck; reads typed facts and AST construct kinds only.
pub(in crate::ssa) fn accepts(inputs: &RefineInputs<'_>) -> bool {
    subset::accepts(inputs)
}

pub(in crate::ssa) enum CallableRoute {
    Refine,
    Compatibility,
    #[cfg(test)]
    ForcedPerFamily,
}

/// Qualify the current callable before choosing a lowering path. The compatibility path
/// supports mixed bodies whose validated facts do not fit the complete refine subset.
pub(in crate::ssa) fn route(inputs: &RefineInputs<'_>) -> CallableRoute {
    if !accepts(inputs) {
        return CallableRoute::Compatibility;
    }
    #[cfg(test)]
    if PER_FAMILY_OVERRIDE.with(std::cell::Cell::get) {
        return CallableRoute::ForcedPerFamily;
    }
    CallableRoute::Refine
}

#[cfg(test)]
thread_local! {
    static PER_FAMILY_OVERRIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Test-only: pin the per-family path to observe or fault-inject its private guards.
#[cfg(test)]
pub(crate) fn with_per_family_lowering<T>(lower: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            PER_FAMILY_OVERRIDE.with(|state| state.set(self.0));
        }
    }
    let previous = PER_FAMILY_OVERRIDE.with(|state| state.replace(true));
    let _reset = Reset(previous);
    lower()
}

impl RefineInputs<'_> {
    /// No member operation follows a receiver whose verified evaluation never continues.
    fn non_completing_projection_receiver(
        &self,
        expression: UnitExpressionId,
    ) -> Option<ExpressionId> {
        let receiver = if let Some(rc) = self.typed.rc_operation(expression) {
            (rc.expression() == expression).then_some(rc.receiver())?
        } else if let Some(projection) = self.typed.aggregate_projection(expression) {
            let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver()
            else {
                return None;
            };
            (projection.expression() == expression).then_some(receiver)?
        } else {
            self.typed.string_operation(expression)?.receiver()
        };
        (receiver.source_unit() == self.source_unit
            && self.typed.expression_falls_through(receiver) == Some(false))
        .then_some(receiver.expression())
    }

    fn conditional_temporary_ids(&self) -> BTreeSet<UnitExpressionId> {
        let scope = self.scope_span().unwrap_or(self.span);
        self.owned
            .drops()
            .iter()
            .filter_map(|fact| {
                let UnitDropTarget::Temporary(id) = fact.target() else {
                    return None;
                };
                let origin = fact.value_origin();
                (fact.is_conditional_temporary()
                    && id.source_unit() == self.source_unit
                    && origin.source_id() == scope.source_id()
                    && origin.start() >= scope.start()
                    && origin.end() <= scope.end())
                .then_some(id)
            })
            .collect()
    }
}

/// Intern the source types and the SSA-only storage for conditionally formed Rc owners.
pub(in crate::ssa) fn prepare_types(
    inputs: &RefineInputs<'_>,
    types: &mut super::unit_lower::type_lower::UnitTypeLowering,
    module: &mut super::model::Module,
) -> Result<(), LoweringError> {
    for ty in subset::type_demands(inputs) {
        types.intern(module, inputs.typed, ty, inputs.span)?;
    }
    for id in inputs.conditional_temporary_ids() {
        // A nested callable's generic arguments may not be bound in the current instance.
        let Some(ty) = inputs
            .typed
            .expression_type(id)
            .and_then(|ty| inputs.concrete(ty))
        else {
            continue;
        };
        let owner = types.intern(module, inputs.typed, ty, inputs.span)?;
        module.add_nullable_handle_type(owner).map_err(|_| {
            super::lowering_support::error(
                super::lowering_support::LoweringErrorKind::InvalidModel,
                inputs.span,
            )
        })?;
    }
    Ok(())
}

/// The unit's interned type identities, filled in after [`prepare_types`].
pub(in crate::ssa) struct RefineTypes<'t> {
    pub(in crate::ssa) type_ids: &'t BTreeMap<UnitTypeId, SsaTypeId>,
    /// Class owner type to its payload aggregate.
    pub(in crate::ssa) heap_payloads: &'t BTreeMap<SsaTypeId, SsaTypeId>,
    /// (owner type, field symbol) to payload field index; enum roots index case payloads.
    pub(in crate::ssa) field_indices: &'t BTreeMap<(UnitTypeId, UnitSymbolId), usize>,
    /// (tagged union type, case value or type symbol) to (variant, payload aggregate).
    pub(in crate::ssa) enum_payloads: &'t BTreeMap<(SsaTypeId, UnitSymbolId), (usize, SsaTypeId)>,
    /// Registered `$map.result.tX` aggregate types mapped to their payload SSA types.
    pub(in crate::ssa) map_results: &'t BTreeMap<SsaTypeId, SsaTypeId>,
    /// Registered nullable map handle types mapped to their inner `MapContainer` SSA types.
    pub(in crate::ssa) nullable_maps: &'t BTreeMap<SsaTypeId, SsaTypeId>,
    /// Frozen module SSA type table.
    pub(in crate::ssa) ssa_types: &'t [super::model::SsaTypeKind],
}

/// Lowers an accepted function body into `function`, whose `entry` block holds
/// `parameters` (the receiver entity, if any, then one per `inputs.parameters` symbol).
pub(in crate::ssa) fn lower(
    inputs: &RefineInputs<'_>,
    function: &mut Function,
    entry: BlockId,
    parameters: &[EntityId],
    types: &RefineTypes<'_>,
) -> Result<(), LoweringError> {
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::identity_return::expect(inputs);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::forward_consumption::expect(inputs);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::binding_consumption::expect(inputs);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::loop_consumption::expect(inputs, types);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::fresh_loop_consumption::expect(inputs, types);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::arm_loop_consumption::expect(inputs, types);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::arm_loop_tail_consumption::expect(inputs, types);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::arm_loop_counter_consumption::expect(inputs, types);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::arm_loop_counter_break_consumption::expect(inputs, types);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::aux_binding_consumption::expect(inputs, types);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::distinct_aux_consumption::expect(inputs, types);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::later_aux_consumption::expect(inputs, types);
    #[cfg(feature = "d10-catalog-experiment")]
    crate::ssa::d10_project_audit::interleaved_aux_consumption::expect(inputs, types);
    let body = build::build(inputs, parameters, types)?;
    #[cfg(test)]
    two_receiver_plan_test_support::record(function.id(), &body);
    emit::emit(&body, function, entry, parameters, inputs.span)?;
    #[cfg(feature = "d10-catalog-experiment")]
    if let Some(receipt) = identity_return::certify(inputs, function, types) {
        crate::ssa::d10_project_audit::identity_return::record(receipt);
    }
    #[cfg(all(test, feature = "d10-catalog-experiment"))]
    identity_return::test_record(inputs, function, types);
    #[cfg(test)]
    ROUTED.with(|routed| {
        if let Some(routed) = routed.borrow_mut().as_mut() {
            routed.push(function.name.clone());
        }
    });
    Ok(())
}

#[cfg(test)]
thread_local! {
    static ROUTED: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

/// Test-only: runs `lower` and returns the names of the functions this layer lowered,
/// in lowering order.
#[cfg(test)]
pub(crate) fn observe_routing<T>(lower: impl FnOnce() -> T) -> (T, Vec<String>) {
    ROUTED.with(|routed| *routed.borrow_mut() = Some(Vec::new()));
    let result = lower();
    let names = ROUTED.with(|routed| routed.borrow_mut().take().unwrap_or_default());
    (result, names)
}

#[cfg(test)]
type OwnerDropWitness = (UnitDropTarget, usize, usize, Option<(usize, usize, usize)>);
#[cfg(test)]
thread_local! {
    static OWNER_DROPS: std::cell::RefCell<Option<Vec<OwnerDropWitness>>> = const { std::cell::RefCell::new(None) };
}

/// Observe only Drop operations already emitted by the production emitter.
#[cfg(test)]
pub(crate) fn observe_owner_drops<T>(lower: impl FnOnce() -> T) -> (T, Vec<OwnerDropWitness>) {
    OWNER_DROPS.with(|events| *events.borrow_mut() = Some(Vec::new()));
    let result = lower();
    let events = OWNER_DROPS.with(|events| events.borrow_mut().take().unwrap_or_default());
    (result, events)
}

#[cfg(test)]
fn record_owner_drop(
    target: UnitDropTarget,
    index: usize,
    slot: usize,
    receipt: Option<(usize, usize, usize)>,
) {
    OWNER_DROPS.with(|events| {
        if let Some(events) = events.borrow_mut().as_mut() {
            events.push((target, index, slot, receipt));
        }
    });
}

#[cfg(test)]
type OwnerCallWitness = (
    lang_frontend::type_checking::UnitExpressionId,
    crate::ssa::model::FunctionId,
    Vec<usize>,
    Option<usize>,
);
#[cfg(test)]
thread_local! {
    static OWNER_CALLS: std::cell::RefCell<Option<Vec<OwnerCallWitness>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn observe_owner_calls<T>(lower: impl FnOnce() -> T) -> (T, Vec<OwnerCallWitness>) {
    OWNER_CALLS.with(|events| *events.borrow_mut() = Some(Vec::new()));
    let result = lower();
    let events = OWNER_CALLS.with(|events| events.borrow_mut().take().unwrap_or_default());
    (result, events)
}
#[cfg(test)]
fn record_owner_call(
    call: lang_frontend::type_checking::UnitExpressionId,
    callee: crate::ssa::model::FunctionId,
    arguments: &[body::Slot],
    receipt: Option<usize>,
) {
    OWNER_CALLS.with(|events| {
        if let Some(events) = events.borrow_mut().as_mut() {
            events.push((
                call,
                callee,
                arguments.iter().map(|s| s.0).collect(),
                receipt,
            ));
        }
    });
}

#[cfg(test)]
type OwnerReturnWitness = (
    lang_frontend::type_checking::UnitExpressionId,
    Option<usize>,
    Option<usize>,
);
#[cfg(test)]
thread_local! {
    static OWNER_RETURNS: std::cell::RefCell<Option<Vec<OwnerReturnWitness>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn observe_owner_returns<T>(lower: impl FnOnce() -> T) -> (T, Vec<OwnerReturnWitness>) {
    OWNER_RETURNS.with(|events| *events.borrow_mut() = Some(Vec::new()));
    let result = lower();
    let events = OWNER_RETURNS.with(|events| events.borrow_mut().take().unwrap_or_default());
    (result, events)
}
#[cfg(test)]
fn record_owner_return(
    expression: lang_frontend::type_checking::UnitExpressionId,
    payload: Option<usize>,
    receipt: Option<usize>,
) {
    OWNER_RETURNS.with(|events| {
        if let Some(events) = events.borrow_mut().as_mut() {
            events.push((expression, payload, receipt));
        }
    });
}

pub(crate) mod named_receipt;

pub(crate) mod temporary_receipt;

pub(crate) mod aux_binding_receipt;
pub(crate) mod auxiliary_sequence_receipt;
pub(crate) mod binding_receipt;
pub(crate) mod distinct_aux_receipt;
pub(crate) mod interleaved_aux_receipt;
pub(crate) mod later_aux_receipt;
pub(crate) mod multi_binding_receipt;

mod statements;

#[cfg(test)]
pub(in crate::ssa::refine) mod temporary_ci_tests;

#[cfg(feature = "d10-catalog-experiment")]
pub(crate) mod entry_borrow_receipt;
#[cfg(feature = "d10-catalog-experiment")]
pub(crate) mod entry_lifecycle;

#[cfg(test)]
pub(in crate::ssa) mod two_receiver_plan_test_support;

#[cfg(test)]
pub(in crate::ssa) mod two_receiver_route_metrics;

mod two_receiver_facts;
pub(in crate::ssa) use two_receiver_facts::TwoReceiverFacts;

mod single_receiver_receipt_index;
pub(in crate::ssa) use single_receiver_receipt_index::SingleReceiverReceiptIndex;

mod single_receiver_facts;
pub(in crate::ssa) use single_receiver_facts::SingleReceiverFacts;

mod subset_drop_index;
pub(in crate::ssa) use subset_drop_index::SubsetDropIndex;
