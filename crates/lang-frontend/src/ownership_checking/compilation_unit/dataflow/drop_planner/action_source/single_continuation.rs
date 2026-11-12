//! Actual bounded single-survivor closure. Live continuation is never a terminal token.
use super::super::loan_bounded_forward_region::single_continuation_plan::{Kind, Node, Plan};
use super::{ActionHandle, ActionSource, Record};
use crate::{
    ownership_checking::{
        LoanKind, UnitBoundedBorrowContinuation, UnitBoundedBorrowContinuationRegion,
        UnitBoundedBorrowForwardPath, UnitDropPoint, UnitLoanActionCoverage,
        UnitLoanNestedForwardEdge, UnitLoanTarget,
    },
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct SingleState {
    pub(in super::super) candidate: usize,
    pub(in super::super) node: usize,
    pub(in super::super) entry: Option<usize>,
    pub(in super::super) tail: bool,
}
#[derive(Clone, Debug)]
pub(in super::super) struct SingleCause {
    pub(in super::super) acquisition: ActionHandle,
    pub(in super::super) state: SingleState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct SingleResume {
    identity: Arc<()>,
    candidate: usize,
    parent: SingleState,
    survivor: SingleState,
    cursor: usize,
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
    PendingResume,
    Closed,
}
#[derive(Clone, Debug)]
struct Token {
    identity: Arc<()>,
    state: SingleState,
    begin: usize,
    end: usize,
    consumed: bool,
    outcome: Outcome,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Closed,
    Continues,
}
#[derive(Clone, Debug)]
struct ActualNode {
    stage: Stage,
    entry: Option<usize>,
    begin: usize,
    token: Option<Token>,
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
struct Resumption {
    parent: SingleState,
    survivor: SingleState,
    cursor: usize,
    edge: UnitLoanNestedForwardEdge,
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
    pending: Option<SingleResume>,
    resumptions: Vec<Resumption>,
    tail: ActualNode,
    tail_ready: bool,
    sealed: bool,
    closed: Option<usize>,
}
fn fresh(cursor: usize) -> ActualNode {
    ActualNode {
        stage: Stage::Awaiting,
        entry: None,
        begin: cursor,
        token: None,
        delivery: None,
    }
}
impl Candidate {
    fn structurally_valid(&self) -> bool {
        self.plan.valid_structure() && self.actual.len() == self.plan.nodes.len()
    }
    fn current(&self, state: SingleState) -> bool {
        self.valid
            && self.structurally_valid()
            && !self.sealed
            && self.closed.is_none()
            && self.pending.is_none()
            && if state.tail {
                self.tail_ready
                    && state.node == self.plan.hole
                    && state.entry == self.actual[self.plan.hole].entry
                    && self.stack.is_empty()
                    && self.tail.stage != Stage::Closed
            } else {
                self.stack.last() == Some(&state.node)
                    && self
                        .actual
                        .get(state.node)
                        .is_some_and(|a| a.entry == state.entry)
            }
    }
    fn data(&self, state: SingleState) -> Option<&ActualNode> {
        if state.tail {
            Some(&self.tail)
        } else {
            self.actual.get(state.node)
        }
    }
    fn data_mut(&mut self, state: SingleState) -> Option<&mut ActualNode> {
        if state.tail {
            Some(&mut self.tail)
        } else {
            self.actual.get_mut(state.node)
        }
    }
    fn node(&self, state: SingleState) -> Option<Node> {
        if state.tail {
            (state.node == self.plan.hole).then_some(Node {
                parent: None,
                depth: 0,
                kind: Kind::Return {
                    control: self.plan.tail.control,
                    value: self.plan.tail.value,
                    argument: self.plan.tail.argument,
                },
            })
        } else {
            self.plan.nodes.get(state.node).copied()
        }
    }
    fn close_node(
        &mut self,
        state: SingleState,
        cursor: usize,
        identity: &Arc<()>,
        outcome: Outcome,
    ) {
        if !self.current(state) || state.tail {
            self.valid = false;
            return;
        }
        let a = &mut self.actual[state.node];
        if a.token.is_some() {
            self.valid = false;
            return;
        }
        a.stage = Stage::Closed;
        a.token = Some(Token {
            identity: identity.clone(),
            state,
            begin: a.begin,
            end: cursor,
            consumed: false,
            outcome,
        });
        self.stack.pop();
        if let Some((parent, side)) = self.plan.nodes[state.node].parent {
            if self.stack.last() != Some(&parent) {
                self.valid = false;
                return;
            }
            let stage = &mut self.actual[parent].stage;
            if *stage
                != if side {
                    Stage::ThenOpen
                } else {
                    Stage::ElseOpen
                }
            {
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
    pub(in super::super) fn begin_single(
        &mut self,
        plan: Plan,
        base: (usize, usize),
        clear: bool,
    ) -> SingleState {
        let candidate = self.single_regions.len();
        let valid = self.enabled
            && clear
            && plan.valid_structure()
            && !self
                .single_regions
                .iter()
                .any(|c| c.plan.callable == plan.callable);
        let actual = plan
            .nodes
            .iter()
            .map(|_| fresh(self.records.len()))
            .collect();
        self.single_regions.push(Candidate {
            plan,
            base,
            start: self.records.len(),
            valid,
            stack: vec![0],
            actual,
            entries: Vec::new(),
            pending: None,
            resumptions: Vec::new(),
            tail: fresh(self.records.len()),
            tail_ready: false,
            sealed: false,
            closed: None,
        });
        SingleState {
            candidate,
            node: 0,
            entry: None,
            tail: false,
        }
    }
    pub(in super::super) fn single_node(
        &self,
        state: SingleState,
    ) -> Option<(Node, (usize, usize))> {
        let c = self.single_regions.get(state.candidate)?;
        Some((c.node(state)?, c.base))
    }
    pub(in super::super) fn single_source_matches(
        &self,
        state: SingleState,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        root: crate::name_resolution::UnitSymbolId,
    ) -> bool {
        self.single_regions.get(state.candidate).is_some_and(|c|c.current(state)&&c.plan.root==root
            && matches!(c.node(state),Some(Node {kind:Kind::Return {value,argument:wanted,..},..}) if value==call&&wanted==argument))
    }
    pub(in super::super) fn single_condition(
        &mut self,
        state: SingleState,
        split: UnitExpressionId,
        condition: UnitExpressionId,
        after: bool,
        clear: bool,
    ) {
        let Some(c) = self.single_regions.get_mut(state.candidate) else {
            return;
        };
        let good = c.current(state)
            && !state.tail
            && clear
            && matches!(c.node(state),Some(Node {kind:Kind::Split {split:s,condition:q,..},..}) if s==split&&q==condition)
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
    pub(in super::super) fn single_arm(
        &mut self,
        state: SingleState,
        split: UnitExpressionId,
        side: bool,
        clear: bool,
    ) -> Option<SingleState> {
        let c = self.single_regions.get_mut(state.candidate)?;
        let Some(Node {
            kind:
                Kind::Split {
                    split: wanted,
                    condition,
                    children,
                    ..
                },
            ..
        }) = c.node(state)
        else {
            c.valid = false;
            return None;
        };
        let child = children[usize::from(!side)];
        c.valid &= c.current(state)
            && !state.tail
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
        Some(SingleState {
            candidate: state.candidate,
            node: child,
            entry: Some(entry),
            tail: false,
        })
    }
    pub(in super::super) fn single_hole(&mut self, state: SingleState, clear: bool) {
        let Some(c) = self.single_regions.get_mut(state.candidate) else {
            return;
        };
        c.valid &= c.current(state)
            && !state.tail
            && clear
            && state.node == c.plan.hole
            && matches!(
                c.node(state),
                Some(Node {
                    kind: Kind::Hole,
                    ..
                })
            )
            && c.actual[state.node].stage == Stage::Awaiting
            && c.actual[state.node].begin == self.records.len();
        if c.valid {
            c.close_node(
                state,
                self.records.len(),
                &self.identity,
                Outcome::Continues,
            );
        }
    }
    pub(in super::super) fn record_single_handoff(
        &mut self,
        cause: Option<SingleCause>,
        completed: Option<SingleState>,
    ) {
        let Some(cause) = cause.filter(|c| Some(c.state) == completed) else {
            return;
        };
        let Some(c) = self.single_regions.get_mut(cause.state.candidate) else {
            return;
        };
        let state = cause.state;
        let index = self.records.len().saturating_sub(1);
        let good = c.current(state)
            && c.data(state).is_some_and(|a| {
                a.stage == Stage::LeafLive
                    && a.begin == cause.acquisition.index
                    && a.delivery.is_none()
            })
            && Arc::ptr_eq(&cause.acquisition.identity, &self.identity)
            && cause.acquisition.index.checked_add(1) == Some(index)
            && matches!((self.records.get(cause.acquisition.index),self.records.get(index)),(Some(Record::Begin(b)),Some(Record::BorrowForwardHandoff{transaction,..})) if b==transaction.source_loan());
        c.valid &= good;
        if c.valid
            && let Some(a) = c.data_mut(state)
        {
            a.delivery = Some((cause.acquisition.index, index));
        }
    }
    pub(in super::super) fn single_return(
        &mut self,
        state: SingleState,
        control: UnitExpressionId,
        post: bool,
        clear: bool,
    ) {
        let Some(c) = self.single_regions.get_mut(state.candidate) else {
            return;
        };
        let Some(Node {
            kind:
                Kind::Return {
                    control: wanted,
                    value,
                    argument,
                },
            ..
        }) = c.node(state)
        else {
            c.valid = false;
            return;
        };
        let expected_stage = if post {
            Stage::LeafLive
        } else {
            Stage::Awaiting
        };
        let good = c.current(state)
            && clear
            && control == wanted
            && c.data(state).is_some_and(|a| {
                a.stage == expected_stage
                    && a.begin.checked_add(if post { 2 } else { 1 }) == Some(self.records.len())
                    && matches!(self.records.get(a.begin), Some(Record::Begin(b))
                        if b.call() == value && b.argument() == argument && b.kind() == LoanKind::Shared
                        && matches!(b.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == c.plan.root))
                    && (!post || (a.delivery == Some((a.begin, a.begin + 1))
                        && matches!(self.records.get(a.begin + 1), Some(Record::BorrowForwardHandoff { transaction, .. })
                            if transaction.returned_expression() == value && transaction.point() == UnitDropPoint::ControlTransfer(control))))
            });
        c.valid &= good;
        if !c.valid {
            return;
        }
        if post {
            if state.tail {
                c.tail.stage = Stage::Closed;
            } else {
                c.close_node(state, self.records.len(), &self.identity, Outcome::Closed);
            }
        } else if let Some(a) = c.data_mut(state) {
            a.stage = Stage::LeafLive;
        }
    }
    pub(in super::super) fn single_join(
        &mut self,
        state: SingleState,
        split: UnitExpressionId,
        then_terminated: bool,
        else_terminated: bool,
        survivor: Option<SingleState>,
        clear: bool,
    ) -> Option<SingleResume> {
        let c = self.single_regions.get_mut(state.candidate)?;
        let Some(Node {
            kind:
                Kind::Split {
                    split: wanted,
                    children,
                    ..
                },
            ..
        }) = c.node(state)
        else {
            c.valid = false;
            return None;
        };
        let token = |child: usize| c.actual.get(child).and_then(|a| a.token.as_ref());
        let good = c.current(state)
            && !state.tail
            && clear
            && split == wanted
            && c.actual[state.node].stage == Stage::ReadyJoin
            && children.iter().enumerate().all(|(i, &child)| {
                token(child).is_some_and(|t| {
                    Arc::ptr_eq(&t.identity, &self.identity)
                        && t.state
                            == SingleState {
                                candidate: state.candidate,
                                node: child,
                                entry: c.actual[child].entry,
                                tail: false,
                            }
                        && !t.consumed
                        && c.plan.nodes[child].parent == Some((state.node, i == 0))
                        && t.state
                            .entry
                            .and_then(|e| c.entries.get(e))
                            .is_some_and(|e| e.parent == state.entry && e.node == child)
                        && (t.outcome == Outcome::Closed)
                            == if i == 0 {
                                then_terminated
                            } else {
                                else_terminated
                            }
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
            return None;
        }
        let continuing = children
            .iter()
            .filter_map(|child| {
                token(*child)
                    .filter(|t| t.outcome == Outcome::Continues)
                    .map(|t| t.state)
            })
            .collect::<Vec<_>>();
        match continuing.as_slice() {
            [] if survivor.is_none() => {}
            [live] if survivor == Some(*live) => {}
            _ => {
                c.valid = false;
                return None;
            }
        }
        for child in children {
            if let Some(t) = c.actual[child].token.as_mut() {
                t.consumed = true;
            }
        }
        if let Some(live) = survivor {
            let resume = SingleResume {
                identity: self.identity.clone(),
                candidate: state.candidate,
                parent: state,
                survivor: live,
                cursor: self.records.len(),
            };
            c.actual[state.node].stage = Stage::PendingResume;
            c.pending = Some(resume.clone());
            Some(resume)
        } else {
            c.close_node(state, self.records.len(), &self.identity, Outcome::Closed);
            None
        }
    }
    pub(in super::super) fn single_resume_node(
        &self,
        resume: &SingleResume,
    ) -> Option<(Node, (usize, usize))> {
        let c = self.single_regions.get(resume.candidate)?;
        if !Arc::ptr_eq(&resume.identity, &self.identity) || c.pending.as_ref() != Some(resume) {
            return None;
        }
        Some((c.node(resume.parent)?, c.base))
    }
    pub(in super::super) fn single_resume(
        &mut self,
        resume: SingleResume,
        survivor: Option<SingleState>,
        clear: bool,
    ) -> Option<SingleState> {
        let c = self.single_regions.get_mut(resume.candidate)?;
        let state = resume.parent;
        let good = c.valid
            && c.structurally_valid()
            && Arc::ptr_eq(&resume.identity, &self.identity)
            && c.pending.as_ref() == Some(&resume)
            && clear
            && survivor == Some(resume.survivor)
            && resume.cursor == self.records.len()
            && c.stack.last() == Some(&state.node)
            && c.actual
                .get(state.node)
                .is_some_and(|a| a.stage == Stage::PendingResume && a.entry == state.entry)
            && c.actual
                .get(resume.survivor.node)
                .and_then(|a| a.token.as_ref())
                .is_some_and(|t| {
                    t.state == resume.survivor
                        && t.consumed
                        && t.outcome == Outcome::Continues
                        && Arc::ptr_eq(&t.identity, &self.identity)
                });
        c.valid &= good;
        if !c.valid {
            return None;
        }
        let edge = c.entries.get(resume.survivor.entry?).map(|e| e.edge)?;
        c.resumptions.push(Resumption {
            parent: state,
            survivor: resume.survivor,
            cursor: self.records.len(),
            edge,
        });
        c.pending = None;
        c.close_node(
            state,
            self.records.len(),
            &self.identity,
            Outcome::Continues,
        );
        if !c.valid {
            return None;
        }
        if state.node == 0 {
            if let Some(t) = c.actual[0].token.as_mut() {
                t.consumed = true;
            }
            c.tail = fresh(self.records.len());
            c.tail_ready = true;
            Some(SingleState {
                candidate: state.candidate,
                node: c.plan.hole,
                entry: c.actual[c.plan.hole].entry,
                tail: true,
            })
        } else {
            Some(state)
        }
    }
    pub(in super::super) fn seal_single(
        &mut self,
        state: SingleState,
        terminal: bool,
        clear: bool,
    ) {
        let Some(c) = self.single_regions.get_mut(state.candidate) else {
            return;
        };
        c.valid &= terminal
            && clear
            && !state.tail
            && state.node == 0
            && state.entry.is_none()
            && !c.sealed
            && c.pending.is_none()
            && c.stack.is_empty()
            && c.tail_ready
            && c.tail.stage == Stage::Closed
            && c.tail.begin.checked_add(2) == Some(self.records.len());
        if c.valid {
            c.sealed = true;
        }
    }
    pub(in super::super) fn close_single(&mut self, callable: UnitItemId) {
        for c in self
            .single_regions
            .iter_mut()
            .filter(|c| c.plan.callable == callable)
        {
            c.valid &= c.sealed
                && c.closed.is_none()
                && c.tail.begin.checked_add(2) == Some(self.records.len());
            if c.valid {
                c.closed = Some(self.records.len());
            }
        }
    }
    pub(super) fn finish_single(&self) -> Vec<UnitBoundedBorrowContinuationRegion> {
        #[cfg(test)]
        tests::record(self);
        self.single_regions
            .iter()
            .enumerate()
            .map(|(id, c)| self.single_region(id, c))
            .collect()
    }
}

impl Candidate {
    fn lineage(
        &self,
        node: usize,
        cursor: usize,
        strict_begin: bool,
    ) -> Option<Vec<UnitLoanNestedForwardEdge>> {
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
                || (strict_begin && current == node && e.cursor != cursor)
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
}
impl ActionSource {
    fn single_region(&self, id: usize, c: &Candidate) -> UnitBoundedBorrowContinuationRegion {
        let partial = || UnitBoundedBorrowContinuationRegion {
            identity: self.identity.clone(),
            callable: c.plan.callable,
            root: c.plan.root,
            coverage: UnitLoanActionCoverage::Partial,
            split_count: c.plan.split_count,
            paths: Vec::new(),
            continuation: None,
        };
        if !c.structurally_valid()
            || c.entries.len() != c.plan.nodes.len() - 1
            || c.entries.len() > 14
        {
            return partial();
        }
        let Some(end) = c.start.checked_add(c.plan.leaves.len() * 2) else {
            return partial();
        };
        let Some(tail_begin) = end.checked_sub(2) else {
            return partial();
        };
        // The one live route is replayed bottom-up, with a hard bound independent of plan validity.
        let mut chain = Vec::new();
        let mut child = c.plan.hole;
        while let Some((parent, _)) = c.plan.nodes[child].parent {
            if chain.len() >= 3 {
                return partial();
            }
            chain.push((parent, child));
            child = parent;
        }
        if child != 0 || chain.len() != c.resumptions.len() {
            return partial();
        }
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
            let continuing = node == c.plan.hole || chain.iter().any(|(parent, _)| *parent == node);
            good &= a.stage == Stage::Closed
                && a.token.as_ref().is_some_and(|t| {
                    Arc::ptr_eq(&t.identity, &self.identity)
                        && t.state
                            == SingleState {
                                candidate: id,
                                node,
                                entry: a.entry,
                                tail: false,
                            }
                        && t.consumed
                        && t.begin == a.begin
                        && t.begin >= c.start
                        && t.end >= t.begin
                        && t.end <= tail_begin
                        && (t.outcome == Outcome::Continues) == continuing
                        && match c.plan.nodes[node].kind {
                            Kind::Return { .. } => t.begin.checked_add(2) == Some(t.end),
                            Kind::Hole => t.begin == t.end,
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
                });
        }
        good &= c
            .actual
            .first()
            .and_then(|a| a.token.as_ref())
            .is_some_and(|t| t.begin == c.start && t.end == tail_begin);
        let mut resumed_edges = Vec::new();
        let mut previous_cursor = c.actual[c.plan.hole].begin;
        for ((parent, child), observed) in chain.iter().zip(&c.resumptions) {
            let expected_parent = SingleState {
                candidate: id,
                node: *parent,
                entry: c.actual[*parent].entry,
                tail: false,
            };
            let expected_child = SingleState {
                candidate: id,
                node: *child,
                entry: c.actual[*child].entry,
                tail: false,
            };
            let edge = c.actual[*child].entry.and_then(|i| c.entries.get(i));
            good &= observed.parent == expected_parent
                && observed.survivor == expected_child
                && edge.is_some_and(|e| {
                    e.edge == observed.edge && e.parent == expected_parent.entry && e.node == *child
                })
                && c.actual[*parent]
                    .token
                    .as_ref()
                    .is_some_and(|t| t.end == observed.cursor)
                && observed.cursor >= previous_cursor
                && observed.cursor <= tail_begin;
            previous_cursor = observed.cursor;
            resumed_edges.push(observed.edge);
        }
        good &= previous_cursor == tail_begin;
        let Some(entries) = c.lineage(c.plan.hole, c.actual[c.plan.hole].begin, true) else {
            return partial();
        };
        good &= resumed_edges
            .iter()
            .copied()
            .eq(entries.iter().rev().copied());
        let mut paths = Vec::new();
        let mut ordinal = 0;
        for &node in &c.plan.leaves {
            let Kind::Return {
                control,
                value,
                argument,
            } = c.plan.nodes[node].kind
            else {
                good &= node == c.plan.hole && c.actual[node].begin == c.start + ordinal * 2;
                continue;
            };
            let begin = c.start + ordinal * 2;
            ordinal += 1;
            good &= c.actual[node].delivery == Some((begin, begin + 1));
            let Some(edges) = c.lineage(node, begin, true) else {
                return partial();
            };
            good &= self.single_delivery(c.plan.root, control, value, argument, begin);
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
        good &= ordinal + 1 == c.plan.leaves.len()
            && self.single_delivery(
                c.plan.root,
                c.plan.tail.control,
                c.plan.tail.value,
                c.plan.tail.argument,
                tail_begin,
            );
        paths.push(UnitBoundedBorrowForwardPath {
            identity: self.identity.clone(),
            callable: c.plan.callable,
            root: c.plan.root,
            control: c.plan.tail.control,
            value: c.plan.tail.value,
            begin: tail_begin,
            handoff: tail_begin + 1,
            edges: entries.clone(),
        });
        if !good {
            return partial();
        }
        UnitBoundedBorrowContinuationRegion {
            identity: self.identity.clone(),
            callable: c.plan.callable,
            root: c.plan.root,
            coverage: UnitLoanActionCoverage::Complete,
            split_count: c.plan.split_count,
            paths,
            continuation: Some(UnitBoundedBorrowContinuation {
                entries,
                resumptions: resumed_edges,
                tail_control: c.plan.tail.control,
            }),
        }
    }
    fn single_delivery(
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

#[cfg(test)]
mod tests;
