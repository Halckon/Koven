//! Actual multi-survivor joins, independent of terminal and single-survivor certificates.
use super::super::loan_bounded_forward_region::multi_continuation_plan::{Kind, Node, Plan};
use super::{ActionHandle, ActionSource, Record};
use crate::{
    ownership_checking::{
        LoanKind, UnitBoundedBorrowForwardPath, UnitBoundedBorrowMultiContinuationRegion,
        UnitBoundedBorrowMultiJoin, UnitBoundedBorrowSharedTail, UnitBoundedBorrowTailOrigin,
        UnitDropPoint, UnitLoanActionCoverage, UnitLoanNestedForwardEdge, UnitLoanTarget,
    },
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct MultiState {
    pub(in super::super) candidate: usize,
    pub(in super::super) node: usize,
    pub(in super::super) entry: Option<usize>,
    pub(in super::super) tail: bool,
}
#[derive(Clone, Debug)]
pub(in super::super) struct MultiCause {
    pub(in super::super) acquisition: ActionHandle,
    pub(in super::super) state: MultiState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct MultiResume {
    identity: Arc<()>,
    candidate: usize,
    parent: MultiState,
    survivors: Vec<MultiState>,
    incoming: [Vec<usize>; 2],
    origins: Vec<usize>,
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
    state: MultiState,
    begin: usize,
    end: usize,
    consumed: bool,
    outcome: Outcome,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    Closed,
    Continues { origins: Vec<usize> },
}
impl Outcome {
    fn origins(&self) -> &[usize] {
        match self {
            Self::Closed => &[],
            Self::Continues { origins } => origins,
        }
    }
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
    parent: MultiState,
    survivors: Vec<MultiState>,
    incoming: [Vec<usize>; 2],
    origins: Vec<usize>,
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
    pending: Option<MultiResume>,
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
    fn current(&self, state: MultiState) -> bool {
        self.valid
            && self.structurally_valid()
            && !self.sealed
            && self.closed.is_none()
            && self.pending.is_none()
            && if state.tail {
                self.tail_ready
                    && state.node == 0
                    && state.entry.is_none()
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
    fn data(&self, state: MultiState) -> Option<&ActualNode> {
        if state.tail {
            Some(&self.tail)
        } else {
            self.actual.get(state.node)
        }
    }
    fn data_mut(&mut self, state: MultiState) -> Option<&mut ActualNode> {
        if state.tail {
            Some(&mut self.tail)
        } else {
            self.actual.get_mut(state.node)
        }
    }
    fn node(&self, state: MultiState) -> Option<Node> {
        if state.tail {
            (state.node == 0 && state.entry.is_none()).then_some(Node {
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
        state: MultiState,
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
    pub(in super::super) fn begin_multi(
        &mut self,
        plan: Plan,
        base: (usize, usize),
        clear: bool,
    ) -> MultiState {
        let candidate = self.multi_regions.len();
        let valid = self.enabled
            && clear
            && plan.valid_structure()
            && !self
                .multi_regions
                .iter()
                .any(|c| c.plan.callable == plan.callable);
        let actual = plan
            .nodes
            .iter()
            .map(|_| fresh(self.records.len()))
            .collect();
        self.multi_regions.push(Candidate {
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
        MultiState {
            candidate,
            node: 0,
            entry: None,
            tail: false,
        }
    }
    pub(in super::super) fn multi_node(&self, state: MultiState) -> Option<(Node, (usize, usize))> {
        let c = self.multi_regions.get(state.candidate)?;
        Some((c.node(state)?, c.base))
    }
    pub(in super::super) fn multi_source_matches(
        &self,
        state: MultiState,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        root: crate::name_resolution::UnitSymbolId,
    ) -> bool {
        self.multi_regions.get(state.candidate).is_some_and(|c|c.current(state)&&c.plan.root==root
            && matches!(c.node(state),Some(Node {kind:Kind::Return {value,argument:wanted,..},..}) if value==call&&wanted==argument))
    }
    pub(in super::super) fn multi_condition(
        &mut self,
        state: MultiState,
        split: UnitExpressionId,
        condition: UnitExpressionId,
        after: bool,
        clear: bool,
    ) {
        let Some(c) = self.multi_regions.get_mut(state.candidate) else {
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
    pub(in super::super) fn multi_arm(
        &mut self,
        state: MultiState,
        split: UnitExpressionId,
        side: bool,
        clear: bool,
    ) -> Option<MultiState> {
        let c = self.multi_regions.get_mut(state.candidate)?;
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
        Some(MultiState {
            candidate: state.candidate,
            node: child,
            entry: Some(entry),
            tail: false,
        })
    }
    pub(in super::super) fn multi_hole(&mut self, state: MultiState, clear: bool) {
        let Some(c) = self.multi_regions.get_mut(state.candidate) else {
            return;
        };
        c.valid &= c.current(state)
            && !state.tail
            && clear
            && c.plan.holes.contains(&state.node)
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
            let origin = c
                .plan
                .holes
                .iter()
                .position(|node| *node == state.node)
                .expect("checked hole");
            c.close_node(
                state,
                self.records.len(),
                &self.identity,
                Outcome::Continues {
                    origins: vec![origin],
                },
            );
        }
    }

    pub(in super::super) fn record_multi_handoff(
        &mut self,
        cause: Option<MultiCause>,
        completed: Option<MultiState>,
    ) {
        let Some(cause) = cause.filter(|c| Some(c.state) == completed) else {
            return;
        };
        let Some(c) = self.multi_regions.get_mut(cause.state.candidate) else {
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
    pub(in super::super) fn multi_return(
        &mut self,
        state: MultiState,
        control: UnitExpressionId,
        post: bool,
        clear: bool,
    ) {
        let Some(c) = self.multi_regions.get_mut(state.candidate) else {
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
    pub(in super::super) fn multi_join(
        &mut self,
        state: MultiState,
        split: UnitExpressionId,
        then_terminated: bool,
        else_terminated: bool,
        survivors: &[MultiState],
        clear: bool,
    ) -> Option<MultiResume> {
        let c = self.multi_regions.get_mut(state.candidate)?;
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
                            == MultiState {
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
                        && matches!(t.outcome, Outcome::Closed)
                            == if i == 0 {
                                then_terminated
                            } else {
                                else_terminated
                            }
                        && valid_origins(&t.outcome, c.plan.holes.len())
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
                    .filter(|t| !matches!(t.outcome, Outcome::Closed))
                    .map(|t| t.state)
            })
            .collect::<Vec<_>>();
        let incoming = children.map(|child| {
            token(child)
                .expect("checked token")
                .outcome
                .origins()
                .to_vec()
        });
        let Some(origins) = union(&incoming, c.plan.holes.len()) else {
            c.valid = false;
            return None;
        };
        if continuing != survivors {
            c.valid = false;
            return None;
        }
        for child in children {
            if let Some(t) = c.actual[child].token.as_mut() {
                t.consumed = true;
            }
        }
        if survivors.is_empty() {
            c.close_node(state, self.records.len(), &self.identity, Outcome::Closed);
            None
        } else {
            let resume = MultiResume {
                identity: self.identity.clone(),
                candidate: state.candidate,
                parent: state,
                survivors: survivors.to_vec(),
                incoming,
                origins,
                cursor: self.records.len(),
            };
            c.actual[state.node].stage = Stage::PendingResume;
            c.pending = Some(resume.clone());
            Some(resume)
        }
    }
    pub(in super::super) fn multi_resume_node(
        &self,
        resume: &MultiResume,
    ) -> Option<(Node, (usize, usize))> {
        let c = self.multi_regions.get(resume.candidate)?;
        if !Arc::ptr_eq(&resume.identity, &self.identity) || c.pending.as_ref() != Some(resume) {
            return None;
        }
        Some((c.node(resume.parent)?, c.base))
    }
    pub(in super::super) fn multi_resume(
        &mut self,
        resume: MultiResume,
        merged: Option<&[MultiState]>,
        clear: bool,
    ) -> Option<MultiState> {
        let c = self.multi_regions.get_mut(resume.candidate)?;
        let state = resume.parent;
        let good = c.valid
            && c.structurally_valid()
            && Arc::ptr_eq(&resume.identity, &self.identity)
            && c.pending.as_ref() == Some(&resume)
            && clear
            && merged == Some(resume.survivors.as_slice())
            && (1..=2).contains(&resume.survivors.len())
            && resume.cursor == self.records.len()
            && c.stack.last() == Some(&state.node)
            && c.actual
                .get(state.node)
                .is_some_and(|a| a.stage == Stage::PendingResume && a.entry == state.entry)
            && resume.survivors.iter().all(|s| {
                c.actual
                    .get(s.node)
                    .and_then(|a| a.token.as_ref())
                    .is_some_and(|t| {
                        t.state == *s
                            && t.consumed
                            && !matches!(t.outcome, Outcome::Closed)
                            && Arc::ptr_eq(&t.identity, &self.identity)
                    })
            })
            && matches!(c.node(state), Some(Node {kind: Kind::Split {children, ..}, ..}) if {
                let tokens = children.map(|child| c.actual.get(child).and_then(|a| a.token.as_ref()));
                tokens.iter().enumerate().all(|(side, token)| token.is_some_and(|t|
                    t.consumed && t.outcome.origins() == resume.incoming[side]
                    && valid_origins(&t.outcome, c.plan.holes.len())
                    && Arc::ptr_eq(&t.identity, &self.identity)))
                    && tokens.into_iter().flatten().filter(|t| !matches!(t.outcome, Outcome::Closed))
                        .map(|t| t.state).eq(resume.survivors.iter().copied())
            })
            && union(&resume.incoming, c.plan.holes.len()) == Some(resume.origins.clone());
        c.valid &= good;
        if !c.valid {
            return None;
        }
        c.resumptions.push(Resumption {
            parent: state,
            survivors: resume.survivors.clone(),
            incoming: resume.incoming.clone(),
            origins: resume.origins.clone(),
            cursor: self.records.len(),
        });
        c.pending = None;
        c.close_node(
            state,
            self.records.len(),
            &self.identity,
            Outcome::Continues {
                origins: resume.origins,
            },
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
            Some(MultiState {
                candidate: state.candidate,
                node: 0,
                entry: None,
                tail: true,
            })
        } else {
            Some(state)
        }
    }
    pub(in super::super) fn seal_multi(&mut self, state: MultiState, terminal: bool, clear: bool) {
        let Some(c) = self.multi_regions.get_mut(state.candidate) else {
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
    pub(in super::super) fn close_multi(&mut self, callable: UnitItemId) {
        for c in self
            .multi_regions
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
    pub(super) fn finish_multi(&self) -> Vec<UnitBoundedBorrowMultiContinuationRegion> {
        #[cfg(test)]
        tests::record(self);
        self.multi_regions
            .iter()
            .enumerate()
            .map(|(id, c)| self.multi_region(id, c))
            .collect()
    }
}

fn valid_origins(outcome: &Outcome, count: usize) -> bool {
    match outcome {
        Outcome::Closed => true,
        Outcome::Continues { origins } => {
            !origins.is_empty()
                && origins.len() <= count
                && origins.iter().all(|id| *id < count)
                && origins.windows(2).all(|p| p[0] < p[1])
        }
    }
}
fn union(incoming: &[Vec<usize>; 2], count: usize) -> Option<Vec<usize>> {
    let mut all = Vec::new();
    for side in incoming {
        if side.len() > count
            || side.iter().any(|id| *id >= count)
            || !side.windows(2).all(|p| p[0] < p[1])
        {
            return None;
        }
        all.extend(side);
    }
    all.sort_unstable();
    if all.len() > count || !all.windows(2).all(|p| p[0] < p[1]) {
        return None;
    }
    Some(all)
}
mod final_validation;
#[cfg(test)]
mod tests;
