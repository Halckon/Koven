/// Iterative Kosaraju: input indices are built from this analysis's symbol table, never source integers.
pub(super) fn cyclic_components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
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
