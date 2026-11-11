//! Actual DFS terminal closure, separate from the bounded declaration tree.
use super::super::bounded_forward_plan::{Kind, Node, Plan};
use super::{ActionHandle, ActionSource, Record};
use crate::{
    ownership_checking::{
        LoanKind, UnitBoundedBorrowForwardPath, UnitBoundedBorrowForwardRegion, UnitDropPoint,
        UnitLoanActionCoverage, UnitLoanNestedForwardEdge, UnitLoanTarget,
    },
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct BoundedState {
    pub(in super::super) candidate: usize,
    pub(in super::super) node: usize,
    pub(in super::super) entry: Option<usize>,
}
#[derive(Clone, Debug)]
pub(in super::super) struct BoundedCause {
    pub(in super::super) acquisition: ActionHandle,
    pub(in super::super) state: BoundedState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Awaiting,
    ConditionRead,
    ConditionReady,
    ThenOpen,
    ThenClosed,
    ElseOpen,
    ReadyJoin,
    LeafLive,
    Closed,
}
#[derive(Clone, Debug)]
struct Closed {
    identity: Arc<()>,
    candidate: usize,
    node: usize,
    entry: Option<usize>,
    begin: usize,
    end: usize,
    consumed: bool,
}
#[derive(Clone, Debug)]
struct ActualNode {
    stage: Stage,
    entry: Option<usize>,
    begin: usize,
    token: Option<Closed>,
    delivery: Option<(usize, usize)>,
}
#[derive(Clone, Debug)]
struct Entry {
    node: usize,
    parent: Option<usize>,
    edge: UnitLoanNestedForwardEdge,
    cursor: usize,
}
#[derive(Clone, Debug)]
pub(super) struct Candidate {
    plan: Plan,
    base: (usize, usize),
    start: usize,
    valid: bool,
    stack: Vec<usize>,
    actual: Vec<ActualNode>,
    entries: Vec<Entry>,
    sealed: bool,
    closed: Option<usize>,
}
impl Candidate {
    fn current(&self, state: BoundedState) -> bool {
        self.valid
            && self.plan.valid_structure()
            && self.actual.len() == self.plan.nodes.len()
            && !self.sealed
            && self.closed.is_none()
            && self.stack.last() == Some(&state.node)
            && self
                .actual
                .get(state.node)
                .is_some_and(|a| a.entry == state.entry)
    }
    fn close_node(&mut self, state: BoundedState, cursor: usize, identity: &Arc<()>) {
        if !self.current(state) {
            self.valid = false;
            return;
        }
        let a = &mut self.actual[state.node];
        if a.token.is_some() {
            self.valid = false;
            return;
        }
        a.stage = Stage::Closed;
        a.token = Some(Closed {
            identity: identity.clone(),
            candidate: state.candidate,
            node: state.node,
            entry: state.entry,
            begin: a.begin,
            end: cursor,
            consumed: false,
        });
        self.stack.pop();
        if let Some((parent, side)) = self.plan.nodes[state.node].parent {
            if self.stack.last() != Some(&parent) {
                self.valid = false;
                return;
            }
            let stage = &mut self.actual[parent].stage;
            let expected = if side {
                Stage::ThenOpen
            } else {
                Stage::ElseOpen
            };
            if *stage != expected {
                self.valid = false;
                return;
            }
            *stage = if side {
                Stage::ThenClosed
            } else {
                Stage::ReadyJoin
            };
        } else if !self.stack.is_empty() {
            self.valid = false;
        }
    }
}
impl ActionSource {
    pub(in super::super) fn begin_bounded(
        &mut self,
        plan: Plan,
        base: (usize, usize),
        clear: bool,
    ) -> BoundedState {
        let candidate = self.bounded_regions.len();
        let unique = !self
            .bounded_regions
            .iter()
            .any(|c| c.plan.callable == plan.callable);
        let plan_valid = plan.valid_structure();
        let actual = plan
            .nodes
            .iter()
            .map(|_| ActualNode {
                stage: Stage::Awaiting,
                entry: None,
                begin: self.records.len(),
                token: None,
                delivery: None,
            })
            .collect();
        self.bounded_regions.push(Candidate {
            plan,
            base,
            start: self.records.len(),
            valid: self.enabled && clear && unique && plan_valid,
            stack: vec![0],
            actual,
            entries: Vec::new(),
            sealed: false,
            closed: None,
        });
        BoundedState {
            candidate,
            node: 0,
            entry: None,
        }
    }
    pub(in super::super) fn bounded_node(
        &self,
        state: BoundedState,
    ) -> Option<(Node, (usize, usize))> {
        let c = self.bounded_regions.get(state.candidate)?;
        Some((*c.plan.nodes.get(state.node)?, c.base))
    }
    pub(in super::super) fn bounded_source_matches(
        &self,
        state: BoundedState,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        root: crate::name_resolution::UnitSymbolId,
    ) -> bool {
        self.bounded_regions.get(state.candidate).is_some_and(|c| c.current(state) && c.plan.root == root
            && matches!(c.plan.nodes[state.node].kind, Kind::Leaf { value, argument: wanted, .. } if value==call && wanted==argument))
    }
    pub(in super::super) fn bounded_condition(
        &mut self,
        state: BoundedState,
        split: UnitExpressionId,
        condition: UnitExpressionId,
        after: bool,
        clear: bool,
    ) {
        let Some(c) = self.bounded_regions.get_mut(state.candidate) else {
            return;
        };
        let good = c.current(state)
            && clear
            && matches!(c.plan.nodes[state.node].kind, Kind::Split { split: s, condition: q, .. } if s==split && q==condition)
            && c.actual[state.node].stage
                == if after {
                    Stage::ConditionRead
                } else {
                    Stage::Awaiting
                }
            && c.actual[state.node].begin == self.records.len();
        c.valid &= good;
        if c.valid {
            c.actual[state.node].stage = if after {
                Stage::ConditionReady
            } else {
                Stage::ConditionRead
            };
        }
    }
    pub(in super::super) fn bounded_arm(
        &mut self,
        state: BoundedState,
        split: UnitExpressionId,
        side: bool,
        clear: bool,
    ) -> Option<BoundedState> {
        let c = self.bounded_regions.get_mut(state.candidate)?;
        let Kind::Split {
            split: wanted,
            condition,
            children,
            ..
        } = c.plan.nodes.get(state.node)?.kind
        else {
            c.valid = false;
            return None;
        };
        let child = children[usize::from(!side)];
        let good = c.current(state)
            && clear
            && split == wanted
            && c.actual[state.node].stage
                == if side {
                    Stage::ConditionReady
                } else {
                    Stage::ThenClosed
                }
            && c.plan
                .nodes
                .get(child)
                .is_some_and(|n| n.parent == Some((state.node, side)))
            && c.actual
                .get(child)
                .is_some_and(|a| a.stage == Stage::Awaiting && a.entry.is_none());
        c.valid &= good;
        if !c.valid {
            return None;
        }
        let entry = c.entries.len();
        c.entries.push(Entry {
            node: child,
            parent: state.entry,
            edge: UnitLoanNestedForwardEdge {
                split,
                condition,
                then_arm: side,
            },
            cursor: self.records.len(),
        });
        c.actual[state.node].stage = if side {
            Stage::ThenOpen
        } else {
            Stage::ElseOpen
        };
        c.actual[child].entry = Some(entry);
        c.actual[child].begin = self.records.len();
        c.stack.push(child);
        Some(BoundedState {
            candidate: state.candidate,
            node: child,
            entry: Some(entry),
        })
    }
    pub(in super::super) fn record_bounded_handoff(
        &mut self,
        cause: Option<BoundedCause>,
        completed: Option<BoundedState>,
    ) {
        let Some(cause) = cause.filter(|c| Some(c.state) == completed) else {
            return;
        };
        let Some(c) = self.bounded_regions.get_mut(cause.state.candidate) else {
            return;
        };
        let state = cause.state;
        let index = self.records.len().saturating_sub(1);
        let good = c.current(state)
            && c.actual[state.node].stage == Stage::LeafLive
            && Arc::ptr_eq(&cause.acquisition.identity, &self.identity)
            && cause.acquisition.index.checked_add(1) == Some(index)
            && c.actual[state.node].begin == cause.acquisition.index
            && c.actual[state.node].delivery.is_none()
            && matches!((self.records.get(cause.acquisition.index), self.records.get(index)), (Some(Record::Begin(begin)),Some(Record::BorrowForwardHandoff { transaction,.. })) if begin==transaction.source_loan());
        c.valid &= good;
        if c.valid {
            c.actual[state.node].delivery = Some((cause.acquisition.index, index));
        }
    }
    pub(in super::super) fn bounded_return(
        &mut self,
        state: BoundedState,
        control: UnitExpressionId,
        post: bool,
        clear: bool,
    ) {
        let Some(c) = self.bounded_regions.get_mut(state.candidate) else {
            return;
        };
        let Some(Node {
            kind:
                Kind::Leaf {
                    control: wanted,
                    value,
                    argument,
                },
            ..
        }) = c.plan.nodes.get(state.node).copied()
        else {
            c.valid = false;
            return;
        };
        let a = &c.actual[state.node];
        let good = c.current(state)
            && clear
            && control == wanted
            && a.stage
                == if post {
                    Stage::LeafLive
                } else {
                    Stage::Awaiting
                }
            && a.begin.checked_add(if post { 2 } else { 1 }) == Some(self.records.len())
            && matches!(self.records.get(a.begin),Some(Record::Begin(begin)) if begin.call()==value && begin.argument()==argument && begin.kind()==LoanKind::Shared && matches!(begin.target(),UnitLoanTarget::Place(p) if p.is_root()&&p.root()==c.plan.root))
            && (!post
                || (a.delivery == Some((a.begin, a.begin + 1))
                    && matches!(self.records.get(a.begin+1),Some(Record::BorrowForwardHandoff{transaction,..}) if transaction.returned_expression()==value && transaction.point()==UnitDropPoint::ControlTransfer(control))));
        c.valid &= good;
        if !c.valid {
            return;
        }
        if post {
            c.close_node(state, self.records.len(), &self.identity);
        } else {
            c.actual[state.node].stage = Stage::LeafLive;
        }
    }
    pub(in super::super) fn bounded_join(
        &mut self,
        state: BoundedState,
        split: UnitExpressionId,
        both: bool,
        empty: bool,
        clear: bool,
    ) {
        let Some(c) = self.bounded_regions.get_mut(state.candidate) else {
            return;
        };
        let Some(Node {
            kind:
                Kind::Split {
                    split: wanted,
                    children,
                    ..
                },
            ..
        }) = c.plan.nodes.get(state.node).copied()
        else {
            c.valid = false;
            return;
        };
        let token = |child: usize| c.actual.get(child).and_then(|a| a.token.as_ref());
        let good = c.current(state)
            && clear
            && both
            && empty
            && split == wanted
            && c.actual[state.node].stage == Stage::ReadyJoin
            && children.iter().enumerate().all(|(i, &child)| {
                token(child).is_some_and(|t| {
                    Arc::ptr_eq(&t.identity, &self.identity)
                        && t.candidate == state.candidate
                        && t.node == child
                        && !t.consumed
                        && t.entry == c.actual[child].entry
                        && c.plan.nodes[child].parent == Some((state.node, i == 0))
                        && t.entry
                            .and_then(|e| c.entries.get(e))
                            .is_some_and(|e| e.parent == state.entry && e.node == child)
                })
            })
            && token(children[0])
                .zip(token(children[1]))
                .is_some_and(|(a, b)| {
                    a.begin == c.actual[state.node].begin
                        && a.end == b.begin
                        && b.end == self.records.len()
                });
        c.valid &= good;
        if !c.valid {
            return;
        }
        for child in children {
            c.actual[child]
                .token
                .as_mut()
                .expect("checked token")
                .consumed = true;
        }
        c.close_node(state, self.records.len(), &self.identity);
    }
    pub(in super::super) fn seal_bounded(
        &mut self,
        state: BoundedState,
        terminal: bool,
        clear: bool,
    ) {
        let Some(c) = self.bounded_regions.get_mut(state.candidate) else {
            return;
        };
        c.valid &= terminal
            && clear
            && state.node == 0
            && state.entry.is_none()
            && !c.sealed
            && c.stack.is_empty()
            && c.actual
                .first()
                .and_then(|a| a.token.as_ref())
                .is_some_and(|t| !t.consumed && t.begin == c.start && t.end == self.records.len());
        if c.valid {
            c.sealed = true;
        }
    }
    pub(in super::super) fn close_bounded(&mut self, callable: UnitItemId) {
        for c in self
            .bounded_regions
            .iter_mut()
            .filter(|c| c.plan.callable == callable)
        {
            c.valid &= c.sealed
                && c.closed.is_none()
                && c.actual
                    .first()
                    .and_then(|a| a.token.as_ref())
                    .is_some_and(|t| t.end == self.records.len());
            if c.valid {
                c.closed = Some(self.records.len());
            }
        }
    }
    pub(super) fn finish_bounded(&self) -> Vec<UnitBoundedBorrowForwardRegion> {
        #[cfg(test)]
        tests::record(self);
        self.bounded_regions
            .iter()
            .enumerate()
            .map(|(id, c)| self.bounded_region(id, c))
            .collect()
    }
    fn bounded_region(&self, id: usize, c: &Candidate) -> UnitBoundedBorrowForwardRegion {
        if !c.plan.valid_structure() || c.actual.len() != c.plan.nodes.len() || c.entries.len() > 14
        {
            return UnitBoundedBorrowForwardRegion {
                identity: self.identity.clone(),
                callable: c.plan.callable,
                root: c.plan.root,
                coverage: UnitLoanActionCoverage::Partial,
                split_count: c.plan.split_count,
                paths: Vec::new(),
            };
        }
        let Some(end) = c.start.checked_add(c.plan.leaves.len() * 2) else {
            return UnitBoundedBorrowForwardRegion {
                identity: self.identity.clone(),
                callable: c.plan.callable,
                root: c.plan.root,
                coverage: UnitLoanActionCoverage::Partial,
                split_count: c.plan.split_count,
                paths: Vec::new(),
            };
        };
        let mut paths = Vec::new();
        let mut good = c.valid
            && c.sealed
            && c.stack.is_empty()
            && c.closed == Some(end)
            && c.plan.nodes.len() == c.actual.len()
            && c.entries.len() == c.plan.nodes.len() - 1
            && c.actual.iter().enumerate().all(|(i, a)| {
                a.stage == Stage::Closed
                    && a.token.as_ref().is_some_and(|t| {
                        Arc::ptr_eq(&t.identity, &self.identity)
                            && t.candidate == id
                            && t.node == i
                            && t.entry == a.entry
                            && t.consumed == (i != 0)
                            && t.begin == a.begin
                            && t.begin >= c.start
                            && t.end >= t.begin
                            && Some(t.end) <= c.closed
                            && match c.plan.nodes[i].kind {
                                Kind::Leaf { .. } => t.begin.checked_add(2) == Some(t.end),
                                Kind::Split { children, .. } => c.actual[children[0]]
                                    .token
                                    .as_ref()
                                    .zip(c.actual[children[1]].token.as_ref())
                                    .is_some_and(|(left, right)| {
                                        left.begin == t.begin
                                            && left.end == right.begin
                                            && right.end == t.end
                                    }),
                            }
                    })
            });
        for (ordinal, &node) in c.plan.leaves.iter().enumerate() {
            let Kind::Leaf {
                control,
                value,
                argument,
            } = c.plan.nodes[node].kind
            else {
                good = false;
                continue;
            };
            let actual = &c.actual[node];
            let begin = c.start + ordinal * 2;
            good &= actual.delivery == Some((begin, begin + 1))
                && matches!((self.records.get(begin),self.records.get(begin+1)),(Some(Record::Begin(b)),Some(Record::BorrowForwardHandoff{transaction,..}))
                        if b==transaction.source_loan() && b.call()==value && b.argument()==argument && b.kind()==LoanKind::Shared
                        && matches!(b.target(),UnitLoanTarget::Place(p) if p.is_root()&&p.root()==c.plan.root)
                        && transaction.returned_expression()==value && transaction.point()==UnitDropPoint::ControlTransfer(control));
            // A duplicate raw delivery is invalid even when it has no observation sidecar.
            // This only proves uniqueness of this exact admitted call/Return, not global absence.
            good &= self.records.iter().filter(|record| {
                matches!(record, Record::Begin(loan) if loan.call() == value && loan.argument() == argument)
            }).count() == 1;
            good &= self
                .records
                .iter()
                .filter(|record| {
                    matches!(record, Record::BorrowForwardHandoff { transaction, .. }
                    if transaction.returned_expression() == value
                        && transaction.point() == UnitDropPoint::ControlTransfer(control))
                })
                .count()
                == 1;
            let mut edges = Vec::new();
            let mut entry = actual.entry;
            let mut current = node;
            let mut later = begin;
            let mut steps = 0;
            while let Some(index) = entry {
                steps += 1;
                if steps > 3 {
                    good = false;
                    break;
                }
                let Some(e) = c.entries.get(index) else {
                    good = false;
                    break;
                };
                let Some((parent, side)) = c.plan.nodes[current].parent else {
                    good = false;
                    break;
                };
                let Kind::Split {
                    split, condition, ..
                } = c.plan.nodes[parent].kind
                else {
                    good = false;
                    break;
                };
                good &= e.node == current
                    && e.edge.split() == split
                    && e.edge.condition() == condition
                    && e.edge.then_arm() == side
                    && e.parent == c.actual[parent].entry
                    && e.cursor <= later
                    && (current != node || e.cursor == begin)
                    && e.parent.is_none_or(|p| p < index);
                edges.push(e.edge);
                later = e.cursor;
                current = parent;
                entry = e.parent;
            }
            good &= current == 0 && edges.len() == c.plan.nodes[node].depth;
            edges.reverse();
            paths.push(UnitBoundedBorrowForwardPath {
                identity: self.identity.clone(),
                callable: c.plan.callable,
                root: c.plan.root,
                control,
                value,
                begin,
                handoff: begin + 1,
                edges,
            });
        }
        UnitBoundedBorrowForwardRegion {
            identity: self.identity.clone(),
            callable: c.plan.callable,
            root: c.plan.root,
            coverage: if good {
                UnitLoanActionCoverage::Complete
            } else {
                UnitLoanActionCoverage::Partial
            },
            split_count: c.plan.split_count,
            paths: if good { paths } else { Vec::new() },
        }
    }
}

#[cfg(test)]
mod tests;
