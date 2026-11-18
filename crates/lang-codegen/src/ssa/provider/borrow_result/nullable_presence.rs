//! Presence tests observe the result loan or one of its lexical children.
use super::*;
use crate::ssa::borrow_result_support::local_if::{
    binding_marker,
    string_control::{collect_condition_operators, statement_control, string_statement_controls},
};
use crate::ssa::{borrow_result_support::ungroup, refine::owned_string_presence_only};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{BinaryOperator, Expression, ParsedFile, PrefixOperator, Statement},
};
use std::collections::{BTreeMap, BTreeSet};

type Binding = BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>;
type Controls = (BTreeSet<usize>, BTreeSet<usize>);

pub(super) struct Observers {
    per_binding: BTreeMap<UnitSymbolId, Controls>,
    loops: BTreeMap<SourceUnitId, BTreeSet<usize>>,
}

impl Observers {
    pub(super) fn new(
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
        };
        for &(binding, file, policy) in bindings {
            let (Some(file), Some(ordinary_local::Policy::Scoped), Some(ty)) =
                (file, policy, nullable_type(typed, binding))
            else {
                continue;
            };
            let result = controls(file, typed, owned, binding, ty);
            this.loops
                .entry(binding.binding().source_unit())
                .or_default()
                .extend(&result.1);
            this.per_binding.insert(binding.binding(), result);
        }
        this
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

fn controls(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    binding: &Binding,
    nullable: UnitTypeId,
) -> Controls {
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
    let mut pending = typed
        .null_comparisons()
        .iter()
        .filter(|fact| {
            fact.expression().source_unit() == unit
                && fact.nullable_type() == nullable
                && belongs(fact.symbol())
        })
        .map(|fact| fact.expression().expression())
        .collect::<Vec<_>>();
    if pending.is_empty() {
        return Default::default();
    }

    // Only the Boolean wrappers that consume this exact test may cross the gate.
    let expressions = file.ast().expressions();
    let mut parents = vec![Vec::<ExpressionId>::new(); expressions.len()];
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
            } => {
                parents[condition.index()].push(id);
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr,
                right,
                ..
            } => {
                parents[left.index()].push(id);
                parents[right.index()].push(id);
            }
            _ => {}
        }
    }
    let mut allowed = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if let Some(parents) = parents.get(id.index())
            && allowed.insert(id.index())
        {
            pending.extend(parents.iter().copied());
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
            allowed.contains(&condition.index()).then_some(id.index())
        })
        .collect();
    (allowed, loops)
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
            string_statement_controls(file, typed, unit, &[*body], |_| false, allowed);
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
