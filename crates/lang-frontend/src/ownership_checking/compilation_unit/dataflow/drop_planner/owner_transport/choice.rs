//! Single-binding Choice collection at real planner transitions, separate from legacy IDs.
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::values::{
    UnitOwnerChoiceIdV1, UnitOwnerChoiceInputV1, UnitOwnerChoiceV1, UnitOwnerProducerCatalogV1,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueObservationV1, UnitOwnerValueV1 as Value,
};
use crate::parser::VariableKind;

/// Private typed admission: exactly one immutable binding, no value uses or exits.
struct Admission {
    callable: UnitOwnerCallable,
    control: UnitExpressionId,
    symbol: UnitSymbolId,
    drop_point: crate::ownership_checking::UnitDropPoint,
    // Actual expression identities from literal outward through Groups to the tail.
    arms: [Vec<UnitExpressionId>; 2],
}

pub(in super::super) struct ChoiceTransaction {
    admission: Admission,
    catalog: UnitOwnerProducerCatalogV1,
    events: [Vec<Event>; 2],
    results: [Option<(UnitExpressionId, UnitOwnerId)>; 2],
    delivered: [bool; 2],
    completed: [usize; 2],
    control_completed: bool,
    suffix: Vec<Event>,
    bound: bool,
    dropped: bool,
    valid: bool,
}

fn admission(checker: &Checker<'_>, item: ItemId) -> Option<Admission> {
    let ast = checker.parsed.ast();
    let Item::Function {
        name,
        parameters,
        form,
        ..
    } = ast.items().get(item).ok()?.payload()
    else {
        return None;
    };
    // A top-level ordinary, non-generic function with exactly one Boolean parameter.
    if !checker.parsed.roots().contains(&item)
        || parameters.len() != 1
        || parameters[0].mode_marker.is_some()
        || checker.receiver_context(*name).is_some()
    {
        return None;
    }
    let function = *checker.marker_symbol(*name)?;
    let signature = checker
        .typed
        .signatures()
        .declarations()
        .iter()
        .find(|declaration| declaration.symbol() == function)?
        .callable()?;
    if signature.receiver().is_some()
        || !signature.type_parameters().is_empty()
        || checker.typed.types().get(signature.return_type())
            != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
    {
        return None;
    }
    let parameter = *checker.marker_symbol(parameters[0].name)?;
    if checker
        .typed
        .symbol_type(parameter)
        .and_then(|ty| checker.typed.types().get(ty))
        != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
    {
        return None;
    }
    let body = match form {
        FunctionForm::ImplicitUnitBlock(body)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => *body,
        _ => return None,
    };
    let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
        return None;
    };
    let [statement] = elements.as_slice() else {
        return None;
    };
    let Statement::LocalVariable { declaration } = ast.statements().get(*statement).ok()?.payload()
    else {
        return None;
    };
    let Item::Variable {
        kind: VariableKind::Val,
        name,
        initializer,
        ..
    } = ast.items().get(*declaration).ok()?.payload()
    else {
        return None;
    };
    let symbol = *checker.marker_symbol(*name)?;
    if checker
        .typed
        .symbol_type(symbol)
        .and_then(|ty| checker.typed.types().get(ty))
        != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        || !checker.is_move_only_variable(symbol)
    {
        return None;
    }
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = ast.expressions().get(*initializer).ok()?.payload()
    else {
        return None;
    };
    if checker
        .typed
        .expression_type(checker.unit_expression(*initializer))
        .and_then(|ty| checker.typed.types().get(ty))
        != Some(&UnitTypeKind::Builtin(BuiltinType::String))
    {
        return None;
    }
    let condition_node = ast.expressions().get(*condition).ok()?;
    if !matches!(condition_node.payload(), Expression::Name)
        || checker.reference_symbol(condition_node.span()) != Some(parameter)
    {
        return None;
    }
    fn arm(checker: &Checker<'_>, body: StatementId) -> Option<Vec<UnitExpressionId>> {
        let ast = checker.parsed.ast();
        let statement = match ast.statements().get(body).ok()?.payload() {
            Statement::ControlBody { elements } => {
                let [tail] = elements.as_slice() else {
                    return None;
                };
                *tail
            }
            Statement::Expression { .. } => body,
            _ => return None,
        };
        let Statement::Expression { expression } = ast.statements().get(statement).ok()?.payload()
        else {
            return None;
        };
        let mut ids = Vec::new();
        let mut current = *expression;
        loop {
            if checker.is_constant_use(current) {
                return None;
            }
            ids.push(checker.unit_expression(current));
            match ast.expressions().get(current).ok()?.payload() {
                Expression::Group { expression } => current = *expression,
                Expression::String { parts }
                    if !parts
                        .iter()
                        .any(|part| matches!(part, StringPart::Interpolation { .. })) =>
                {
                    break;
                }
                _ => return None,
            }
        }
        ids.reverse();
        Some(ids)
    }
    Some(Admission {
        callable: UnitOwnerCallable::Function(UnitItemId::new(checker.source_unit, item)),
        control: checker.unit_expression(*initializer),
        symbol,
        drop_point: crate::ownership_checking::UnitDropPoint::AfterStatement(UnitStatementId::new(
            checker.source_unit,
            *statement,
        )),
        arms: [arm(checker, *then_branch)?, arm(checker, *else_branch)?],
    })
}

impl ChoiceTransaction {
    pub(in super::super) fn begin(
        checker: &Checker<'_>,
        item: ItemId,
        identity: std::sync::Arc<()>,
    ) -> Option<Self> {
        let admission = admission(checker, item)?;
        Some(Self {
            catalog: UnitOwnerProducerCatalogV1 {
                identity,
                callable: admission.callable,
                definitions: Vec::new(),
            },
            admission,
            events: [Vec::new(), Vec::new()],
            results: [None, None],
            delivered: [false, false],
            completed: [0, 0],
            control_completed: false,
            suffix: Vec::new(),
            bound: false,
            dropped: false,
            valid: true,
        })
    }
    fn abort(&mut self) {
        self.valid = false;
        self.catalog.definitions.clear();
        self.events.iter_mut().for_each(Vec::clear);
        self.results = [None, None];
        self.delivered = [false, false];
        self.completed = [0, 0];
        self.control_completed = false;
        self.suffix.clear();
        self.bound = false;
        self.dropped = false;
    }
    pub(in super::super) fn string_completed(&mut self, expression: UnitExpressionId) {
        if !self.valid {
            return;
        }
        let Some(edge) = self
            .admission
            .arms
            .iter()
            .position(|arm| arm.first() == Some(&expression))
        else {
            self.abort();
            return;
        };
        if self.results[edge].is_some() || self.delivered[edge] || (edge == 1 && !self.delivered[0])
        {
            self.abort();
            return;
        }
        let owner = UnitOwnerId {
            identity: self.catalog.identity.clone(),
            callable: self.catalog.callable,
            index: self.catalog.definitions.len(),
        };
        self.catalog.definitions.push(UnitOwnerDefinition {
            id: owner.clone(),
            origin: UnitOwnerOrigin::String(expression),
        });
        self.events[edge].push(Event::Create(expression, owner.clone()));
        self.results[edge] = Some((expression, owner));
    }
    pub(in super::super) fn expression_completed(
        &mut self,
        expression: UnitExpressionId,
        consuming: bool,
        continues: bool,
    ) {
        if !self.valid {
            return;
        }
        if !continues {
            self.abort();
            return;
        }
        if expression == self.admission.control {
            if !consuming || self.control_completed || self.delivered != [true, true] {
                self.abort();
                return;
            }
            self.control_completed = true;
            return;
        }
        let Some((edge, position)) =
            self.admission
                .arms
                .iter()
                .enumerate()
                .find_map(|(edge, arm)| {
                    arm.iter()
                        .position(|id| *id == expression)
                        .map(|position| (edge, position))
                })
        else {
            return;
        };
        if !consuming || self.delivered[edge] || self.completed[edge] != position {
            self.abort();
            return;
        }
        let Some((from, owner)) = self.results[edge].clone() else {
            self.abort();
            return;
        };
        if position == 0 {
            if from != expression {
                self.abort();
            }
        } else if self.admission.arms[edge].get(position - 1) == Some(&from) {
            self.events[edge].push(Event::Forward(
                from,
                expression,
                Value::Definition(owner.clone()),
            ));
            self.results[edge] = Some((expression, owner));
        } else {
            self.abort();
        }
        if self.valid {
            self.completed[edge] += 1;
        }
    }
    pub(in super::super) fn edge_completed(
        &mut self,
        control: UnitExpressionId,
        edge: usize,
        tail: Option<UnitExpressionId>,
        continues: bool,
    ) {
        if !self.valid {
            return;
        }
        if control != self.admission.control
            || edge > 1
            || !continues
            || self.delivered[edge]
            || self.completed[edge] != self.admission.arms[edge].len()
            || tail != self.admission.arms[edge].last().copied()
            || self.results[edge].as_ref().map(|(actual, _)| *actual) != tail
        {
            self.abort();
            return;
        }
        self.delivered[edge] = true;
    }
    fn choice_id(&self) -> UnitOwnerChoiceIdV1 {
        UnitOwnerChoiceIdV1 {
            identity: self.catalog.identity.clone(),
            callable: self.catalog.callable,
            control: self.admission.control,
        }
    }
    pub(in super::super) fn bound(&mut self, symbol: UnitSymbolId, initializer: UnitExpressionId) {
        if !self.valid {
            return;
        }
        if self.bound
            || !self.control_completed
            || self.delivered != [true, true]
            || symbol != self.admission.symbol
            || initializer != self.admission.control
        {
            self.abort();
            return;
        }
        self.suffix.push(Event::Bind(
            symbol,
            initializer,
            Value::Choice(self.choice_id()),
        ));
        self.bound = true;
    }
    pub(in super::super) fn dropped(
        &mut self,
        point: crate::ownership_checking::UnitDropPoint,
        symbol: UnitSymbolId,
    ) {
        if !self.valid {
            return;
        }
        if !self.bound
            || self.dropped
            || symbol != self.admission.symbol
            || point != self.admission.drop_point
        {
            self.abort();
            return;
        }
        self.suffix
            .push(Event::Drop(point, symbol, Value::Choice(self.choice_id())));
        self.dropped = true;
    }
    pub(in super::super) fn finish(self) -> Option<UnitOwnerValueObservationV1> {
        if !self.valid || !self.bound || !self.dropped || self.delivered != [true, true] {
            return None;
        }
        let [Some((left_tail, left)), Some((right_tail, right))] = &self.results else {
            return None;
        };
        let choice = UnitOwnerChoiceV1 {
            id: self.choice_id(),
            events: self.events.clone(),
            inputs: [
                UnitOwnerChoiceInputV1 {
                    edge: 0,
                    tail: *left_tail,
                    owner: left.clone(),
                },
                UnitOwnerChoiceInputV1 {
                    edge: 1,
                    tail: *right_tail,
                    owner: right.clone(),
                },
            ],
        };
        let graph = UnitOwnerValueObservationV1 {
            catalog: self.catalog,
            prefix: Vec::new(),
            choice: Some(choice),
            suffix: self.suffix,
        };
        graph.is_consistent().then_some(graph)
    }
}
