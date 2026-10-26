//! Deterministic precheck for the migrated subset (SPEC-0350 §3). It reads typed and
//! ownership facts plus AST construct kinds; it never runs lowering. Anything outside
//! the subset keeps the existing per-family path.
mod heap;

use std::collections::BTreeSet;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{SymbolKind, UnitSymbolId},
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::{
        AssignmentOperator, BinaryOperator, Expression, Item, LiteralKind, NameMarker,
        PrefixOperator, Statement, WhenCondition, WhenEntry,
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
    /// Expressions lowered in branch form, whose own drop points are never emitted.
    branch_form: BTreeSet<UnitExpressionId>,
    /// Pilot routing gate: the function touches a nullable value.
    nullable: bool,
    /// Enclosing `while`/`loop` statements; `break`/`continue` need one.
    loops: usize,
}

pub(super) fn accepts(inputs: &RefineInputs<'_>) -> bool {
    let mut checker = Checker {
        inputs,
        known: inputs.parameters.iter().copied().collect(),
        branch_form: BTreeSet::new(),
        nullable: false,
        loops: 0,
    };
    for ty in inputs
        .parameters
        .iter()
        .filter_map(|symbol| inputs.typed.symbol_type(*symbol))
        .chain([inputs.return_type])
    {
        checker.note_nullable(ty);
    }
    // Borrow parameters are read by copy, so only Copyable ones are in the subset.
    let signature = inputs.parameters.len() == inputs.borrowed.len()
        && inputs
            .parameters
            .iter()
            .zip(&inputs.borrowed)
            .all(|(symbol, borrowed)| {
                inputs.typed.symbol_type(*symbol).is_some_and(|ty| {
                    checker.storage_type(ty)
                        && (!borrowed || inputs.typed.copyability(ty) == Copyability::Copyable)
                })
            })
        && (checker.unit(inputs.return_type) || checker.storage_type(inputs.return_type));
    signature
        && match inputs.body {
            RefineBody::Expression(expression) => checker.expression(expression),
            RefineBody::Block(statement) => checker.statement(statement),
        }
        && checker.drops_supported()
        && checker.nullable
}

/// Every storage type an accepted body uses, plus nullable inners and Boolean flags.
pub(super) fn type_demands(inputs: &RefineInputs<'_>) -> Vec<UnitTypeId> {
    let checker = Checker {
        inputs,
        known: BTreeSet::new(),
        branch_form: BTreeSet::new(),
        nullable: false,
        loops: 0,
    };
    let Ok(item) = inputs.parsed.ast().items().get(inputs.function_item) else {
        return Vec::new();
    };
    let item = item.span();
    let typed = inputs.typed;
    let mut demands: Vec<UnitTypeId> = inputs
        .parameters
        .iter()
        .filter_map(|symbol| typed.symbol_type(*symbol))
        .chain([inputs.return_type])
        .collect();
    for (expression, node) in inputs.parsed.ast().expressions().iter() {
        let span = node.span();
        if item.start() <= span.start() && span.end() <= item.end() {
            demands.extend(
                typed.expression_type(UnitExpressionId::new(inputs.source_unit, expression)),
            );
        }
    }
    demands.extend(typed.body_symbol_types().keys().filter_map(|symbol| {
        (symbol.source_unit() == inputs.source_unit)
            .then(|| typed.symbol_type(*symbol))
            .flatten()
    }));
    demands.extend(typed.types().builtin(BuiltinType::Boolean));
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for ty in demands {
        if !(checker.unit(ty) || checker.storage_type(ty)) {
            continue;
        }
        if let Some(UnitTypeKind::Nullable(inner)) = checker.kind(ty)
            && seen.insert(*inner)
        {
            ordered.push(*inner);
        }
        if seen.insert(ty) {
            ordered.push(ty);
        }
    }
    ordered
}

const fn integer(builtin: BuiltinType) -> bool {
    matches!(
        builtin,
        BuiltinType::Byte
            | BuiltinType::Short
            | BuiltinType::Int
            | BuiltinType::Long
            | BuiltinType::UByte
            | BuiltinType::UShort
            | BuiltinType::UInt
            | BuiltinType::ULong
    )
}

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

    fn scalar(&self, ty: UnitTypeId) -> bool {
        matches!(self.kind(ty), Some(UnitTypeKind::Builtin(b)) if *b == BuiltinType::Boolean || integer(*b))
    }

    fn string(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.kind(ty),
            Some(UnitTypeKind::Builtin(BuiltinType::String))
        )
    }

    fn integer_type(&self, ty: UnitTypeId) -> bool {
        matches!(self.kind(ty), Some(UnitTypeKind::Builtin(b)) if integer(*b))
    }

    /// A non-generic `class` whose fields are scalars or String.
    fn class(&self, ty: UnitTypeId) -> bool {
        self.class_fields(ty).is_some_and(|fields| {
            fields
                .iter()
                .all(|field| self.scalar(*field) || self.string(*field))
        })
    }

    /// Concrete field types of a non-generic `class`, in declaration order.
    fn class_fields(&self, ty: UnitTypeId) -> Option<Vec<UnitTypeId>> {
        let nominal = self.class_signature(ty)?;
        resolve_nominal_runtime_field_types(self.inputs.typed, ty, nominal, &[]).ok()
    }

    fn class_signature(&self, ty: UnitTypeId) -> Option<&UnitNominalSignature> {
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
        (arguments.is_empty()
            && nominal.kind() == NominalKind::Class
            && nominal.type_parameters().is_empty())
        .then_some(nominal)
    }

    /// MoveOnly nullable inners are read through proof views.
    fn view_inner(&self, ty: UnitTypeId) -> bool {
        self.string(ty) || self.class(ty)
    }

    /// Types a slot may hold: scalars, String, plain classes and their nullables.
    fn storage_type(&self, ty: UnitTypeId) -> bool {
        match self.kind(ty) {
            Some(UnitTypeKind::Builtin(BuiltinType::String)) => true,
            Some(UnitTypeKind::Nullable(inner)) => self.scalar(*inner) || self.view_inner(*inner),
            _ => self.scalar(ty) || self.class(ty),
        }
    }

    /// Records whether the function handles a nullable value at all.
    fn note_nullable(&mut self, ty: UnitTypeId) {
        if matches!(self.kind(ty), Some(UnitTypeKind::Nullable(_))) {
            self.nullable = true;
        }
    }

    fn expression_type(&self, expression: ExpressionId) -> Option<UnitTypeId> {
        self.inputs.typed.expression_type(self.id(expression))
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
            && typed.ownership_primitive(id).is_none()
            && typed.nullable_when(id).is_none()
            && self.inputs.constant_owned.is_none_or(|owned| {
                owned.materialization_at(id).is_none() && owned.short_circuit_at(id).is_none()
            })
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

    fn expression(&mut self, expression: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Some(ty) = self.expression_type(expression) else {
            return false;
        };
        // Jumps are typed `Nothing`; a `return` operand carries the value.
        // A `Nothing`-typed `if` diverges on every branch.
        let returns = matches!(
            node.payload(),
            Expression::Return { .. }
                | Expression::Break { .. }
                | Expression::Continue { .. }
                | Expression::If { .. }
        ) && matches!(
            self.kind(ty),
            Some(UnitTypeKind::Builtin(BuiltinType::Nothing))
        );
        if !self.unspecialized(expression) || !(returns || self.unit(ty) || self.storage_type(ty)) {
            return false;
        }
        self.note_nullable(ty);
        let id = self.id(expression);
        if self.inputs.typed.construction(id).is_some() {
            return self.construction(expression);
        }
        if self.inputs.typed.aggregate_projection(id).is_some() {
            return self.projection(expression);
        }
        if self.inputs.typed.non_null_assertion(id).is_some() {
            return self.non_null_assertion(expression);
        }
        match node.payload().clone() {
            Expression::Group { expression } => self.expression(expression),
            Expression::Literal(LiteralKind::Integer(_)) => self.integer_type(ty),
            Expression::Literal(LiteralKind::Boolean(_) | LiteralKind::Null) => true,
            Expression::String { .. } => self
                .inputs
                .sources
                .source_text(self.inputs.parsed.source_id())
                .ok()
                .and_then(|text| {
                    string_literal::decode_plain(self.inputs.parsed, text, expression).ok()
                })
                .flatten()
                .is_some(),
            Expression::Name => self.name(expression),
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => self.expression(operand),
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => self.binary(expression, left, operator, right),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.condition(condition, true)
                    && self.statement(then_branch)
                    && else_branch.is_none_or(|branch| self.statement(branch))
                    && (self.unit(ty) || else_branch.is_some())
                    && (!returns || else_branch.is_some())
            }
            Expression::When {
                subject: None,
                entries,
                ..
            } => self.subjectless_when(ty, &entries),
            Expression::Call { arguments, .. } => {
                let values: Vec<_> = arguments.iter().map(|argument| argument.value).collect();
                self.call(expression) && self.arguments(expression, &values)
            }
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } => self.assignment(target) && self.expression(value),
            Expression::Return { value, .. } => value.is_none_or(|value| self.expression(value)),
            Expression::Break { .. } | Expression::Continue { .. } => self.loops > 0,
            _ => false,
        }
    }

    fn name(&self, expression: ExpressionId) -> bool {
        let Some(symbol) = self.reference(expression) else {
            return false;
        };
        if !self.known.contains(&symbol) {
            return false;
        }
        match self.inputs.typed.non_null_use(self.id(expression)) {
            // Only Copyable inners are narrowed by value in the migrated subset.
            Some(fact) => {
                fact.symbol() == symbol
                    && self.inputs.typed.copyability(fact.narrowed_type()) == Copyability::Copyable
            }
            None => true,
        }
    }

    fn binary(
        &mut self,
        expression: ExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
    ) -> bool {
        if matches!(
            operator,
            BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
        ) || self
            .inputs
            .typed
            .null_comparison(self.id(expression))
            .is_some()
        {
            return self.condition(expression, false);
        }
        let Some(operand) = self.expression_type(left) else {
            return false;
        };
        let supported = match operator {
            BinaryOperator::Add if self.string(operand) => true,
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => self.integer_type(operand),
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                self.scalar(operand) || self.string(operand)
            }
            _ => false,
        };
        let string = self.string(operand);
        supported && self.operand(left, string) && self.operand(right, string)
    }

    /// A String operand may be a MoveOnly non-null name, read through its proof view.
    fn operand(&mut self, expression: ExpressionId, string: bool) -> bool {
        if string
            && let Some(fact) = self.inputs.typed.non_null_use(self.id(expression))
            && self.inputs.typed.copyability(fact.narrowed_type()) == Copyability::MoveOnly
        {
            return self.reference(expression) == Some(fact.symbol())
                && self.known.contains(&fact.symbol());
        }
        self.expression(expression)
    }

    /// Mirrors the builder: `!`, groups, `&&`, `||` and null comparisons are branch form
    /// (their own drop points are never emitted); other conditions are plain values.
    /// `views`: a MoveOnly null comparison may open a proof view only directly in an
    /// `if`/`while` condition (through groups and `&&`), on its true edge.
    fn condition(&mut self, expression: ExpressionId, views: bool) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let id = self.id(expression);
        let boolean = self.expression_type(expression).is_some_and(|ty| {
            matches!(
                self.kind(ty),
                Some(UnitTypeKind::Builtin(BuiltinType::Boolean))
            )
        });
        if let Some(fact) = self.inputs.typed.null_comparison(id) {
            self.branch_form.insert(id);
            self.note_nullable(fact.nullable_type());
            let inner_ok = match self.kind(fact.nullable_type()) {
                Some(UnitTypeKind::Nullable(inner)) if self.scalar(*inner) => true,
                Some(UnitTypeKind::Nullable(inner)) if self.view_inner(*inner) => {
                    views && fact.non_null_when_true()
                }
                _ => false,
            };
            return boolean && inner_ok && self.known.contains(&fact.symbol());
        }
        match node.payload().clone() {
            Expression::Group { expression: inner } => {
                self.branch_form.insert(id);
                boolean && self.plain(expression) && self.condition(inner, views)
            }
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand: inner,
                ..
            } => {
                self.branch_form.insert(id);
                boolean && self.plain(expression) && self.condition(inner, false)
            }
            Expression::Binary {
                left,
                operator: operator @ (BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr),
                right,
                ..
            } => {
                self.branch_form.insert(id);
                let views = views && operator == BinaryOperator::LogicalAnd;
                boolean
                    && self.plain(expression)
                    && self.condition(left, views)
                    && self.condition(right, views)
            }
            _ => self.expression(expression),
        }
    }

    /// Boolean alternatives tested in order; a value-context `when` needs `else`.
    fn subjectless_when(&mut self, ty: UnitTypeId, entries: &[WhenEntry]) -> bool {
        let has_else = entries.iter().any(|entry| entry.else_span.is_some());
        (self.unit(ty) || has_else)
            && entries.iter().all(|entry| {
                entry.else_span.is_some() == entry.conditions.is_empty()
                    && entry.conditions.iter().all(|condition| {
                        matches!(condition, WhenCondition::Expression(condition)
                            if self.condition(*condition, false))
                    })
                    && self.statement(entry.body)
            })
    }

    fn call(&self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.call(id) else {
            return false;
        };
        let simple = descriptor.receiver().is_none()
            && !descriptor.aborts()
            && descriptor.range_construction().is_none()
            && descriptor.borrow_return().is_none()
            && descriptor.arguments().iter().all(|argument| {
                matches!(
                    argument.mode(),
                    ParameterMode::Value | ParameterMode::Borrow
                ) && self.storage_type(argument.parameter_type())
            });
        let target = descriptor.prints_line()
            || (matches!(
                descriptor.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            ) && self
                .inputs
                .source_plan
                .call_site(self.inputs.source_token, id)
                .is_some_and(|site| self.inputs.function_ids.contains_key(site.key())));
        simple && target
    }

    /// Arguments are plain expressions, except that a Borrow argument may be a MoveOnly
    /// non-null name read through its proof view.
    fn arguments(&mut self, call: ExpressionId, values: &[ExpressionId]) -> bool {
        let Some(descriptor) = self.inputs.typed.call(self.id(call)) else {
            return false;
        };
        let modes: Vec<_> = descriptor
            .arguments()
            .iter()
            .map(|argument| {
                (
                    argument.argument_index(),
                    argument.mode(),
                    argument.parameter_type(),
                )
            })
            .collect();
        modes.into_iter().all(|(index, mode, parameter)| {
            let Some(value) = values.get(index).copied() else {
                return false;
            };
            // A borrowed MoveOnly value is wrapped for a nullable parameter only when it is
            // a temporary whose drop obligation can move to the wrapper.
            if mode == ParameterMode::Borrow
                && self.expression_type(value).is_some_and(|actual| {
                    actual != parameter
                        && self.inputs.typed.copyability(actual) != Copyability::Copyable
                })
                && self.inputs.typed.expression_category(self.id(value))
                    != Some(lang_frontend::type_checking::ExpressionCategory::Temporary)
            {
                return false;
            }
            if mode == ParameterMode::Borrow
                && let Some(fact) = self.inputs.typed.non_null_use(self.id(value))
                && self.inputs.typed.copyability(fact.narrowed_type()) == Copyability::MoveOnly
            {
                return self.reference(value) == Some(fact.symbol())
                    && self.known.contains(&fact.symbol());
            }
            self.expression(value)
        })
    }

    fn assignment(&self, target: ExpressionId) -> bool {
        let Some(symbol) = self.reference(target) else {
            return false;
        };
        self.known.contains(&symbol)
            && self
                .inputs
                .typed
                .symbol_type(symbol)
                .is_some_and(|ty| self.inputs.typed.copyability(ty) == Copyability::Copyable)
    }

    fn statement(&mut self, statement: StatementId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().statements().get(statement) else {
            return false;
        };
        match node.payload().clone() {
            Statement::Block { elements } | Statement::ControlBody { elements } => {
                elements.into_iter().all(|element| self.statement(element))
            }
            Statement::Expression { expression } => self.expression(expression),
            Statement::LocalVariable { declaration } => self.local_variable(declaration),
            Statement::While {
                condition, body, ..
            } => self.condition(condition, true) && self.loop_body(body),
            Statement::Loop { body, .. } => self.loop_body(body),
            _ => false,
        }
    }

    fn loop_body(&mut self, body: StatementId) -> bool {
        self.loops += 1;
        let supported = self.statement(body);
        self.loops -= 1;
        supported
    }

    fn local_variable(&mut self, declaration: ItemId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().items().get(declaration) else {
            return false;
        };
        let Item::Variable {
            name: NameMarker::Present(name_span),
            initializer,
            ..
        } = node.payload().clone()
        else {
            return false;
        };
        let initializer_id = self.id(initializer);
        if self
            .inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .any(|fact| fact.initializer() == initializer_id)
            || self
                .inputs
                .sources
                .slice(name_span)
                .is_ok_and(|name| name == "_")
            || !self.expression(initializer)
        {
            return false;
        }
        let symbol = self
            .inputs
            .names
            .names()
            .source_units()
            .get(self.inputs.source_unit.index())
            .and_then(|source| {
                source.resolution().symbols().iter().find(|symbol| {
                    symbol.span() == name_span && symbol.kind() == SymbolKind::Variable
                })
            })
            .and_then(|local| {
                self.inputs
                    .typed
                    .body_symbol_types()
                    .keys()
                    .copied()
                    .find(|symbol| {
                        symbol.source_unit() == self.inputs.source_unit
                            && symbol.symbol() == local.id()
                    })
            });
        let Some(symbol) = symbol else {
            return false;
        };
        let declared = self.inputs.typed.symbol_type(symbol);
        if let Some(ty) = declared {
            self.note_nullable(ty);
        }
        let supported = declared.is_some_and(|ty| self.storage_type(ty));
        self.known.insert(symbol);
        supported
    }

    /// Drop facts must name plain bindings or temporaries at points the builder emits.
    fn drops_supported(&self) -> bool {
        self.inputs.owned.drops().iter().all(|fact| {
            let point_ok = match fact.point() {
                UnitDropPoint::AfterExpression(expression)
                | UnitDropPoint::AfterBinaryOperands(expression) => {
                    !self.branch_form.contains(&expression)
                }
                UnitDropPoint::LambdaEntry(_)
                | UnitDropPoint::BeforeReplacement(_)
                | UnitDropPoint::AfterReplacement(_) => false,
                _ => true,
            };
            let target_ok = matches!(
                fact.target(),
                UnitDropTarget::Named(_) | UnitDropTarget::Temporary(_)
            );
            !self.fact_in_function(fact.value_origin()) || (point_ok && target_ok)
        }) && self
            .inputs
            .owned
            .conditional_receiver_drops()
            .iter()
            .all(|fact| !self.fact_in_function(fact.value_origin()))
    }

    fn fact_in_function(&self, origin: lang_frontend::source::Span) -> bool {
        self.inputs
            .parsed
            .ast()
            .items()
            .get(self.inputs.function_item)
            .is_ok_and(|item| {
                let item = item.span();
                origin.source_id() == item.source_id()
                    && item.start() <= origin.start()
                    && origin.end() <= item.end()
            })
    }
}
