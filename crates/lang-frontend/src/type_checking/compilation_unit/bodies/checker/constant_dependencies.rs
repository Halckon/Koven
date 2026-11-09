//! Order constant initializers before ordinary bodies; syntax edges retain short-circuit RHSs.
use super::{BodyChecker, CompilationUnitTypeError};
use crate::{
    ast::TypeRefId,
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{DeclarationId, Namespace, UnitSymbolId},
    parser::{Expression, Item, LiteralKind, NameMarker, StringPart},
    source::Span,
    type_checking::{
        BuiltinType, TypeCheckingError, UnitExpressionId, UnitTypeKind,
        constant_graph::strongly_connected_components,
        constant_value::{accepts_binary_operand, accepts_prefix_operand},
    },
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone)]
struct Initializer {
    expression: UnitExpressionId,
    type_ref: Option<TypeRefId>,
    owner: Option<DeclarationId>,
    name_span: Span,
    expressions: Vec<UnitExpressionId>,
}

struct SyntaxDependencies {
    dependencies: BTreeSet<UnitSymbolId>,
    expressions: Vec<UnitExpressionId>,
}

impl BodyChecker<'_> {
    pub(super) fn precheck_constant_dependencies(
        &mut self,
    ) -> Result<(), CompilationUnitTypeError> {
        let has_signature_errors = !self.signatures.diagnostics().is_empty();
        let mut inputs = BTreeMap::new();
        for source in self.names.index().source_units() {
            for (_, node) in self.file(source.id()).ast().items().iter() {
                let Item::Constant {
                    name: NameMarker::Present(name_span),
                    type_ref,
                    initializer,
                    ..
                } = node.payload()
                else {
                    continue;
                };
                let Some(symbol) = self.symbol_at(source.id(), *name_span, Namespace::Value) else {
                    continue;
                };
                if self.signatures.symbol_type(symbol).is_none() {
                    continue;
                }
                inputs.insert(
                    symbol,
                    Initializer {
                        expression: UnitExpressionId::new(source.id(), *initializer),
                        type_ref: *type_ref,
                        owner: self
                            .signatures
                            .constant_declaration(symbol)
                            .map(|(owner, _)| owner),
                        name_span: *name_span,
                        expressions: Vec::new(),
                    },
                );
            }
        }
        let mut dependencies = BTreeMap::new();
        for (&symbol, input) in &mut inputs {
            // Retain the symbol as an invalid dependency. Its initializer still
            // receives ordinary recovery checking outside the dependency graph.
            if let Some(type_ref) = input.type_ref
                && ((has_signature_errors
                    && self.signature_error_in(
                        self.top_level_type_ref_span(input.expression.source_unit(), type_ref)?,
                    ))
                    || self
                        .signatures
                        .symbol_type(symbol)
                        .is_some_and(|ty| !self.constant_type_is_allowed(ty)))
            {
                continue;
            }
            if let Ok(syntax) = self.constant_syntax_dependencies(input.expression, None)? {
                input.expressions = syntax.expressions;
                dependencies.insert(symbol, syntax.dependencies);
            }
        }
        self.constant_inputs = inputs
            .iter()
            .map(|(&symbol, input)| {
                (
                    symbol,
                    (
                        input.name_span,
                        dependencies
                            .get(&symbol)
                            .map(|edges| edges.iter().copied().collect())
                            .unwrap_or_default(),
                    ),
                )
            })
            .collect();
        let mut invalid = inputs
            .keys()
            .filter(|symbol| !dependencies.contains_key(symbol))
            .copied()
            .collect::<BTreeSet<_>>();
        let mut reverse: BTreeMap<UnitSymbolId, Vec<UnitSymbolId>> = BTreeMap::new();
        for (&symbol, edges) in &dependencies {
            for &target in edges {
                reverse.entry(target).or_default().push(symbol);
            }
        }
        let initial_invalid = invalid.iter().copied().collect::<Vec<_>>();
        propagate_invalid(&mut invalid, &reverse, initial_invalid);
        let mut remaining = dependencies
            .iter()
            .map(|(&symbol, edges)| (symbol, edges.len()))
            .collect::<BTreeMap<_, _>>();
        let mut ready = remaining
            .iter()
            .filter_map(|(&symbol, &count)| (count == 0).then_some(symbol))
            .collect::<VecDeque<_>>();
        let mut evaluation_failures = Vec::new();
        while let Some(symbol) = ready.pop_front() {
            if !invalid.contains(&symbol) {
                if !self.check_constant_initializer(symbol, &inputs[&symbol])? {
                    invalid.insert(symbol);
                    propagate_invalid(&mut invalid, &reverse, [symbol]);
                } else if dependencies[&symbol]
                    .iter()
                    .all(|target| self.constant_values.contains_key(target))
                {
                    match crate::type_checking::constant_evaluation::evaluate(
                        self,
                        inputs[&symbol].expression,
                    )? {
                        Some(value) => {
                            self.constant_values.insert(symbol, value);
                        }
                        None => evaluation_failures.push(symbol),
                    }
                }
            }
            for &dependent in reverse.get(&symbol).into_iter().flatten() {
                let count = remaining
                    .get_mut(&dependent)
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                *count -= 1;
                if *count == 0 {
                    ready.push_back(dependent);
                }
            }
        }
        let symbols = remaining
            .iter()
            .filter_map(|(&symbol, &count)| {
                (count > 0 && !invalid.contains(&symbol)).then_some(symbol)
            })
            .collect::<Vec<_>>();
        let indices = symbols
            .iter()
            .enumerate()
            .map(|(index, &symbol)| (symbol, index))
            .collect::<BTreeMap<_, _>>();
        let mut edges = vec![Vec::new(); symbols.len()];
        for (index, symbol) in symbols.iter().enumerate() {
            for target in &dependencies[symbol] {
                if let Some(&provider) = indices.get(target) {
                    edges[index].push(provider);
                } else if !inputs.contains_key(target) {
                    return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
                }
            }
        }
        let mut components = strongly_connected_components(&edges);
        let mut component_of = vec![0; symbols.len()];
        for (id, component) in components.iter_mut().enumerate() {
            component.sort_unstable();
            for &index in component.iter() {
                component_of[index] = id;
            }
        }

        // Edges point from consumers to providers. Stabilize each remaining
        // component before checking consumers outside it.
        for (id, component) in components.iter().enumerate().rev() {
            let mut queued = component
                .iter()
                .map(|&index| symbols[index])
                .filter(|symbol| !invalid.contains(symbol))
                .collect::<BTreeSet<_>>();
            let mut pending = queued.iter().copied().collect::<VecDeque<_>>();
            while let Some(symbol) = pending.pop_front() {
                queued.remove(&symbol);
                if invalid.contains(&symbol) {
                    continue;
                }
                let previous = self.parts.symbol_types.get(&symbol).copied();
                if !self.check_constant_initializer(symbol, &inputs[&symbol])? {
                    invalid.insert(symbol);
                    propagate_invalid(&mut invalid, &reverse, [symbol]);
                    continue;
                }
                let current = self.parts.symbol_types.get(&symbol).copied();
                // Deferred reason changes cannot stabilize an unanchored alias cycle.
                if current != previous && current.is_some_and(|ty| !self.is_deferred(ty)) {
                    for &dependent in reverse.get(&symbol).into_iter().flatten() {
                        if indices
                            .get(&dependent)
                            .is_some_and(|&index| component_of[index] == id)
                            && !invalid.contains(&dependent)
                            && queued.insert(dependent)
                        {
                            pending.push_back(dependent);
                        }
                    }
                }
            }
        }
        // Recover independent qualification diagnostics without using types
        // that depend on a failed signature or initializer.
        for (&symbol, input) in &inputs {
            if dependencies.contains_key(&symbol)
                && invalid.contains(&symbol)
                && !self.constant_prechecked.contains(&symbol)
            {
                let nodes = self.constant_invalid_dependency_nodes(input, &invalid)?;
                self.constant_recovery_nodes.insert(symbol, nodes);
            }
        }
        // A failed value does not invalidate operand types or hide qualification
        // errors. It only blocks dependent evaluation and cycle diagnostics.
        invalid.extend(evaluation_failures.iter().copied());
        propagate_invalid(&mut invalid, &reverse, evaluation_failures);
        // Reverse invalidation reaches an entire component, so surviving
        // cycles can reuse the original partition.
        let mut cycles = components
            .into_iter()
            .filter(|component| {
                let first = component[0];
                !invalid.contains(&symbols[first])
                    && (component.len() > 1 || edges[first].contains(&first))
            })
            .collect::<Vec<_>>();
        cycles.sort_by_key(|cycle| cycle[0]);
        let code = codes::catalog()?.resolve(codes::CONSTANT_DEPENDENCY_CYCLE)?;
        for cycle in cycles {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                code,
                "constant dependency cycle",
                inputs[&symbols[cycle[0]]].name_span,
            )?;
            for &index in &cycle[1..] {
                diagnostic.add_label(
                    self.sources,
                    inputs[&symbols[index]].name_span,
                    "constant participates in this cycle",
                )?;
            }
            self.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    fn check_constant_initializer(
        &mut self,
        symbol: UnitSymbolId,
        input: &Initializer,
    ) -> Result<bool, CompilationUnitTypeError> {
        // Only closed initializer expressions are cached here; receiver namespace prefixes were not evaluated.
        for expression in &input.expressions {
            self.parts.expression_types.remove(expression);
            self.parts.expression_categories.remove(expression);
            self.parts.expression_falls_through.remove(expression);
            self.parts
                .storage_recovery
                .rejected_origins
                .remove(expression);
            self.parts
                .storage_recovery
                .pending_values
                .remove(expression);
            self.parts
                .storage_recovery
                .unpublished_map_writes
                .remove(expression);
            self.parts
                .contextual_argument_requirements
                .remove(expression);
            self.parts.constant_selections.remove(expression);
        }
        self.constant_prechecked.insert(symbol);
        let previous_owner = self.current_owner;
        self.current_owner = input.owner;
        self.checking_constants = true;
        let before = self.diagnostics.len();
        let result = self.check_value_initializer(
            input.expression.source_unit(),
            symbol,
            input.type_ref,
            input.expression.expression(),
        );
        self.checking_constants = false;
        self.current_owner = previous_owner;
        result?;
        Ok(self.diagnostics.len() == before
            && self
                .constant_syntax_dependencies(input.expression, None)?
                .is_ok())
    }

    fn constant_invalid_dependency_nodes(
        &self,
        input: &Initializer,
        invalid: &BTreeSet<UnitSymbolId>,
    ) -> Result<BTreeSet<UnitExpressionId>, CompilationUnitTypeError> {
        let source = input.expression.source_unit();
        let mut affected = BTreeSet::new();
        // The dependency walk records parents before their operands.
        for &expression in input.expressions.iter().rev() {
            let node = self
                .file(source)
                .ast()
                .expressions()
                .get(expression.expression())
                .map_err(TypeCheckingError::from)?;
            let affected_child = |id| affected.contains(&UnitExpressionId::new(source, id));
            let depends_on_invalid = match node.payload() {
                Expression::Group { expression }
                | Expression::Prefix {
                    operand: expression,
                    ..
                } => affected_child(*expression),
                Expression::Binary { left, right, .. } => {
                    affected_child(*left) || affected_child(*right)
                }
                Expression::Name => self
                    .constant_target(source, node.span())
                    .is_some_and(|target| invalid.contains(&target)),
                Expression::Member { name_span, .. } => self
                    .constant_target(source, *name_span)
                    .is_some_and(|target| invalid.contains(&target)),
                _ => false,
            };
            if depends_on_invalid {
                affected.insert(expression);
            }
        }
        Ok(affected)
    }

    /// Qualification shares the dependency walk, including the unevaluated short-circuit RHS.
    pub(super) fn check_constant_expression(
        &mut self,
        symbol: UnitSymbolId,
        root: UnitExpressionId,
    ) -> Result<(), CompilationUnitTypeError> {
        let affected = self.constant_recovery_nodes.get(&symbol);
        if let Err(span) = self.constant_syntax_dependencies(root, affected)? {
            self.emit(
                codes::INVALID_CONSTANT_EXPRESSION,
                "expression is not permitted in a constant initializer",
                span,
            )?;
        }
        Ok(())
    }

    fn constant_operand_allows(
        &self,
        expression: UnitExpressionId,
        affected: Option<&BTreeSet<UnitExpressionId>>,
        accepts: impl Fn(BuiltinType) -> bool,
    ) -> bool {
        if affected.is_some_and(|nodes| nodes.contains(&expression)) {
            return true;
        }
        match self
            .parts
            .expression_types
            .get(&expression)
            .and_then(|ty| self.signatures.types().get(*ty))
        {
            None | Some(UnitTypeKind::Deferred(_) | UnitTypeKind::IntegerLiteral(_)) => true,
            Some(UnitTypeKind::Builtin(ty)) => accepts(*ty),
            _ => false,
        }
    }

    /// Only the closed structural forms participate here. Runtime expressions do not seed SCCs.
    fn constant_syntax_dependencies(
        &self,
        root: UnitExpressionId,
        affected: Option<&BTreeSet<UnitExpressionId>>,
    ) -> Result<Result<SyntaxDependencies, Span>, CompilationUnitTypeError> {
        let mut pending = vec![root.expression()];
        let mut dependencies = BTreeSet::new();
        let mut expressions = Vec::new();
        while let Some(expression) = pending.pop() {
            let id = UnitExpressionId::new(root.source_unit(), expression);
            expressions.push(id);
            let node = self
                .file(root.source_unit())
                .ast()
                .expressions()
                .get(expression)
                .map_err(TypeCheckingError::from)?;
            match node.payload() {
                Expression::Literal(
                    LiteralKind::Boolean(_) | LiteralKind::Integer(_) | LiteralKind::Char,
                ) => {}
                Expression::String { parts }
                    if parts.iter().all(|part| matches!(part, StringPart::Text(_))) => {}
                Expression::Group { expression } => pending.push(*expression),
                Expression::Prefix {
                    operator, operand, ..
                } => {
                    if !self.constant_operand_allows(
                        UnitExpressionId::new(root.source_unit(), *operand),
                        affected,
                        |ty| accepts_prefix_operand(*operator, ty),
                    ) {
                        return Ok(Err(node.span()));
                    }
                    pending.push(*operand);
                }
                Expression::Binary {
                    operator,
                    left,
                    right,
                    ..
                } => {
                    let accepts = |ty| accepts_binary_operand(*operator, ty);
                    if !(accepts(BuiltinType::Int) || accepts(BuiltinType::Boolean))
                        || !self.constant_operand_allows(
                            UnitExpressionId::new(root.source_unit(), *left),
                            affected,
                            accepts,
                        )
                        || !self.constant_operand_allows(
                            UnitExpressionId::new(root.source_unit(), *right),
                            affected,
                            accepts,
                        )
                    {
                        return Ok(Err(node.span()));
                    }
                    pending.push(*right);
                    pending.push(*left);
                }
                Expression::Name | Expression::Member { safe: false, .. } => {
                    let span = match node.payload() {
                        Expression::Member { name_span, .. } => *name_span,
                        _ => node.span(),
                    };
                    let Some(target) = self.constant_target(root.source_unit(), span) else {
                        return Ok(Err(node.span()));
                    };
                    dependencies.insert(target);
                }
                _ => return Ok(Err(node.span())),
            }
        }
        Ok(Ok(SyntaxDependencies {
            dependencies,
            expressions,
        }))
    }
}

fn propagate_invalid(
    invalid: &mut BTreeSet<UnitSymbolId>,
    reverse: &BTreeMap<UnitSymbolId, Vec<UnitSymbolId>>,
    seeds: impl IntoIterator<Item = UnitSymbolId>,
) {
    let mut pending = seeds.into_iter().collect::<Vec<_>>();
    while let Some(symbol) = pending.pop() {
        for &dependent in reverse.get(&symbol).into_iter().flatten() {
            if invalid.insert(dependent) {
                pending.push(dependent);
            }
        }
    }
}
