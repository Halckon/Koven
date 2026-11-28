//! Presence tests observe the result loan or one of its lexical children.
use super::*;
use crate::ssa::borrow_result_support::local_if::{
    binding_marker,
    string_control::{collect_condition_operators, statement_control, string_statement_controls},
};
use crate::ssa::{borrow_result_support::ungroup, refine::owned_string_presence_only};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::LoanKind as FrontendLoanKind,
    parser::{BinaryOperator, Expression, ParsedFile, PrefixOperator, Statement},
};
use std::collections::{BTreeMap, BTreeSet};

type Binding = BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>;
type Controls = (BTreeSet<usize>, BTreeSet<usize>);

pub(super) struct Observers {
    per_binding: BTreeMap<UnitSymbolId, Controls>,
    loops: BTreeMap<SourceUnitId, BTreeSet<usize>>,
    arrays: BTreeMap<UnitSymbolId, BTreeSet<usize>>,
    field_children: BTreeSet<UnitSymbolId>,
}

impl Observers {
    pub(super) fn new(
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &dyn UnitPreflightFacts,
        bindings: &[(
            &Binding,
            Option<&ParsedFile>,
            Option<ordinary_local::Policy>,
        )],
    ) -> Self {
        let mut this = Self {
            per_binding: BTreeMap::new(),
            loops: BTreeMap::new(),
            arrays: BTreeMap::new(),
            field_children: BTreeSet::new(),
        };
        let mut parents = BTreeMap::new();
        let mut record = |binding: UnitSymbolId, file, comparisons: Vec<ExpressionId>| {
            let controls = if comparisons.is_empty() {
                Controls::default()
            } else {
                parents
                    .entry(binding.source_unit())
                    .or_insert_with(|| BooleanParents::new(file))
                    .controls(comparisons)
            };
            this.insert(binding, controls);
        };
        for &(binding, file, policy) in bindings {
            let (Some(file), Some(ordinary_local::Policy::Scoped), Some(ty)) =
                (file, policy, nullable_type(typed, binding))
            else {
                continue;
            };
            record(
                binding.binding(),
                file,
                nullable_comparisons(typed, owned, binding, ty),
            );
        }
        let fields = field_comparisons(names, typed, owned, bindings);
        for (binding, (file, comparisons)) in fields.sites {
            record(binding, file, comparisons);
        }
        this.arrays = fields.arrays;
        this.field_children = fields.children;
        this
    }

    pub(super) fn uses_returned_field(&self, binding: UnitSymbolId) -> bool {
        self.field_children.contains(&binding)
    }

    fn insert(&mut self, binding: UnitSymbolId, controls: Controls) {
        self.loops
            .entry(binding.source_unit())
            .or_default()
            .extend(&controls.1);
        let existing = self.per_binding.entry(binding).or_default();
        existing.0.extend(controls.0);
        existing.1.extend(controls.1);
    }

    pub(super) fn take(
        &mut self,
        file: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &dyn UnitPreflightFacts,
        fact: &Binding,
    ) -> Option<Controls> {
        let mut result = match self.per_binding.remove(&fact.binding()) {
            Some(controls) => controls,
            None => (
                admit_string_result_observers(file, names, typed, fact)?,
                BTreeSet::new(),
            ),
        };
        let observed = self.loops.get(&fact.binding().source_unit());
        extend_lexical_controls(file, typed, owned, fact, observed, &mut result);
        // Array admission is local to the binding observed in that provider body.
        result
            .1
            .extend(self.arrays.remove(&fact.binding()).into_iter().flatten());
        Some(result)
    }
}

fn nullable_type(typed: &CompilationUnitTypes, fact: &Binding) -> Option<UnitTypeId> {
    if fact.storage() != BorrowBindingStorage::BorrowedStorage || fact.source_loan().is_none() {
        return None;
    }
    typed.symbol_type(fact.binding()).filter(|&ty| {
        typed.expression_type(fact.initializer()) == Some(ty)
            && matches!(typed.types().get(ty), Some(UnitTypeKind::Nullable(inner))
                if typed.types().get(*inner) == Some(&UnitTypeKind::Builtin(BuiltinType::String)))
    })
}

fn nullable_comparisons(
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    binding: &Binding,
    nullable: UnitTypeId,
) -> Vec<ExpressionId> {
    let unit = binding.binding().source_unit();
    let bindings = owned.borrow_results().bindings();
    let belongs = |symbol| {
        std::iter::successors(Some(symbol), |symbol| {
            let index = bindings
                .binary_search_by_key(symbol, |binding| binding.binding())
                .ok()?;
            let child = &bindings[index];
            if child.storage() != BorrowBindingStorage::BorrowedStorage
                || child.origin() != binding.origin()
                || typed.symbol_type(*symbol) != Some(nullable)
            {
                return None;
            }
            child.parent()
        })
        .take(bindings.len())
        .any(|symbol| symbol == binding.binding())
    };
    typed
        .null_comparisons()
        .iter()
        .filter(|fact| {
            fact.expression().source_unit() == unit
                && fact.nullable_type() == nullable
                && belongs(fact.symbol())
        })
        .map(|fact| fact.expression().expression())
        .collect()
}

/// One parent index serves all observed bindings in a source unit.
struct BooleanParents {
    wrappers: Vec<Vec<ExpressionId>>,
    loops: BTreeMap<usize, usize>,
}

impl BooleanParents {
    fn new(file: &ParsedFile) -> Self {
        let expressions = file.ast().expressions();
        let mut wrappers = vec![Vec::new(); expressions.len()];
        for (id, node) in expressions.iter() {
            match node.payload() {
                Expression::If { condition, .. }
                | Expression::Group {
                    expression: condition,
                }
                | Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand: condition,
                    ..
                } => wrappers[condition.index()].push(id),
                Expression::Binary {
                    left,
                    operator: BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr,
                    right,
                    ..
                } => {
                    wrappers[left.index()].push(id);
                    wrappers[right.index()].push(id);
                }
                _ => {}
            }
        }
        let loops = file
            .ast()
            .statements()
            .iter()
            .filter_map(|(id, node)| {
                let Statement::While { condition, .. } = node.payload() else {
                    return None;
                };
                Some((condition.index(), id.index()))
            })
            .collect();
        Self { wrappers, loops }
    }

    fn controls(&self, mut pending: Vec<ExpressionId>) -> Controls {
        let mut allowed = BTreeSet::new();
        let mut loops = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if allowed.insert(id.index()) {
                pending.extend(self.wrappers[id.index()].iter().copied());
                loops.extend(self.loops.get(&id.index()));
            }
        }
        (allowed, loops)
    }
}

/// Index comparisons by their syntactic root binding, then require the same lease
/// as the published operand loan for an effectful null-only RHS/LHS.
#[derive(Default)]
struct FieldComparisons<'a> {
    sites: BTreeMap<UnitSymbolId, (&'a ParsedFile, Vec<ExpressionId>)>,
    arrays: BTreeMap<UnitSymbolId, BTreeSet<usize>>,
    children: BTreeSet<UnitSymbolId>,
}

fn field_comparisons<'a>(
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    bindings: &[(
        &Binding,
        Option<&'a ParsedFile>,
        Option<ordinary_local::Policy>,
    )],
) -> FieldComparisons<'a> {
    use crate::ssa::lowering_support::{is_null_literal, is_null_only, is_nullable_string};
    let mut files = BTreeMap::new();
    let mut scoped = BTreeSet::new();
    for &(binding, file, policy) in bindings {
        let symbol = binding.binding();
        if let (Some(file), Some(ordinary_local::Policy::Scoped)) = (file, policy)
            && binding.storage() == BorrowBindingStorage::BorrowedStorage
            && binding.source_loan().is_some()
            && typed.symbol_type(symbol).is_some_and(|ty| {
                matches!(typed.types().get(ty), Some(UnitTypeKind::Nominal { .. }))
            })
        {
            files.insert(symbol.source_unit(), file);
            scoped.insert(symbol);
        }
    }
    if scoped.is_empty() {
        return FieldComparisons::default();
    }
    let origins: BTreeMap<_, _> = bindings
        .iter()
        .filter(|(binding, _, _)| {
            binding.storage() == BorrowBindingStorage::BorrowedStorage
                && files.contains_key(&binding.binding().source_unit())
        })
        .map(|(binding, _, _)| (binding.binding(), *binding))
        .collect();
    let mut references = BTreeMap::new();
    for reference in names.names().references() {
        let unit = reference.source_unit();
        if files.contains_key(&unit) {
            let span = reference.span();
            references
                .entry((unit, span.start(), span.end()))
                .and_modify(|target| *target = None)
                .or_insert_with(|| match reference.target() {
                    UnitReferenceTarget::Symbol(symbol) if origins.contains_key(symbol) => {
                        Some(*symbol)
                    }
                    _ => None,
                });
        }
    }
    let mut loans = BTreeMap::<_, Vec<_>>::new();
    for loan in owned.loans().iter().filter(|loan| {
        loan.kind() == FrontendLoanKind::Shared && files.contains_key(&loan.call().source_unit())
    }) {
        loans
            .entry((loan.call(), loan.argument()))
            .or_default()
            .push(loan.target());
    }
    let return_ends = owned
        .borrow_results()
        .ends()
        .iter()
        .filter_map(|end| match end.point() {
            UnitDropPoint::ControlTransfer(transfer) if scoped.contains(&end.binding()) => {
                Some((end.binding(), transfer))
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut result = FieldComparisons::default();
    for (unit, file) in files {
        let id = |expression| UnitExpressionId::new(unit, expression);
        let arrays = array_bodies(file, typed, unit);
        for (comparison, node) in file.ast().expressions().iter() {
            let Expression::Binary {
                operator: BinaryOperator::Equal | BinaryOperator::NotEqual,
                left,
                right,
                ..
            } = *node.payload()
            else {
                continue;
            };
            for (operand, other) in [(left, right), (right, left)] {
                if !typed
                    .expression_type(id(operand))
                    .is_some_and(|ty| is_nullable_string(typed, ty))
                {
                    continue;
                }
                let Some(root) = field_root(file, typed, id(operand), node.span()) else {
                    continue;
                };
                let Some(symbol) = references
                    .get(&(unit, root.start(), root.end()))
                    .copied()
                    .flatten()
                else {
                    continue;
                };
                let origin = origins[&symbol].origin();
                if is_null_literal(file, other)
                    || typed
                        .expression_type(id(other))
                        .is_some_and(|ty| is_null_only(typed, ty))
                        && loans
                            .get(&(id(comparison), id(operand)))
                            .is_some_and(|targets| targets.as_slice() == [origin])
                {
                    // A child reborrow of the same nominal lease also observes
                    // its still-live lexical ancestors.
                    let ancestors = std::iter::successors(Some(symbol), |symbol| {
                        let parent = origins.get(symbol)?.parent()?;
                        (origins.get(&parent)?.origin() == origin
                            && typed.symbol_type(parent) == typed.symbol_type(*symbol))
                        .then_some(parent)
                    });
                    let chain = ancestors.take(origins.len()).collect::<Vec<_>>();
                    let Some(last) = chain.iter().rposition(|id| scoped.contains(id)) else {
                        continue;
                    };
                    result.children.extend(&chain[..last]);
                    for ancestor in chain.into_iter().filter(|id| scoped.contains(id)) {
                        let controls = &mut result
                            .sites
                            .entry(ancestor)
                            .or_insert_with(|| (file, Vec::new()))
                            .1;
                        controls.push(comparison);
                        let span = node.span();
                        // Only enclosing arrays entered after the binding was formed
                        // may carry it. An observer in one loop proves no sibling loop.
                        let nearest = arrays
                            .partition_point(|array| array.body.start() <= span.start())
                            .checked_sub(1);
                        for index in std::iter::successors(nearest, |&index| arrays[index].parent) {
                            let array = &arrays[index];
                            if span.end() <= array.body.end()
                                && origins[&ancestor].marker_span().end() <= array.start
                            {
                                result
                                    .arrays
                                    .entry(ancestor)
                                    .or_default()
                                    .insert(array.statement);
                            }
                        }
                        if let Ok(other) = ungroup(file, other, node.span()) {
                            collect_operand_returns(
                                file,
                                typed,
                                id(other),
                                &|transfer| return_ends.contains(&(ancestor, transfer)),
                                controls,
                            );
                        }
                    }
                }
            }
        }
    }
    result
}

struct ArrayBody {
    body: Span,
    start: usize,
    statement: usize,
    parent: Option<usize>,
}

/// A source-local interval index gives each observer just its enclosing loops.
fn array_bodies(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    unit: SourceUnitId,
) -> Vec<ArrayBody> {
    use lang_frontend::type_checking::{IterationProvider, SequentialContainerKind};
    let mut arrays = typed
        .sequential_iterations()
        .iter()
        .filter_map(|descriptor| {
            let statement = descriptor.statement();
            if statement.source_unit() != unit
                || descriptor.provider()
                    != IterationProvider::Sequential(SequentialContainerKind::Array)
            {
                return None;
            }
            let node = file.ast().statements().get(statement.statement()).ok()?;
            let Statement::For { source, body, .. } = *node.payload() else {
                return None;
            };
            if descriptor.source() != UnitExpressionId::new(unit, source) {
                return None;
            }
            Some(ArrayBody {
                body: file.ast().statements().get(body).ok()?.span(),
                start: node.span().start(),
                statement: statement.statement().index(),
                parent: None,
            })
        })
        .collect::<Vec<_>>();
    arrays.sort_by_key(|array| (array.body.start(), std::cmp::Reverse(array.body.end())));
    let mut parent: Option<usize> = None;
    for index in 0..arrays.len() {
        while let Some(enclosing) = parent {
            if arrays[index].body.end() <= arrays[enclosing].body.end() {
                break;
            }
            parent = arrays[enclosing].parent;
        }
        arrays[index].parent = parent;
        parent = Some(index);
    }
    arrays
}

/// Follow value-producing arms of this operand only. Other controls still
/// require their own admission, including controls preceding the tail.
fn collect_operand_returns(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    expression: UnitExpressionId,
    ends_on_return: &impl Fn(UnitExpressionId) -> bool,
    allowed: &mut Vec<ExpressionId>,
) {
    let Ok(node) = file.ast().expressions().get(expression.expression()) else {
        return;
    };
    let start = allowed.len();
    match *node.payload() {
        Expression::Return { .. }
            if typed.expression_falls_through(expression) == Some(false)
                && ends_on_return(expression) =>
        {
            allowed.push(expression.expression());
        }
        Expression::If {
            then_branch,
            else_branch: Some(other),
            ..
        } if typed.expression_type(expression).is_some_and(|ty| {
            crate::ssa::lowering_support::is_null_only(typed, ty)
                || typed.types().get(ty) == Some(&UnitTypeKind::Builtin(BuiltinType::Nothing))
        }) =>
        {
            for branch in [then_branch, other] {
                if let Some(tail) = ordinary_local::control_tail(file, branch) {
                    collect_operand_returns(
                        file,
                        typed,
                        UnitExpressionId::new(expression.source_unit(), tail),
                        ends_on_return,
                        allowed,
                    );
                }
            }
            if allowed.len() > start {
                allowed.push(expression.expression());
            }
        }
        _ => {}
    }
}

fn field_root(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    operand: UnitExpressionId,
    span: Span,
) -> Option<Span> {
    use lang_frontend::type_checking::UnitAggregateProjectionKind;
    let unit = operand.source_unit();
    let mut current = operand.expression();
    let mut traversed_field = false;
    loop {
        let leaf = ungroup(file, current, span).ok()?;
        let node = file.ast().expressions().get(leaf).ok()?;
        match *node.payload() {
            Expression::Name if traversed_field => return Some(node.span()),
            Expression::Member { receiver, .. } => {
                let id = UnitExpressionId::new(unit, leaf);
                let fact = typed.aggregate_projection(id)?;
                if fact.expression() != id
                    || fact.kind() != UnitAggregateProjectionKind::Field
                    || fact.receiver()
                        != UnitAggregateProjectionReceiver::Expression(UnitExpressionId::new(
                            unit, receiver,
                        ))
                {
                    return None;
                }
                traversed_field = true;
                current = receiver;
            }
            _ => return None,
        }
    }
}

fn extend_lexical_controls(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    binding: &Binding,
    observed: Option<&BTreeSet<usize>>,
    controls: &mut Controls,
) {
    let unit = binding.binding().source_unit();
    if (observed.is_some_and(|loops| !loops.is_empty())
        || typed
            .nullable_whens()
            .iter()
            .any(|when| when.expression().source_unit() == unit))
        && let Some(lifetime) = LexicalLifetime::new(file, owned, binding)
    {
        lifetime.extend(file, typed, owned, binding, observed, controls);
    }
    let (allowed, loops) = controls;
    if loops.is_empty() {
        return;
    }
    for (statement, node) in file.ast().statements().iter() {
        let Statement::While {
            condition, body, ..
        } = node.payload()
        else {
            continue;
        };
        if loops.contains(&statement.index()) {
            collect_condition_operators(file, *condition, allowed, |_| false);
            string_statement_controls(file, typed, unit, &[*body], |_| false, false, allowed);
        }
    }
}

struct LexicalLifetime {
    scope: StatementId,
    normal_end: Option<StatementId>,
}

impl LexicalLifetime {
    fn new(file: &ParsedFile, owned: &dyn UnitPreflightFacts, binding: &Binding) -> Option<Self> {
        let unit = binding.binding().source_unit();
        let (scope, scope_span, suffix) =
            file.ast().statements().iter().find_map(|(scope, node)| {
                let (Statement::Block { elements } | Statement::ControlBody { elements }) =
                    node.payload()
                else {
                    return None;
                };
                let declaration = elements
                    .iter()
                    .position(|id| binding_marker(file, *id) == Some(binding.marker_span()))?;
                Some((scope, node.span(), &elements[declaration..]))
            })?;
        let mut ends = owned
            .borrow_results()
            .ends()
            .iter()
            .filter(|end| end.binding() == binding.binding())
            .peekable();
        ends.peek()?;
        let mut normal_end = None;
        for end in ends {
            match end.point() {
                UnitDropPoint::AfterStatement(end) if end.source_unit() == unit => {
                    let statement = end.statement();
                    if (statement != scope && !suffix.contains(&statement))
                        || normal_end.replace(statement).is_some()
                    {
                        return None;
                    }
                }
                UnitDropPoint::ControlTransfer(end) if end.source_unit() == unit => {
                    let node = file.ast().expressions().get(end.expression()).ok()?;
                    let span = node.span();
                    // The frontend certifies the binding actually ends here.
                    // A function return cannot feed a loop successor.
                    if !matches!(node.payload(), Expression::Return { .. })
                        || span.source_id() != scope_span.source_id()
                        || span.start() < binding.marker_span().end()
                        || span.end() > scope_span.end()
                    {
                        return None;
                    }
                }
                _ => return None,
            }
        }
        Some(Self { scope, normal_end })
    }

    fn extend(
        &self,
        file: &ParsedFile,
        typed: &CompilationUnitTypes,
        owned: &dyn UnitPreflightFacts,
        binding: &Binding,
        observed: Option<&BTreeSet<usize>>,
        (allowed, loops): &mut Controls,
    ) {
        let unit = binding.binding().source_unit();
        let statements = file.ast().statements();
        let mut parents = vec![None; statements.len()];
        for (block, node) in statements.iter() {
            if let Statement::Block { elements } | Statement::ControlBody { elements } =
                node.payload()
            {
                for &child in elements {
                    parents[child.index()] = Some(block);
                }
            }
        }
        let mut root = self.scope;
        while let Some(parent) = parents[root.index()] {
            root = parent;
        }

        // Ascend only through sequential scopes. From there a typed statement
        // control can reach its own arms, but an arm's local binding cannot
        // authorize loops in a sibling arm. Never walk into a loop or callable.
        let mut pending = vec![(root, false)];
        while let Some((statement, inside_nullable_when)) = pending.pop() {
            let Ok(node) = statements.get(statement) else {
                continue;
            };
            if observed.is_some_and(|loops| loops.contains(&statement.index()))
                && Some(statement) != self.normal_end
            {
                loops.insert(statement.index());
            }
            match node.payload() {
                Statement::Block { elements } | Statement::ControlBody { elements } => {
                    pending.extend(
                        elements
                            .iter()
                            .map(|&element| (element, inside_nullable_when)),
                    );
                }
                Statement::Expression { expression } => {
                    let Ok(id) = ungroup(file, *expression, node.span()) else {
                        continue;
                    };
                    if let Some(control) = statement_control(file, typed, unit, id) {
                        if inside_nullable_when {
                            control.allow(id, file, typed, unit, allowed);
                        }
                        pending.extend(
                            control
                                .arms
                                .into_iter()
                                .map(|arm| (arm, inside_nullable_when)),
                        );
                    } else if let Some(arms) = independent_nullable_arms(typed, owned, binding, id)
                    {
                        // This control has its own checked subject and arms. Its
                        // admission is independent of the presence of a loop.
                        allowed.insert(id.index());
                        pending.extend(arms.into_iter().map(|arm| (arm, true)));
                    }
                }
                _ => {}
            }
        }
    }
}

fn independent_nullable_arms(
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    binding: &Binding,
    expression: ExpressionId,
) -> Option<[StatementId; 2]> {
    let unit = binding.binding().source_unit();
    let control = UnitExpressionId::new(unit, expression);
    let descriptor = typed.nullable_when(control)?;
    let plan = owned.nullable_when(control)?;
    let subject = plan.source_place()?;
    let UnitLoanTarget::Place(origin) = binding.origin() else {
        return None;
    };
    if plan.descriptor() != descriptor
        || !owned_string_presence_only(typed, plan)
        || !subject.is_root()
        || descriptor.stable_symbol() != Some(subject.root())
        || subject.root() == origin.root()
    {
        return None;
    }
    let [first, second] = plan.entries() else {
        return None;
    };
    let [first_typed, second_typed] = descriptor.entries() else {
        return None;
    };
    (first.body() == first_typed.body()
        && second.body() == second_typed.body()
        && first.body().source_unit() == unit
        && second.body().source_unit() == unit)
        .then_some([first.body().statement(), second.body().statement()])
}
