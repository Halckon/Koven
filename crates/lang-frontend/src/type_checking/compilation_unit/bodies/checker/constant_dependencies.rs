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
        constant_graph::cyclic_components,
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
        if !self.signatures.diagnostics().is_empty() {
            return Ok(());
        }
        let mut inputs = BTreeMap::new();
        for source in self.names.names().index().source_units() {
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
            if let Ok(syntax) = self.constant_syntax_dependencies(input.expression)? {
                input.expressions = syntax.expressions;
                dependencies.insert(symbol, syntax.dependencies);
            }
        }
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
        // A dependency must establish its type eligibility before consumers are checked.
        // Cyclic nodes are handled by the concrete-type worklist below.
        self.constant_prechecked
            .extend(dependencies.keys().copied());
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
        while let Some(symbol) = ready.pop_front() {
            if !invalid.contains(&symbol) {
                let valid = self.check_constant_initializer(symbol, &inputs[&symbol])?;
                let value = if valid {
                    crate::type_checking::constant_evaluation::evaluate(
                        self,
                        inputs[&symbol].expression,
                    )?
                } else {
                    None
                };
                if let Some(value) = value {
                    self.constant_values.insert(symbol, value);
                } else {
                    invalid.insert(symbol);
                    propagate_invalid(&mut invalid, &reverse, [symbol]);
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
        // Cyclic nodes never become topologically ready. Propagate newly concrete types
        // through their dependents before deciding which syntax cycles remain valid.
        let mut queued = remaining
            .iter()
            .filter_map(|(&symbol, &count)| {
                (count > 0 && !invalid.contains(&symbol)).then_some(symbol)
            })
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
            // Deferred reason changes convey no new operand eligibility and must not
            // keep an unanchored alias cycle alive in this worklist.
            if current != previous && current.is_some_and(|ty| !self.is_deferred(ty)) {
                for &dependent in reverse.get(&symbol).into_iter().flatten() {
                    if remaining[&dependent] > 0
                        && !invalid.contains(&dependent)
                        && queued.insert(dependent)
                    {
                        pending.push_back(dependent);
                    }
                }
            }
        }
        let symbols = inputs.keys().copied().collect::<Vec<_>>();
        let indices = symbols
            .iter()
            .enumerate()
            .map(|(index, &symbol)| (symbol, index))
            .collect::<BTreeMap<_, _>>();
        let mut edges = vec![Vec::new(); symbols.len()];
        for (&symbol, targets) in &dependencies {
            if invalid.contains(&symbol) {
                continue;
            }
            for target in targets {
                edges[indices[&symbol]].push(
                    *indices
                        .get(target)
                        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?,
                );
            }
        }
        let mut cycles = cyclic_components(&edges);
        for cycle in &mut cycles {
            cycle.sort_unstable();
        }
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
            self.parts.constant_selections.remove(expression);
        }
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
            && self.constant_syntax_dependencies(input.expression)?.is_ok())
    }

    /// Qualification shares the dependency walk, including the unevaluated short-circuit RHS.
    pub(super) fn check_constant_expression(
        &mut self,
        root: UnitExpressionId,
    ) -> Result<(), CompilationUnitTypeError> {
        if let Err(span) = self.constant_syntax_dependencies(root)? {
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
        accepts: impl Fn(BuiltinType) -> bool,
    ) -> bool {
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
    ) -> Result<Result<SyntaxDependencies, Span>, CompilationUnitTypeError> {
        let mut pending = vec![root.expression()];
        let mut dependencies = BTreeSet::new();
        let mut expressions = Vec::new();
        while let Some(expression) = pending.pop() {
            expressions.push(UnitExpressionId::new(root.source_unit(), expression));
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
                            accepts,
                        )
                        || !self.constant_operand_allows(
                            UnitExpressionId::new(root.source_unit(), *right),
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
