//! Deterministic precheck for the migrated subset (SPEC-0350 §3). It reads typed and
//! ownership facts plus AST construct kinds; it never runs lowering. Anything outside
//! the subset keeps the existing per-family path.
mod callable;
mod container;
#[cfg(test)]
mod drop_scan_scope_probe;
mod expression;
mod field;
mod heap;
mod imported_receiver_route;
mod map;
pub(super) mod named_route;
mod nullable_when;
mod owned_receiver_sequence_route;
mod range_borrow;
mod scalar_text;
mod when_control;
mod whole_nominal_exit;
use control::LoopRoute;
pub(super) use control::accepts;
use scalar_text::control;
mod tagged;
use tagged::receiver;

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    parser::{
        AssignmentOperator, BinaryOperator, Expression, Item, LiteralKind, PrefixOperator,
        Statement,
    },
    type_checking::{
        BuiltinType, Copyability, NominalKind, ParameterMode, UnitCallTarget, UnitExpressionId,
        UnitNominalSignature, UnitTypeId, UnitTypeKind,
    },
};

use super::{RefineBody, RefineInputs};
use crate::ssa::{
    lowering_support::string_literal, unit_plan::resolve_nominal_runtime_field_types,
};

struct Checker<'i, 'a> {
    inputs: &'i RefineInputs<'a>,
    /// Parameters and locals declared so far, with whether they are `var`.
    known: BTreeSet<UnitSymbolId>,
    /// Local bindings holding a concrete callable owner (function pointer or closure).
    callables: BTreeSet<UnitSymbolId>,
    /// Concrete SSA type of each local callable binding.
    callable_ssa_types: BTreeMap<UnitSymbolId, crate::ssa::model::SsaTypeId>,
    /// Expressions lowered in branch form, whose own drop points are never emitted.
    branch_form: BTreeSet<UnitExpressionId>,
    /// Temporary field parents whose pending loan and cleanup lower together.
    temporary_field_parents: BTreeSet<UnitExpressionId>,
    /// Source operands whose interpolation borrow ends before a later part can exit.
    interpolation_operands: BTreeSet<UnitExpressionId>,
    whole_nominal_exit: Option<whole_nominal_exit::WholeNominalExit>,
    /// Owned field receivers whose evaluated call prefix cannot dispatch.
    non_dispatching_field_calls: BTreeSet<UnitExpressionId>,
    /// Pilot routing gate: the function touches a nullable value.
    nullable: bool,
    /// Enclosing `while`/`loop` statements; `break`/`continue` need one.
    loops: usize,
    /// Routing gate: the function uses a construct whose family moved to this layer.
    migrated: bool,
    /// A validated local destructuring in this callable requires refinement.
    has_local_destructuring: bool,
    /// A temporary sequential source can refine only in an already migrated scope.
    has_temporary_sequential_for: bool,
    /// A reached When that cannot be lowered on the ordinary route.
    has_refine_only_when: bool,
    /// A validated function-value call needs the refinement callable ABI.
    has_function_value_call: bool,
    loop_route: LoopRoute,
    loop_route_retry: bool,
    /// Field replacements in the subset; their `BeforeReplacement` drops are consumed by
    /// the replacement itself.
    replacements: BTreeSet<UnitExpressionId>,
    /// Container element replacements (`xs[i] = v`) whose `AfterReplacement` drops are
    /// consumed by `ContainerReplace`.
    element_replacements: BTreeSet<UnitExpressionId>,
    /// Element or component bindings introduced by a sequential `for` loop (`Borrow` loans).
    for_borrowed: BTreeSet<UnitSymbolId>,
    /// Local `borrow val` bindings holding a `RangeView` descriptor (`NewRangeDescriptor`).
    range_view_bindings: BTreeSet<UnitSymbolId>,
    /// MoveOnly nullable bindings whose non-null proof view is active in the current branch.
    view_symbols: BTreeSet<UnitSymbolId>,
    string_when_reads: BTreeSet<UnitExpressionId>,
}

fn accepts_with_loop_route(inputs: &RefineInputs<'_>, loop_route: LoopRoute) -> (bool, bool) {
    let mut known: BTreeSet<_> = inputs.parameters.iter().copied().collect();
    if let RefineBody::Lambda(thunk) = &inputs.body {
        known.extend(thunk.captures.iter().map(|capture| capture.symbol));
    }
    let mut checker = Checker {
        inputs,
        known,
        callables: BTreeSet::new(),
        callable_ssa_types: BTreeMap::new(),
        branch_form: BTreeSet::new(),
        temporary_field_parents: BTreeSet::new(),
        interpolation_operands: BTreeSet::new(),
        whole_nominal_exit: None,
        non_dispatching_field_calls: BTreeSet::new(),
        nullable: false,
        loops: 0,
        migrated: false,
        has_local_destructuring: false,
        has_temporary_sequential_for: false,
        has_refine_only_when: false,
        has_function_value_call: false,
        loop_route,
        loop_route_retry: false,
        replacements: BTreeSet::new(),
        element_replacements: BTreeSet::new(),
        for_borrowed: BTreeSet::new(),
        range_view_bindings: BTreeSet::new(),
        view_symbols: BTreeSet::new(),
        string_when_reads: BTreeSet::new(),
    };
    let mut signature_types: Vec<_> = inputs
        .parameters
        .iter()
        .filter_map(|symbol| checker.symbol_type(*symbol))
        .chain([inputs.return_type])
        .chain(inputs.receiver.map(|receiver| receiver.ty))
        .collect();
    if let RefineBody::Lambda(thunk) = &inputs.body {
        signature_types.extend(thunk.captures.iter().map(|capture| capture.ty));
    }
    for ty in signature_types {
        checker.note_nullable(ty);
        checker.migrated |= checker.container_element(ty).is_some()
            || checker.range_view_element(ty).is_some()
            || checker.range_type(ty)
            || checker.box_payload(ty).is_some()
            || checker.enum_type(ty)
            || checker.box_enum_type(ty)
            || checker.map_kv(ty).is_some()
            || checker.rc_payload(ty).is_some()
            || checker.class_has_deinit(ty)
            || checker.move_value_class(ty);
    }
    if inputs.function_item.is_some_and(|item| {
        matches!(
            inputs
                .parsed
                .ast()
                .items()
                .get(item)
                .map(|node| node.payload()),
            Ok(Item::Deinit { .. })
        )
    }) {
        checker.migrated = true;
    }
    // Copyable Borrow parameters are read by copy, except String when subjects read the
    // loan. An Inout parameter's storage is accessed through the caller's exclusive loan.
    let signature = inputs.parameters.len() == inputs.modes.len()
        && inputs
            .parameters
            .iter()
            .zip(&inputs.modes)
            .all(|(symbol, mode)| {
                checker.symbol_type(*symbol).is_some_and(|ty| {
                    // ADR-0034: a Borrow function parameter is a callable view.
                    (checker.unit(ty) && *mode != ParameterMode::Inout)
                        || (*mode == ParameterMode::Borrow && checker.view_function(ty))
                        || checker.storage_type(ty)
                            && match mode {
                                ParameterMode::Value => true,
                                // A MoveOnly Borrow parameter is only lent on (a plain class,
                                // container, or boxed owner through its loan), never read by copy.
                                ParameterMode::Borrow => {
                                    checker.copyability_of(ty) == Copyability::Copyable
                                        || checker.class(ty)
                                        || checker.value_class(ty)
                                        || checker.move_value_class(ty)
                                        || checker.string(ty)
                                        || checker.container_element(ty).is_some()
                                        || checker.range_view_element(ty).is_some()
                                        || checker.box_payload(ty).is_some()
                                        || checker.box_enum_type(ty)
                                        || checker.map_kv(ty).is_some()
                                        || checker.rc_payload(ty).is_some()
                                        || checker.nullable_storage(ty)
                                }
                                ParameterMode::Inout => true,
                            }
                })
            })
        && (checker.unit(inputs.return_type)
            || checker.nothing(inputs.return_type)
            || checker.storage_type(inputs.return_type)
            // A borrow-result function forwards a loan of a nullable owner unchanged.
            || (inputs.borrow_return.is_some() && checker.nullable_storage(inputs.return_type))
            || checker.callable_return())
        && inputs.receiver.is_none_or(|receiver| match receiver.mode {
            ParameterMode::Value | ParameterMode::Borrow => {
                checker.enum_type(receiver.ty)
                    || checker.box_enum_type(receiver.ty)
                    || checker.class(receiver.ty)
                    || checker.value_class(receiver.ty)
                    || checker.move_value_class(receiver.ty)
                    || checker.object_type(receiver.ty)
            }
            ParameterMode::Inout => {
                checker.class(receiver.ty)
                    || checker.value_class(receiver.ty)
                    || checker.move_value_class(receiver.ty)
                    || checker.enum_type(receiver.ty)
                    || checker.box_enum_type(receiver.ty)
            }
        });
    // An Inout parameter or receiver, a MoveOnly Value receiver, or a StaticSelf/object
    // specialization belongs to the migrated subset.
    checker.migrated |= checker.nothing(inputs.return_type)
        || inputs.modes.contains(&ParameterMode::Inout)
        || inputs.static_self.is_some()
        || inputs.receiver.is_some_and(|receiver| {
            receiver.mode == ParameterMode::Inout
                || checker.object_type(receiver.ty)
                || (receiver.mode == ParameterMode::Value
                    && checker.copyability_of(receiver.ty) == Copyability::MoveOnly)
        });
    let body_ok = match &inputs.body {
        RefineBody::Expression(expression) => {
            if inputs.borrow_return.is_some() {
                checker.borrow_return_body(*expression, *expression)
            } else if inputs.carrier_return.is_some() {
                checker.carrier_return_body(*expression, *expression)
            } else {
                checker.expression(*expression)
            }
        }
        RefineBody::Block(statement) => checker.statement(*statement),
        RefineBody::Lambda(thunk) => checker.thunk_body(thunk),
    };
    // Use only visited, admitted When nodes to qualify the temporary-source route.
    // The scope scan would also see When nodes after a terminal statement or in a nested callable.
    let accepted = signature
        && body_ok
        && (loop_route == LoopRoute::Existing
            || checker.loop_route == LoopRoute::Verified
                && (checker.has_local_destructuring || checker.has_refine_only_when))
        && (!checker.has_temporary_sequential_for
            || checker.has_local_destructuring
            || checker.has_refine_only_when
            || checker.has_function_value_call
            || matches!(inputs.body, RefineBody::Lambda(_))
            || checker.scope_requires_refine_without_when())
        && checker.drops_supported()
        && (checker.nullable
            || checker.migrated
            || checker.bounded_receiver_route()
            || checker.imported_receiver_route().is_some()
            || checker.owned_receiver_sequence_route().is_some()
            || checker.bounded_named_call().is_some()
            || checker.bounded_temporary_call().is_some()
            || checker.linear_scalar_body());
    (accepted, signature && checker.loop_route_retry)
}

/// Reuse the exact existing bounded shape without consulting observation products.
pub(super) fn bounded_receiver_call(inputs: &RefineInputs<'_>) -> Option<UnitExpressionId> {
    if !query_checker(inputs).bounded_receiver_route() {
        return None;
    }
    let RefineBody::Block(body) = inputs.body else {
        return None;
    };
    let Statement::Block { elements } = inputs.parsed.ast().statements().get(body).ok()?.payload()
    else {
        return None;
    };
    let [statement] = elements.as_slice() else {
        return None;
    };
    let Statement::Expression { expression } = inputs
        .parsed
        .ast()
        .statements()
        .get(*statement)
        .ok()?
        .payload()
    else {
        return None;
    };
    Some(UnitExpressionId::new(inputs.source_unit, *expression))
}

fn query_checker<'i, 'a>(inputs: &'i RefineInputs<'a>) -> Checker<'i, 'a> {
    Checker {
        inputs,
        known: BTreeSet::new(),
        callables: BTreeSet::new(),
        callable_ssa_types: BTreeMap::new(),
        branch_form: BTreeSet::new(),
        temporary_field_parents: BTreeSet::new(),
        interpolation_operands: BTreeSet::new(),
        whole_nominal_exit: None,
        non_dispatching_field_calls: BTreeSet::new(),
        nullable: false,
        loops: 0,
        migrated: false,
        has_local_destructuring: false,
        has_temporary_sequential_for: false,
        has_refine_only_when: false,
        has_function_value_call: false,
        loop_route: LoopRoute::Existing,
        loop_route_retry: false,
        replacements: BTreeSet::new(),
        element_replacements: BTreeSet::new(),
        for_borrowed: BTreeSet::new(),
        range_view_bindings: BTreeSet::new(),
        view_symbols: BTreeSet::new(),
        string_when_reads: BTreeSet::new(),
    }
}

/// Every storage type an accepted body uses, plus nullable inners and Boolean flags.
pub(super) fn type_demands(inputs: &RefineInputs<'_>) -> Vec<UnitTypeId> {
    let checker = query_checker(inputs);
    let Some(item) = inputs.scope_span() else {
        return Vec::new();
    };
    let typed = inputs.typed;
    let mut demands: Vec<UnitTypeId> = inputs
        .parameters
        .iter()
        .filter_map(|symbol| checker.symbol_type(*symbol))
        .chain([inputs.return_type])
        .chain(inputs.receiver.map(|receiver| receiver.ty))
        .collect();
    if let RefineBody::Lambda(thunk) = &inputs.body {
        demands.extend(thunk.captures.iter().map(|capture| capture.ty));
    }
    for (expression, node) in inputs.parsed.ast().expressions().iter() {
        let span = node.span();
        if item.start() <= span.start() && span.end() <= item.end() {
            let id = UnitExpressionId::new(inputs.source_unit, expression);
            checker.demand_prefix_nullable_arguments(id, node.payload(), &mut demands);
            if let Some(ty) = inputs.typed.expression_type(id) {
                let ty = match checker.kind(ty) {
                    Some(UnitTypeKind::EnumCase { root, .. }) => *root,
                    _ => ty,
                };
                demands.extend(inputs.concrete(ty));
            }
            if let Some(site) = inputs.source_plan.call_site(inputs.source_token, id) {
                for route in site.delegation() {
                    demands.push(route.outer_receiver());
                    demands.push(route.delegate_receiver());
                }
            }
            if let Some(prim) = inputs.typed.ownership_primitive(id) {
                demands.push(checker.c(prim.value_type()));
            }
        }
    }
    demands.extend(checker.local_type_demands(item));
    demands.extend(typed.types().builtin(BuiltinType::Boolean));
    // Case tests compare the `Int` discriminant; numeric range `for` loops and container
    // operations use `Int` lengths/indices.
    if demands.iter().any(|ty| {
        checker.enum_type(*ty)
            || checker.box_enum_type(*ty)
            || checker.container_element(*ty).is_some()
            || checker.range_view_element(*ty).is_some()
            || checker.map_kv(*ty).is_some()
            || matches!(
                checker.kind(*ty),
                Some(UnitTypeKind::Builtin(BuiltinType::IntRange))
            )
    }) {
        demands.extend(typed.types().builtin(BuiltinType::Int));
    }
    if demands.iter().any(|ty| checker.map_kv(*ty).is_some()) {
        demands.extend(typed.types().builtin(BuiltinType::ULong));
    }
    if demands.iter().any(|ty| {
        matches!(
            checker.kind(*ty),
            Some(UnitTypeKind::Builtin(BuiltinType::LongRange))
        )
    }) {
        demands.extend(typed.types().builtin(BuiltinType::Long));
    }
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for ty in demands {
        checker.push_demand(ty, &mut seen, &mut ordered);
    }
    ordered
}

pub(in crate::ssa) use crate::ssa::lowering_support::is_integer_builtin as integer;

impl Checker<'_, '_> {
    fn kind(&self, ty: UnitTypeId) -> Option<&UnitTypeKind> {
        self.inputs.typed.types().get(ty)
    }

    fn unit(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.kind(ty),
            Some(UnitTypeKind::Builtin(BuiltinType::Unit))
        )
    }

    pub(super) fn nothing(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.kind(ty),
            Some(UnitTypeKind::Builtin(BuiltinType::Nothing))
        )
    }

    fn scalar(&self, ty: UnitTypeId) -> bool {
        matches!(self.kind(ty), Some(UnitTypeKind::Builtin(b)) if *b == BuiltinType::Boolean || integer(*b))
    }

    fn nullable_scalar_inner(&self, ty: UnitTypeId) -> bool {
        self.scalar(ty)
            || matches!(self.kind(ty), Some(UnitTypeKind::Builtin(b))
                if *b == BuiltinType::Char || crate::ssa::lowering_support::is_float_builtin(*b))
    }

    fn string(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.kind(ty),
            Some(UnitTypeKind::Builtin(BuiltinType::String))
        )
    }

    pub(super) fn integer_type(&self, ty: UnitTypeId) -> bool {
        matches!(self.kind(ty), Some(UnitTypeKind::Builtin(b)) if integer(*b))
    }

    /// A concrete non-recursive `class` whose fields are supported storage types.
    fn class(&self, ty: UnitTypeId) -> bool {
        self.class_seen(ty, &mut BTreeSet::new())
    }

    /// Concrete field types of a `class`, in declaration order.
    fn class_fields(&self, ty: UnitTypeId) -> Option<Vec<UnitTypeId>> {
        let (nominal, arguments) = self.class_signature(ty)?;
        resolve_nominal_runtime_field_types(self.inputs.typed, ty, nominal, &arguments).ok()
    }

    fn class_signature(&self, ty: UnitTypeId) -> Option<(&UnitNominalSignature, Vec<UnitTypeId>)> {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.kind(ty)
        else {
            return None;
        };
        let nominal = self
            .inputs
            .typed
            .signatures()
            .declaration(*declaration)?
            .nominal()?;
        (nominal.kind() == NominalKind::Class).then(|| (nominal, arguments.clone()))
    }

    /// MoveOnly nullable inners are read through proof views.
    fn view_inner(&self, ty: UnitTypeId) -> bool {
        self.view_inner_seen(ty, &mut BTreeSet::new())
    }

    /// Types a slot may hold: scalars, String, plain classes and their nullables.
    fn storage_type(&self, ty: UnitTypeId) -> bool {
        self.storage_type_seen(ty, &mut BTreeSet::new())
    }

    /// Records whether the function handles a nullable value at all.
    fn note_nullable(&mut self, ty: UnitTypeId) {
        if matches!(self.kind(ty), Some(UnitTypeKind::Nullable(_))) {
            self.nullable = true;
        }
    }

    fn expression_type(&self, expression: ExpressionId) -> Option<UnitTypeId> {
        self.expression_type_of(self.id(expression))
    }

    /// Typed facts carry template types; a generic instance reads them concretely.
    fn expression_type_of(&self, id: UnitExpressionId) -> Option<UnitTypeId> {
        self.inputs
            .typed
            .expression_type(id)
            .and_then(|ty| self.inputs.concrete(ty))
    }

    fn symbol_type(&self, symbol: UnitSymbolId) -> Option<UnitTypeId> {
        self.inputs
            .typed
            .symbol_type(symbol)
            .and_then(|ty| self.inputs.concrete(ty))
    }

    /// The concrete identity of a type carried by a typed descriptor; unresolvable types
    /// stay as they are and match no concrete rule.
    fn c(&self, ty: UnitTypeId) -> UnitTypeId {
        self.inputs.concrete(ty).unwrap_or(ty)
    }

    fn copyability_of(&self, ty: UnitTypeId) -> Copyability {
        self.inputs.typed.copyability(self.c(ty))
    }

    fn id(&self, expression: ExpressionId) -> UnitExpressionId {
        UnitExpressionId::new(self.inputs.source_unit, expression)
    }

    /// No specialized typed fact may claim the expression.
    fn plain(&self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        typed.construction(id).is_none()
            && typed.aggregate_projection(id).is_none()
            && typed.non_null_assertion(id).is_none()
            && typed.ownership_primitive(id).is_none()
            && self
                .inputs
                .constant_owned
                .is_none_or(|owned| owned.materialization_at(id).is_none())
            && self.unspecialized(expression)
    }

    /// No typed fact outside the class/assertion families claims the expression.
    fn unspecialized(&self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        typed.integer_operation(id).is_none()
            && typed.integer_range_operation(id).is_none()
            && typed.string_operation(id).is_none()
            && typed.rc_operation(id).is_none()
            && typed.range_size(id).is_none()
            && typed.nullable_when(id).is_none()
            && !self.has_container_fact(expression)
    }

    fn reference(&self, expression: ExpressionId) -> Option<UnitSymbolId> {
        let span = self
            .inputs
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .ok()?
            .span();
        self.inputs
            .references
            .get(&(span.start(), span.end()))
            .copied()
    }

    fn statement(&mut self, statement: StatementId) -> bool {
        if self.when_proof_crosses_loop(statement) {
            return false;
        }
        let Ok(node) = self.inputs.parsed.ast().statements().get(statement) else {
            return false;
        };
        match node.payload().clone() {
            Statement::Block { elements }
            | Statement::ControlBody { elements }
            | Statement::LambdaBody { elements } => {
                let mut iter = elements.into_iter();
                while let Some(element) = iter.next() {
                    if !self.statement(element) {
                        return false;
                    }
                    if !self.statement_may_continue(element) {
                        if iter.any(|stmt| self.stmt_has_runtime_container(stmt)) {
                            self.migrated = true;
                        }
                        break;
                    }
                }
                true
            }
            Statement::Expression { expression } => self.expression(expression),
            Statement::LocalVariable { declaration } => self.local_variable(declaration),
            Statement::While {
                condition, body, ..
            } => self.check_while(condition, body, node.span()),
            Statement::Loop { body, .. } => {
                self.strip_reassigned_loop_views(node.span(), false);
                self.loop_body(body)
            }
            Statement::For { source, body, .. } => {
                self.strip_reassigned_loop_views(node.span(), false);
                self.numeric_for(statement, source, body)
                    || self.sequential_for(statement, source, body, node.span())
                    || self.range_view_for(statement, source, body)
                    || self.map_for(statement, source, body)
            }
            Statement::LocalDestructuring { initializer, .. } => {
                self.local_destructuring(statement, initializer)
            }
            _ => false,
        }
    }

    fn loop_body(&mut self, body: StatementId) -> bool {
        self.loops += 1;
        let supported = self.statement(body);
        self.loops -= 1;
        supported
    }
}
