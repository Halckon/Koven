//! 有错误依赖时先传播失效；其余语法图每个 SCC 最多发布一次环诊断。

use super::super::*;

impl Checker<'_> {
    /// Resolve forward operand types before admitting expressions to the cycle graph.
    pub(in crate::type_checking::checker) fn recheck_constant_dependencies(
        &mut self,
    ) -> Result<(), TypeCheckingError> {
        let mut remaining = vec![0; self.symbol_kinds.len()];
        let mut reverse = vec![Vec::new(); self.symbol_kinds.len()];
        let mut ready = std::collections::VecDeque::new();
        for (&symbol, dependencies) in &self.constant_dependencies {
            remaining[symbol.index()] = dependencies.len();
            if dependencies.is_empty() {
                ready.push_back(symbol);
            }
            for dependency in dependencies {
                reverse[dependency.index()].push(symbol);
            }
        }
        let invalid = self.prune_invalid_constant_dependencies(&reverse);
        while let Some(symbol) = ready.pop_front() {
            self.pending_constant_errors.remove(&symbol);
            let dependencies = self.constant_dependencies.remove(&symbol);
            // Invalid predecessors suppress dependent errors; cyclic nodes remain for SCC analysis.
            if dependencies.as_ref().is_some_and(|dependencies| {
                dependencies
                    .iter()
                    .all(|dependency| self.constant_dependencies.contains_key(dependency))
            }) && let Some(&item) = self.constant_items.get(&symbol)
            {
                if let Some(expressions) = self.constant_expressions.get(&symbol) {
                    for expression in expressions {
                        self.expression_types[expression.index()] = None;
                    }
                }
                self.rechecking_constants = true;
                let result = self.check_item(item);
                self.rechecking_constants = false;
                result?;
            }
            for &dependent in &reverse[symbol.index()] {
                if invalid.contains(&dependent) {
                    continue;
                }
                remaining[dependent.index()] -= 1;
                if remaining[dependent.index()] == 0 {
                    ready.push_back(dependent);
                }
            }
        }
        self.prune_invalid_constant_dependencies(&reverse);
        // Unresolved cyclic initializers still report their first forbidden expression before SCCs.
        for (symbol, error) in std::mem::take(&mut self.pending_constant_errors) {
            self.constant_dependencies.remove(&symbol);
            self.emit_constant_expression_error(error)?;
        }
        Ok(())
    }

    fn prune_invalid_constant_dependencies(
        &mut self,
        reverse: &[Vec<SymbolId>],
    ) -> BTreeSet<SymbolId> {
        // Propagate through cycles too: waiting for all predecessors would retain cascading errors.
        let mut invalid = self
            .constant_dependencies
            .values()
            .flatten()
            .copied()
            .filter(|dependency| !self.constant_dependencies.contains_key(dependency))
            .collect::<BTreeSet<_>>();
        let mut invalid_pending = invalid.iter().copied().collect::<Vec<_>>();
        while let Some(symbol) = invalid_pending.pop() {
            self.constant_dependencies.remove(&symbol);
            self.pending_constant_errors.remove(&symbol);
            for &dependent in &reverse[symbol.index()] {
                if invalid.insert(dependent) {
                    invalid_pending.push(dependent);
                }
            }
        }
        invalid
    }

    pub(in crate::type_checking::checker) fn check_constant_cycles(
        &mut self,
    ) -> Result<(), TypeCheckingError> {
        let count = self.symbol_kinds.len();
        let mut edges = vec![Vec::new(); count];
        let mut reverse = vec![Vec::new(); count];
        let mut invalid = vec![true; count];
        for (symbol, dependencies) in &self.constant_dependencies {
            invalid[symbol.index()] = false;
            for target in dependencies {
                edges[symbol.index()].push(target.index());
                reverse[target.index()].push(symbol.index());
            }
        }
        // 一次队列传播，避免长链在逐轮扫描中退化为平方复杂度。
        let mut pending = invalid
            .iter()
            .enumerate()
            .filter_map(|(index, invalid)| invalid.then_some(index))
            .collect::<Vec<_>>();
        while let Some(node) = pending.pop() {
            for &dependent in &reverse[node] {
                if !invalid[dependent] {
                    invalid[dependent] = true;
                    pending.push(dependent);
                }
            }
        }
        for (node, successors) in edges.iter_mut().enumerate() {
            if invalid[node] {
                successors.clear();
            }
        }
        let mut cycles = cyclic_components(&edges);
        for cycle in &mut cycles {
            cycle.sort_by_key(|node| self.symbol_spans[*node].start());
        }
        cycles.sort_by_key(|cycle| self.symbol_spans[cycle[0]].start());
        for cycle in cycles {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.constant_cycle_code,
                "constant dependency cycle",
                self.symbol_spans[cycle[0]],
            )?;
            for node in &cycle[1..] {
                diagnostic.add_label(
                    self.sources,
                    self.symbol_spans[*node],
                    "constant participates in this cycle",
                )?;
            }
            self.diagnostics.push(diagnostic);
        }
        Ok(())
    }
}

/// Iterative Kosaraju: input indices are built from this analysis's symbol table, never source integers.
fn cyclic_components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut visited = vec![false; edges.len()];
    let mut order = Vec::with_capacity(edges.len());
    for root in 0..edges.len() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((node, next)) = stack.last_mut() {
            if let Some(&successor) = edges[*node].get(*next) {
                *next += 1;
                if !visited[successor] {
                    visited[successor] = true;
                    stack.push((successor, 0));
                }
            } else {
                order.push(*node);
                stack.pop();
            }
        }
    }
    let mut reverse = vec![Vec::new(); edges.len()];
    for (node, successors) in edges.iter().enumerate() {
        for &successor in successors {
            reverse[successor].push(node);
        }
    }
    visited.fill(false);
    let mut cycles = Vec::new();
    for root in order.into_iter().rev() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut pending = vec![root];
        let mut component = Vec::new();
        while let Some(node) = pending.pop() {
            component.push(node);
            for &predecessor in &reverse[node] {
                if !visited[predecessor] {
                    visited[predecessor] = true;
                    pending.push(predecessor);
                }
            }
        }
        if component.len() > 1 || edges[root].contains(&root) {
            cycles.push(component);
        }
    }
    cycles
}
