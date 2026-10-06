//! Plan-local source identities refer to earlier records rather than recursively enclosing keys.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::ssa) struct SourceToken(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::ssa) struct CallableToken(usize);

/// The existing source key keeps its own target/types/receiver plus ordered callback slots.
pub(in crate::ssa) trait SourceIdentity: Clone + Ord {
    fn callable_arguments(&self) -> &[(usize, CallableToken)];
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::ssa) enum CallableKey<E> {
    Lambda { owner: SourceToken, expression: E },
    KnownFunction { function: SourceToken },
}

#[derive(Debug, PartialEq, Eq)]
enum Record<K, E> {
    Source(K),
    Callable(CallableKey<E>),
}

/// One append-only record order supplies creation ordinals to both kinds of token.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::ssa) struct CallableArena<K, E> {
    sources: BTreeMap<K, SourceToken>,
    callables: BTreeMap<CallableKey<E>, CallableToken>,
    records: Vec<Record<K, E>>,
}

impl<K: SourceIdentity, E: Clone + Ord> Default for CallableArena<K, E> {
    fn default() -> Self {
        Self {
            sources: BTreeMap::new(),
            callables: BTreeMap::new(),
            records: Vec::new(),
        }
    }
}

impl<K: SourceIdentity, E: Clone + Ord> CallableArena<K, E> {
    /// Deduplicate complete keys before reserving a new source. No budget is charged here.
    pub(in crate::ssa) fn intern_source(&mut self, key: K) -> Option<SourceToken> {
        if let Some(token) = self.sources.get(&key) {
            return Some(*token);
        }
        let arguments = key.callable_arguments();
        if arguments.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return None;
        }
        // Existing records have strictly smaller ordinals than this prospective source.
        for &(_, token) in arguments {
            self.callable(token)?;
        }
        let token = SourceToken(self.records.len());
        self.records.push(Record::Source(key.clone()));
        self.sources.insert(key, token);
        Some(token)
    }

    /// A lambda carries its concrete source owner, never that owner's recursively expanded key.
    pub(in crate::ssa) fn intern_callable(&mut self, key: CallableKey<E>) -> Option<CallableToken> {
        if let Some(token) = self.callables.get(&key) {
            return Some(*token);
        }
        let owner = match &key {
            CallableKey::Lambda { owner, .. } => *owner,
            CallableKey::KnownFunction { function } => *function,
        };
        self.source(owner)?;
        let token = CallableToken(self.records.len());
        self.records.push(Record::Callable(key.clone()));
        self.callables.insert(key, token);
        Some(token)
    }

    pub(in crate::ssa) fn source(&self, token: SourceToken) -> Option<&K> {
        match self.records.get(token.0)? {
            Record::Source(key) => Some(key),
            Record::Callable(_) => None,
        }
    }

    pub(in crate::ssa) fn callable(&self, token: CallableToken) -> Option<&CallableKey<E>> {
        match self.records.get(token.0)? {
            Record::Callable(key) => Some(key),
            Record::Source(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Key {
        function: usize,
        arguments: Vec<(usize, CallableToken)>,
    }
    impl SourceIdentity for Key {
        fn callable_arguments(&self) -> &[(usize, CallableToken)] {
            &self.arguments
        }
    }

    #[test]
    fn callable_arena_parameter_forwarding_reuses_identity_without_enclosing_keys() {
        let mut arena = CallableArena::default();
        let root = arena
            .intern_source(Key {
                function: 0,
                arguments: vec![],
            })
            .unwrap();
        let callback = arena
            .intern_callable(CallableKey::Lambda {
                owner: root,
                expression: 7,
            })
            .unwrap();
        let helper_key = Key {
            function: 1,
            arguments: vec![(3, callback)],
        };
        let helper = arena.intern_source(helper_key.clone()).unwrap();
        assert!(root.0 < callback.0 && callback.0 < helper.0);
        for _ in 0..4 {
            // Forwarding the parameter uses its token rather than qualifying it by each caller.
            assert_eq!(arena.intern_source(helper_key.clone()), Some(helper));
            assert_eq!(
                arena.intern_callable(CallableKey::Lambda {
                    owner: root,
                    expression: 7
                }),
                Some(callback)
            );
        }
        assert_eq!(arena.records.len(), 3);
        assert_eq!(arena.source(helper), Some(&helper_key));
    }

    #[test]
    fn callable_arena_new_owner_keeps_same_expression_distinct_with_flat_references() {
        let mut arena = CallableArena::default();
        let mut owner = arena
            .intern_source(Key {
                function: 0,
                arguments: vec![],
            })
            .unwrap();
        let mut callbacks = Vec::new();
        for _ in 0..4 {
            let callback = arena
                .intern_callable(CallableKey::Lambda {
                    owner,
                    expression: 7,
                })
                .unwrap();
            let next = arena
                .intern_source(Key {
                    function: 1,
                    arguments: vec![(0, callback)],
                })
                .unwrap();
            assert!(owner.0 < callback.0 && callback.0 < next.0);
            assert!(!callbacks.contains(&callback));
            assert_eq!(arena.source(next).unwrap().arguments, [(0, callback)]);
            callbacks.push(callback);
            owner = next;
        }
        assert_eq!(arena.records.len(), 9);
    }

    #[test]
    fn callable_arena_named_body_and_lambda_do_not_merge_equal_pointer_shapes() {
        let mut arena = CallableArena::default();
        let owner = arena
            .intern_source(Key {
                function: 0,
                arguments: vec![],
            })
            .unwrap();
        let named_key = CallableKey::KnownFunction { function: owner };
        let named = arena.intern_callable(named_key.clone()).unwrap();
        let lambda = arena
            .intern_callable(CallableKey::Lambda {
                owner,
                expression: 7,
            })
            .unwrap();
        assert_ne!(named, lambda);
        assert_eq!(arena.intern_callable(named_key.clone()), Some(named));
        assert_eq!(arena.callable(named), Some(&named_key));
        assert_eq!(arena.records.len(), 3);
    }
}
