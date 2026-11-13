//! Independent final validation of actual origins and merge certificates, not AST execution inference.
use super::*;
impl Candidate {
    fn lineage(&self, node: usize, cursor: usize) -> Option<Vec<UnitLoanNestedForwardEdge>> {
        let mut current = node;
        let mut entry = self.actual.get(node)?.entry;
        let mut later = cursor;
        let mut edges = Vec::new();
        while let Some(index) = entry {
            if edges.len() >= 3 {
                return None;
            }
            let e = self.entries.get(index)?;
            let (parent, side) = self.plan.nodes.get(current)?.parent?;
            let Kind::Split {
                split, condition, ..
            } = self.plan.nodes.get(parent)?.kind
            else {
                return None;
            };
            if e.node != current
                || e.edge.split() != split
                || e.edge.condition() != condition
                || e.edge.then_arm() != side
                || e.parent != self.actual.get(parent)?.entry
                || e.cursor > later
                || (current == node && e.cursor != cursor)
                || e.parent.is_some_and(|p| p >= index)
            {
                return None;
            }
            edges.push(e.edge);
            later = e.cursor;
            current = parent;
            entry = e.parent;
        }
        if current != 0 || edges.len() != self.plan.nodes.get(node)?.depth {
            return None;
        }
        edges.reverse();
        Some(edges)
    }
    // Reconstruct only the expected structural postorder. Actual live unions come from tokens.
    fn postorder(&self, node: usize, depth: usize, out: &mut Vec<usize>) -> Option<()> {
        if depth > 3 || out.len() > 7 {
            return None;
        }
        if let Kind::Split { children, .. } = self.plan.nodes.get(node)?.kind {
            self.postorder(children[0], depth + 1, out)?;
            self.postorder(children[1], depth + 1, out)?;
            out.push(node);
        }
        Some(())
    }
}
impl ActionSource {
    pub(super) fn multi_region(
        &self,
        id: usize,
        c: &Candidate,
    ) -> UnitBoundedBorrowMultiContinuationRegion {
        let partial = || UnitBoundedBorrowMultiContinuationRegion {
            identity: self.identity.clone(),
            callable: c.plan.callable,
            root: c.plan.root,
            coverage: UnitLoanActionCoverage::Partial,
            split_count: c.plan.split_count,
            terminal_paths: Vec::new(),
            tail: None,
            origins: Vec::new(),
            joins: Vec::new(),
        };
        if !c.structurally_valid()
            || c.entries.len() != c.plan.nodes.len() - 1
            || c.entries.len() > 14
        {
            return partial();
        }
        let Some(return_count) = c
            .plan
            .leaves
            .len()
            .checked_sub(c.plan.holes.len())
            .and_then(|n| n.checked_add(1))
        else {
            return partial();
        };
        let Some(end) = return_count
            .checked_mul(2)
            .and_then(|n| c.start.checked_add(n))
        else {
            return partial();
        };
        let Some(tail_begin) = end.checked_sub(2) else {
            return partial();
        };
        let mut good = c.valid
            && c.sealed
            && c.closed == Some(end)
            && c.stack.is_empty()
            && c.pending.is_none()
            && c.tail_ready
            && c.tail.stage == Stage::Closed
            && c.tail.begin == tail_begin
            && c.tail.delivery == Some((tail_begin, tail_begin + 1))
            && c.tail.token.is_none();
        for (node, a) in c.actual.iter().enumerate() {
            let state = MultiState {
                candidate: id,
                node,
                entry: a.entry,
                tail: false,
            };
            good &= a.stage == Stage::Closed
                && a.token.as_ref().is_some_and(|t| {
                    Arc::ptr_eq(&t.identity, &self.identity)
                        && t.state == state
                        && t.consumed
                        && t.begin == a.begin
                        && t.begin >= c.start
                        && t.end >= t.begin
                        && t.end <= tail_begin
                        && valid_origins(&t.outcome, c.plan.holes.len())
                        && match c.plan.nodes[node].kind {
                            Kind::Return { .. } => {
                                matches!(t.outcome, Outcome::Closed)
                                    && t.begin.checked_add(2) == Some(t.end)
                            }
                            Kind::Hole => {
                                c.plan
                                    .holes
                                    .iter()
                                    .position(|h| *h == node)
                                    .is_some_and(|origin| {
                                        t.begin == t.end
                                            && t.outcome
                                                == Outcome::Continues {
                                                    origins: vec![origin],
                                                }
                                    })
                            }
                            Kind::Split { children, .. } => c.actual[children[0]]
                                .token
                                .as_ref()
                                .zip(c.actual[children[1]].token.as_ref())
                                .is_some_and(|(left, right)| {
                                    let incoming = [
                                        left.outcome.origins().to_vec(),
                                        right.outcome.origins().to_vec(),
                                    ];
                                    let Some(origins) = union(&incoming, c.plan.holes.len()) else {
                                        return false;
                                    };
                                    left.begin == t.begin
                                        && left.end == right.begin
                                        && right.end == t.end
                                        && if origins.is_empty() {
                                            matches!(t.outcome, Outcome::Closed)
                                        } else {
                                            t.outcome == Outcome::Continues { origins }
                                        }
                                }),
                        }
                });
        }
        let all_origins = (0..c.plan.holes.len()).collect::<Vec<_>>();
        good &= c
            .actual
            .first()
            .and_then(|a| a.token.as_ref())
            .is_some_and(|t| {
                t.begin == c.start
                    && t.end == tail_begin
                    && t.outcome
                        == Outcome::Continues {
                            origins: all_origins,
                        }
            });
        let mut postorder = Vec::new();
        if c.postorder(0, 0, &mut postorder).is_none() {
            return partial();
        }
        let live = postorder
            .into_iter()
            .filter(|node| {
                c.actual[*node]
                    .token
                    .as_ref()
                    .is_some_and(|t| !matches!(t.outcome, Outcome::Closed))
            })
            .collect::<Vec<_>>();
        if live.len() != c.resumptions.len() {
            return partial();
        }
        let mut joins = Vec::new();
        let mut previous = c.start;
        for (node, r) in live.into_iter().zip(&c.resumptions) {
            let Kind::Split {
                split,
                condition,
                children,
                ..
            } = c.plan.nodes[node].kind
            else {
                return partial();
            };
            let Some(left) = c.actual[children[0]].token.as_ref() else {
                return partial();
            };
            let Some(right) = c.actual[children[1]].token.as_ref() else {
                return partial();
            };
            let incoming = [
                left.outcome.origins().to_vec(),
                right.outcome.origins().to_vec(),
            ];
            let Some(origins) = union(&incoming, c.plan.holes.len()) else {
                return partial();
            };
            let survivors = [left, right]
                .iter()
                .filter(|t| !matches!(t.outcome, Outcome::Closed))
                .map(|t| t.state)
                .collect::<Vec<_>>();
            good &= r.parent
                == MultiState {
                    candidate: id,
                    node,
                    entry: c.actual[node].entry,
                    tail: false,
                }
                && r.survivors == survivors
                && r.incoming == incoming
                && r.origins == origins
                && !origins.is_empty()
                && r.cursor >= previous
                && r.cursor <= tail_begin
                && c.actual[node]
                    .token
                    .as_ref()
                    .is_some_and(|t| t.end == r.cursor);
            previous = r.cursor;
            joins.push(UnitBoundedBorrowMultiJoin {
                split,
                condition,
                incoming,
                origins,
            });
        }
        good &= previous == tail_begin;
        let mut terminal_paths = Vec::new();
        let mut origins = Vec::new();
        let mut ordinal = 0;
        for &node in &c.plan.leaves {
            let begin = c.start + ordinal * 2;
            let Some(edges) = c.lineage(node, begin) else {
                return partial();
            };
            match c.plan.nodes[node].kind {
                Kind::Hole => {
                    good &= c.plan.holes.get(origins.len()) == Some(&node)
                        && c.actual[node].begin == begin;
                    origins.push(UnitBoundedBorrowTailOrigin { edges });
                }
                Kind::Return {
                    control,
                    value,
                    argument,
                } => {
                    good &= c.actual[node].delivery == Some((begin, begin + 1))
                        && self.multi_delivery(c.plan.root, control, value, argument, begin);
                    terminal_paths.push(UnitBoundedBorrowForwardPath {
                        identity: self.identity.clone(),
                        callable: c.plan.callable,
                        root: c.plan.root,
                        control,
                        value,
                        begin,
                        handoff: begin + 1,
                        edges,
                    });
                    ordinal += 1;
                }
                _ => return partial(),
            }
        }
        good &= ordinal + 1 == return_count
            && origins.len() == c.plan.holes.len()
            && self.multi_delivery(
                c.plan.root,
                c.plan.tail.control,
                c.plan.tail.value,
                c.plan.tail.argument,
                tail_begin,
            );
        if !good {
            return partial();
        }
        UnitBoundedBorrowMultiContinuationRegion {
            identity: self.identity.clone(),
            callable: c.plan.callable,
            root: c.plan.root,
            coverage: UnitLoanActionCoverage::Complete,
            split_count: c.plan.split_count,
            terminal_paths,
            origins,
            joins,
            tail: Some(UnitBoundedBorrowSharedTail {
                identity: self.identity.clone(),
                callable: c.plan.callable,
                root: c.plan.root,
                control: c.plan.tail.control,
                value: c.plan.tail.value,
                begin: tail_begin,
                handoff: tail_begin + 1,
            }),
        }
    }
    fn multi_delivery(
        &self,
        root: crate::name_resolution::UnitSymbolId,
        control: UnitExpressionId,
        value: UnitExpressionId,
        argument: UnitExpressionId,
        begin: usize,
    ) -> bool {
        matches!((self.records.get(begin),self.records.get(begin+1)),(Some(Record::Begin(b)),Some(Record::BorrowForwardHandoff{transaction,..}))
            if b==transaction.source_loan()&&b.call()==value&&b.argument()==argument&&b.kind()==LoanKind::Shared
            &&matches!(b.target(),UnitLoanTarget::Place(p) if p.is_root()&&p.root()==root)
            &&transaction.returned_expression()==value&&transaction.point()==UnitDropPoint::ControlTransfer(control))
            &&self.records.iter().filter(|r|matches!(r,Record::Begin(b) if b.call()==value&&b.argument()==argument)).count()==1
            &&self.records.iter().filter(|r|matches!(r,Record::BorrowForwardHandoff{transaction,..} if transaction.returned_expression()==value&&transaction.point()==UnitDropPoint::ControlTransfer(control))).count()==1
    }
}
