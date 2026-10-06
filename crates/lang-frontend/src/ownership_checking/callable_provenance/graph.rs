//! Finite source-origin equations; ownership and loan validity stay in the existing checker.

use crate::source::Span;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NodeId(usize);

#[derive(Debug)]
enum Node<O> {
    Unknown,
    Leaf(O),
    Merge(Vec<NodeId>),
}

/// One arena belongs to one analysis. States share it only while that analysis runs.
#[derive(Debug)]
pub(crate) struct Graph<O> {
    nodes: Vec<Node<O>>,
}

impl<O: Copy + Eq> Graph<O> {
    pub(crate) fn new() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            nodes: vec![Node::Unknown],
        }))
    }
    pub(crate) fn leaf(&mut self, origin: O) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node::Leaf(origin));
        id
    }
    pub(crate) fn merge(&mut self, inputs: &[NodeId]) -> NodeId {
        if inputs.is_empty() {
            return NodeId::default();
        }
        if inputs.iter().all(|input| *input == inputs[0]) {
            return inputs[0];
        }
        self.header(inputs[0], &inputs[1..])
    }
    fn header(&mut self, entry: NodeId, rest: &[NodeId]) -> NodeId {
        let id = NodeId(self.nodes.len());
        let mut inputs = vec![entry];
        inputs.extend_from_slice(rest);
        self.nodes.push(Node::Merge(inputs));
        id
    }
    fn connect(&mut self, header: NodeId, input: NodeId) {
        if let Node::Merge(inputs) = &mut self.nodes[header.0] {
            inputs.push(input);
        }
    }
    /// Each value rises at most twice. Reverse edges join incrementally, never rescan inputs.
    pub(crate) fn solve(&self, valid: impl Fn(O) -> bool) -> Solution<O> {
        let mut values = vec![Value::Bottom; self.nodes.len()];
        let mut parents = vec![Vec::new(); self.nodes.len()];
        let mut pending = VecDeque::new();
        for (index, node) in self.nodes.iter().enumerate() {
            match node {
                Node::Unknown => values[index] = Value::Unknown,
                Node::Leaf(origin) => {
                    values[index] = if valid(*origin) {
                        Value::Known(*origin)
                    } else {
                        Value::Unknown
                    }
                }
                Node::Merge(inputs) => {
                    for input in inputs {
                        parents[input.0].push(index);
                    }
                }
            }
            if values[index] != Value::Bottom {
                pending.push_back(index);
            }
        }
        while let Some(index) = pending.pop_front() {
            for &parent in &parents[index] {
                let next = values[parent].join(values[index]);
                if next != values[parent] {
                    values[parent] = next;
                    pending.push_back(parent);
                }
            }
        }
        Solution(values)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value<O> {
    Bottom,
    Known(O),
    Unknown,
}
impl<O: Copy + Eq> Value<O> {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Bottom, value) | (value, Self::Bottom) => value,
            (Self::Known(left), Self::Known(right)) if left == right => self,
            _ => Self::Unknown,
        }
    }
}
pub(crate) struct Solution<O>(Vec<Value<O>>);
impl<O: Copy> Solution<O> {
    pub(crate) fn get(&self, id: NodeId) -> Option<O> {
        match self.0[id.0] {
            Value::Known(origin) => Some(origin),
            _ => None,
        }
    }
}

/// A thin addition to an existing control-flow State; no captured layout or loan graph.
#[derive(Clone, Debug)]
pub(crate) struct OriginState<S, O> {
    arena: Option<Rc<RefCell<Graph<O>>>>,
    bindings: BTreeMap<S, NodeId>,
    pub(crate) result: NodeId,
}
impl<S, O> Default for OriginState<S, O> {
    fn default() -> Self {
        Self {
            arena: None,
            bindings: BTreeMap::new(),
            result: NodeId::default(),
        }
    }
}
impl<S: Ord + Eq, O> PartialEq for OriginState<S, O> {
    fn eq(&self, other: &Self) -> bool {
        let same = match (&self.arena, &other.arena) {
            (Some(left), Some(right)) => Rc::ptr_eq(left, right),
            (None, None) => true,
            _ => false,
        };
        same && self.bindings == other.bindings && self.result == other.result
    }
}
impl<S: Ord + Eq, O> Eq for OriginState<S, O> {}
impl<S: Copy + Ord, O: Copy + Eq> OriginState<S, O> {
    pub(crate) fn attach(&mut self, arena: &Rc<RefCell<Graph<O>>>) {
        self.arena = Some(arena.clone());
    }
    pub(crate) fn binding(&self, symbol: S) -> NodeId {
        self.bindings.get(&symbol).copied().unwrap_or_default()
    }
    pub(crate) fn bind(&mut self, symbol: S, value: NodeId) {
        self.bindings.insert(symbol, value);
    }
    pub(crate) fn merge(&mut self, other: &Self) {
        let Some(arena) = self.arena.as_ref().or(other.arena.as_ref()).cloned() else {
            return;
        };
        // Gather IDs before the single mutable arena borrow; user CFGs cannot nest RefCell borrows.
        let symbols: BTreeSet<_> = self
            .bindings
            .keys()
            .chain(other.bindings.keys())
            .copied()
            .collect();
        let inputs: Vec<_> = symbols
            .into_iter()
            .map(|symbol| (symbol, [self.binding(symbol), other.binding(symbol)]))
            .collect();
        let result_inputs = [self.result, other.result];
        {
            let mut graph = arena.borrow_mut();
            for (symbol, pair) in inputs {
                self.bindings.insert(symbol, graph.merge(&pair));
            }
            self.result = graph.merge(&result_inputs);
        }
        self.arena = Some(arena);
    }
    /// Reserve condition-visible headers before inspecting the body or any while condition.
    pub(crate) fn begin_loop(&mut self) -> Vec<(S, NodeId)> {
        let Some(arena) = &self.arena else {
            return Vec::new();
        };
        let mut graph = arena.borrow_mut();
        let mut headers = Vec::new();
        for (symbol, node) in &mut self.bindings {
            *node = graph.header(*node, &[]);
            headers.push((*symbol, *node));
        }
        headers
    }
    pub(crate) fn backedge(&self, headers: &[(S, NodeId)]) {
        let Some(arena) = &self.arena else {
            return;
        };
        let inputs: Vec<_> = headers
            .iter()
            .map(|(symbol, header)| (*header, self.binding(*symbol)))
            .collect();
        let mut graph = arena.borrow_mut();
        for (header, input) in inputs {
            graph.connect(header, input);
        }
    }
    /// Only repeating edges update the header; exits join the false/zero edge and breaks.
    pub(crate) fn loop_exit(
        &self,
        headers: &[(S, NodeId)],
        normal: Option<&Self>,
        continues: Option<&Self>,
        breaks: Option<&Self>,
    ) -> Self {
        for state in [normal, continues].into_iter().flatten() {
            state.backedge(headers);
        }
        let mut exit = self.clone();
        if let Some(state) = breaks {
            exit.merge(state);
        }
        exit
    }
}

pub(crate) struct ReturnFrame<T, Y, E> {
    pub(crate) target: T,
    pub(crate) function_type: Y,
    pub(crate) deliveries: Vec<(E, NodeId, Span, bool)>,
}

/// Candidate storage is private until every body and the existing analysis gates succeed.
pub(crate) struct Collector<K, E, O, T, Y> {
    pub(crate) arena: Rc<RefCell<Graph<O>>>,
    pub(crate) uses: BTreeMap<K, (E, NodeId, Span)>,
    pub(crate) active_return: Option<ReturnFrame<T, Y, E>>,
    pub(crate) returns: Vec<ReturnFrame<T, Y, E>>,
}
impl<K: Ord, E: Copy, O: Copy + Eq, T, Y> Default for Collector<K, E, O, T, Y> {
    fn default() -> Self {
        Self {
            arena: Graph::new(),
            uses: BTreeMap::new(),
            active_return: None,
            returns: Vec::new(),
        }
    }
}
impl<K: Ord, E: Copy, O: Copy + Eq, T, Y> Collector<K, E, O, T, Y> {
    pub(crate) fn leaf(&self, origin: O) -> NodeId {
        self.arena.borrow_mut().leaf(origin)
    }
    pub(crate) fn record(&mut self, key: K, expression: E, node: NodeId, span: Span) {
        if let Some(previous) = self.uses.get_mut(&key) {
            previous.1 = self.arena.borrow_mut().merge(&[previous.1, node]);
        } else {
            self.uses.insert(key, (expression, node, span));
        }
    }
}
