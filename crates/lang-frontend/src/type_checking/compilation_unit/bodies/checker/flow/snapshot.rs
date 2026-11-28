//! Share unchanged ranges across Unit flow snapshots.

use std::sync::Arc;

use super::FlowKey;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Snapshot<V> {
    symbols: Option<Arc<Node<V>>>,
    receiver: Option<V>,
}

impl<V> Default for Snapshot<V> {
    fn default() -> Self {
        Self {
            symbols: None,
            receiver: None,
        }
    }
}

// A branch selects the highest bit that differs anywhere in its subtree.
// Its prefix stores only the bits above that split, so equal key sets have
// the same shape regardless of insertion or removal order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Node<V> {
    Leaf(u128, V),
    Branch {
        prefix: u128,
        bit: u128,
        children: [Arc<Self>; 2],
    },
}

impl<V> Node<V> {
    fn key(&self) -> u128 {
        match self {
            Self::Leaf(key, _) => *key,
            Self::Branch { prefix, .. } => *prefix,
        }
    }

    fn bit(&self) -> u128 {
        match self {
            Self::Leaf(..) => 0,
            Self::Branch { bit, .. } => *bit,
        }
    }
}

fn symbol_key(key: FlowKey) -> Option<u128> {
    match key {
        FlowKey::Symbol(symbol) => {
            let source = symbol.source_unit().index() as u128;
            Some((source << 64) | symbol.symbol().index() as u128)
        }
        FlowKey::This => None,
    }
}

fn split_bit(a: u128, b: u128) -> u128 {
    let difference = a ^ b;
    if difference == 0 {
        0
    } else {
        1 << (u128::BITS - 1 - difference.leading_zeros())
    }
}

fn branch_side(key: u128, bit: u128) -> usize {
    usize::from(key & bit != 0)
}

fn parent<V>(left: Arc<Node<V>>, right: Arc<Node<V>>, bit: u128) -> Arc<Node<V>> {
    let prefix = left.key() & !(bit | (bit - 1));
    let children = if branch_side(left.key(), bit) == 0 {
        [left, right]
    } else {
        [right, left]
    };
    Arc::new(Node::Branch {
        prefix,
        bit,
        children,
    })
}

fn lookup<V>(mut node: &Node<V>, key: u128) -> Option<&V> {
    loop {
        match node {
            Node::Leaf(found, value) => return (*found == key).then_some(value),
            Node::Branch { bit, children, .. } => node = &children[branch_side(key, *bit)],
        }
    }
}

fn writable<'a, V: Copy>(node: &'a mut Arc<Node<V>>, _copies: &mut usize) -> &'a mut Node<V> {
    #[cfg(test)]
    {
        *_copies += usize::from(Arc::strong_count(node) > 1);
    }
    Arc::make_mut(node)
}

fn put<V: Copy>(node: &mut Arc<Node<V>>, key: u128, value: V, copies: &mut usize) {
    let split = split_bit(key, node.key());
    if split > node.bit() {
        *node = parent(Arc::clone(node), Arc::new(Node::Leaf(key, value)), split);
        return;
    }
    match writable(node, copies) {
        Node::Leaf(_, current) => *current = value,
        Node::Branch { bit, children, .. } => {
            put(&mut children[branch_side(key, *bit)], key, value, copies);
        }
    }
}

// Called only after lookup found the leaf.
fn erase<V: Copy>(node: &mut Arc<Node<V>>, key: u128, copies: &mut usize) {
    let Node::Branch { bit, children, .. } = node.as_ref() else {
        unreachable!("the caller removes root leaves directly");
    };
    let side = branch_side(key, *bit);
    if matches!(children[side].as_ref(), Node::Leaf(..)) {
        *node = Arc::clone(&children[1 - side]);
        return;
    }
    let Node::Branch { children, .. } = writable(node, copies) else {
        unreachable!();
    };
    erase(&mut children[side], key, copies);
}

impl<V: Copy + Eq> Snapshot<V> {
    pub(super) fn get(&self, key: FlowKey) -> Option<&V> {
        if let Some(key) = symbol_key(key) {
            self.symbols.as_deref().and_then(|node| lookup(node, key))
        } else {
            self.receiver.as_ref()
        }
    }

    /// Return the number of shared nodes copied by this update.
    pub(super) fn set(&mut self, key: FlowKey, value: Option<V>) -> usize {
        if self.get(key).copied() == value {
            return 0;
        }
        let Some(key) = symbol_key(key) else {
            self.receiver = value;
            return 0;
        };
        let mut copies = 0;
        match (&mut self.symbols, value) {
            (Some(node), Some(value)) => put(node, key, value, &mut copies),
            (None, Some(value)) => self.symbols = Some(Arc::new(Node::Leaf(key, value))),
            (Some(node), None) if node.bit() != 0 => erase(node, key, &mut copies),
            (_, None) => self.symbols = None,
        }
        copies
    }

    /// Join versions, or intersect equal facts. Shared ranges need no traversal.
    pub(super) fn join(
        &self,
        other: &Self,
        keep_unmatched: bool,
        combine: impl Fn(V, V) -> Option<V>,
    ) -> (Self, usize) {
        let mut visits = 0;
        let symbols = join(
            self.symbols.as_ref(),
            other.symbols.as_ref(),
            keep_unmatched,
            &combine,
            &mut visits,
        );
        let receiver = match (self.receiver, other.receiver) {
            (Some(left), Some(right)) => combine(left, right),
            (left, right) if keep_unmatched => left.or(right),
            _ => None,
        };
        (Self { symbols, receiver }, visits)
    }
}

// Both callers use a commutative, idempotent combination.
fn join<V: Copy + Eq>(
    left: Option<&Arc<Node<V>>>,
    right: Option<&Arc<Node<V>>>,
    keep: bool,
    combine: &impl Fn(V, V) -> Option<V>,
    _visits: &mut usize,
) -> Option<Arc<Node<V>>> {
    let (Some(left), Some(right)) = (left, right) else {
        return if keep { left.or(right).cloned() } else { None };
    };
    if Arc::ptr_eq(left, right) {
        return Some(Arc::clone(left));
    }
    #[cfg(test)]
    {
        *_visits += 2;
    }
    let split = split_bit(left.key(), right.key());
    if split > left.bit().max(right.bit()) {
        return keep.then(|| parent(Arc::clone(left), Arc::clone(right), split));
    }
    if left.bit() == right.bit() {
        return match (left.as_ref(), right.as_ref()) {
            (Node::Leaf(key, a), Node::Leaf(_, b)) => combine(*a, *b).map(|value| {
                if value == *a {
                    Arc::clone(left)
                } else if value == *b {
                    Arc::clone(right)
                } else {
                    Arc::new(Node::Leaf(*key, value))
                }
            }),
            (
                Node::Branch {
                    bit, children: a, ..
                },
                Node::Branch { children: b, .. },
            ) => {
                let zero = join(Some(&a[0]), Some(&b[0]), keep, combine, _visits);
                let one = join(Some(&a[1]), Some(&b[1]), keep, combine, _visits);
                match (zero, one) {
                    (Some(zero), Some(one))
                        if Arc::ptr_eq(&zero, &a[0]) && Arc::ptr_eq(&one, &a[1]) =>
                    {
                        Some(Arc::clone(left))
                    }
                    (Some(zero), Some(one))
                        if Arc::ptr_eq(&zero, &b[0]) && Arc::ptr_eq(&one, &b[1]) =>
                    {
                        Some(Arc::clone(right))
                    }
                    (Some(zero), Some(one)) => Some(parent(zero, one, *bit)),
                    (zero, one) => zero.or(one),
                }
            }
            _ => unreachable!(),
        };
    }
    let (outer, inner) = if left.bit() > right.bit() {
        (left, right)
    } else {
        (right, left)
    };
    let Node::Branch { bit, children, .. } = outer.as_ref() else {
        unreachable!()
    };
    let side = branch_side(inner.key(), *bit);
    let changed = join(Some(&children[side]), Some(inner), keep, combine, _visits);
    if !keep {
        return changed;
    }
    Some(match changed {
        Some(child) if Arc::ptr_eq(&child, &children[side]) => Arc::clone(outer),
        Some(child) => parent(child, Arc::clone(&children[1 - side]), *bit),
        None => Arc::clone(&children[1 - side]),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    const KEYS: [u128; 8] = [0, 1, 7, 8, 17, (1 << 64) | 7, 2 << 64, (1 << 127) | 9];

    fn make_tree(entries: impl IntoIterator<Item = (u128, u64)>) -> Snapshot<u64> {
        let mut result = Snapshot::default();
        for (key, value) in entries {
            match &mut result.symbols {
                None => result.symbols = Some(Arc::new(Node::Leaf(key, value))),
                Some(node) => put(node, key, value, &mut 0),
            }
        }
        result
    }

    fn entries(map: &Snapshot<u64>) -> BTreeMap<u128, u64> {
        fn collect(node: &Node<u64>, result: &mut BTreeMap<u128, u64>) {
            match node {
                Node::Leaf(key, value) => assert!(result.insert(*key, *value).is_none()),
                Node::Branch { children, .. } => {
                    for child in children {
                        collect(child, result);
                    }
                }
            }
        }

        let mut result = BTreeMap::new();
        if let Some(node) = map.symbols.as_deref() {
            collect(node, &mut result);
        }
        result
    }

    #[test]
    fn updates_share_untouched_ranges_and_preserve_prior_snapshots() {
        let expected: BTreeMap<_, _> = KEYS.into_iter().zip(0..).collect();
        let base = make_tree(expected.iter().rev().map(|(&k, &v)| (k, v)));
        for key in KEYS {
            let mut current = base.clone();
            let mut copies = 0;
            let Some(root) = current.symbols.as_mut() else {
                panic!("fixture contains keys");
            };
            erase(root, key, &mut copies);
            assert!(copies < KEYS.len());
            assert!(lookup(root, key).is_none());
            put(root, key, expected[&key] + 10, &mut copies);
            assert_eq!(lookup(root, key), Some(&(expected[&key] + 10)));
            put(root, key, expected[&key], &mut copies);
            assert_eq!(current.symbols, base.symbols);
            assert_eq!(entries(&base), expected);
        }
    }

    #[test]
    fn receiver_updates_and_joins_stay_independent_of_symbol_roots() {
        let mut left = make_tree([(0, 1), (u128::MAX, 4)]);
        let mut right = left.clone();
        left.set(FlowKey::This, Some(0));
        right.set(FlowKey::This, Some(u64::MAX));
        assert_ne!(left.receiver, right.receiver);
        let (merged, visits) = left.join(&right, true, |a, b| Some(a.max(b)));
        assert_eq!(merged.get(FlowKey::This), Some(&u64::MAX));
        assert_eq!(visits, 0);
        let (intersection, _) = left.join(&right, false, |a, b| (a == b).then_some(a));
        assert_eq!(intersection.get(FlowKey::This), None);
        assert_eq!(entries(&intersection), entries(&left));
        left.set(FlowKey::This, None);
        assert_eq!(left.get(FlowKey::This), None);
        assert_eq!(right.get(FlowKey::This), Some(&u64::MAX));
    }

    #[test]
    fn one_key_fork_only_visits_the_changed_path_in_a_wide_join() {
        let left = make_tree((0..512).map(|key| (key, 1)));
        let mut right = left.clone();
        let mut copies = 0;
        put(right.symbols.as_mut().unwrap(), 257, 2, &mut copies);
        let (union, union_visits) = left.join(&right, true, |a, b| Some(a.max(b)));
        let (intersection, intersection_visits) =
            left.join(&right, false, |a, b| (a == b).then_some(a));
        assert!(copies <= 10, "{copies}");
        assert!(union_visits <= 20, "{union_visits}");
        assert!(intersection_visits <= 20, "{intersection_visits}");
        assert!(union.symbols == right.symbols);
        let mut expected = entries(&left);
        expected.remove(&257);
        assert_eq!(entries(&intersection), expected);
    }

    #[test]
    fn joins_match_ordered_reference_maps_for_disjoint_nested_and_shared_ranges() {
        let reference = |mask: u16, right| {
            KEYS.into_iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(index, key)| (key, index as u64 + u64::from(right && index % 2 == 0)))
                .collect::<BTreeMap<_, _>>()
        };
        let all = (0..1_u16 << KEYS.len())
            .map(|mask| {
                let left = reference(mask, false);
                let right = reference(mask, true);
                let tree_l = make_tree(left.iter().map(|(&k, &v)| (k, v)));
                let tree_r = make_tree(right.iter().rev().map(|(&k, &v)| (k, v)));
                (left, right, tree_l, tree_r)
            })
            .collect::<Vec<_>>();
        for (left, _, tree_l, _) in &all {
            for (_, right, _, tree_r) in &all {
                let (union, _) = tree_l.join(tree_r, true, |a, b| Some(a.max(b)));
                let (intersection, _) = tree_l.join(tree_r, false, |a, b| (a == b).then_some(a));
                let mut expected = left.clone();
                for (&key, &value) in right {
                    expected
                        .entry(key)
                        .and_modify(|a| *a = (*a).max(value))
                        .or_insert(value);
                }
                assert_eq!(entries(&union), expected);
                assert_eq!(
                    entries(&intersection),
                    left.iter()
                        .filter_map(|(&key, &value)| (right.get(&key) == Some(&value))
                            .then_some((key, value)))
                        .collect()
                );
            }
        }
        let original = &all.last().unwrap().2;
        let (same, visits) = original.join(&original.clone(), false, |a, b| (a == b).then_some(a));
        assert_eq!(visits, 0);
        assert!(Arc::ptr_eq(
            original.symbols.as_ref().unwrap(),
            same.symbols.as_ref().unwrap()
        ));
    }
}
