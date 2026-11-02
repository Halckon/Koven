//! Shared capture scope, source containment and capability aggregation rules.
//! Typed identities and capture collection remain in their respective producers.
use super::Transferability;
use crate::{
    name_resolution::{NameResolution, ScopeId},
    source::Span,
};

pub(super) fn ancestors(
    scope: ScopeId,
    names: &NameResolution,
) -> impl Iterator<Item = ScopeId> + '_ {
    std::iter::successors(Some(scope), |scope| {
        names
            .scopes()
            .get(scope.index())
            .and_then(|scope| scope.parent())
    })
}

pub(super) fn is_descendant(scope: ScopeId, ancestor: ScopeId, names: &NameResolution) -> bool {
    ancestors(scope, names).any(|scope| scope == ancestor)
}

pub(super) fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

pub(super) const fn combine(left: Transferability, right: Transferability) -> Transferability {
    match (left, right) {
        (Transferability::Error, _) | (_, Transferability::Error) => Transferability::Error,
        (Transferability::Unknown, _) | (_, Transferability::Unknown) => Transferability::Unknown,
        (Transferability::NotTransferable, _) | (_, Transferability::NotTransferable) => {
            Transferability::NotTransferable
        }
        (Transferability::Transferable, Transferability::Transferable) => {
            Transferability::Transferable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lexer::lex,
        name_resolution::{ScopeKind, resolve_names},
        parser::parse_file,
        source::SourceMap,
        type_checking::standard_environments,
    };

    #[test]
    fn aggregation_keeps_all_sixteen_existing_capability_outcomes() {
        use Transferability::{Error, NotTransferable, Transferable, Unknown};
        let values = [Transferable, NotTransferable, Unknown, Error];
        let expected = [
            [Transferable, NotTransferable, Unknown, Error],
            [NotTransferable, NotTransferable, Unknown, Error],
            [Unknown, Unknown, Unknown, Error],
            [Error, Error, Error, Error],
        ];
        for (row, left) in values.into_iter().enumerate() {
            for (column, right) in values.into_iter().enumerate() {
                assert_eq!(combine(left, right), expected[row][column]);
            }
        }
    }

    #[test]
    fn containment_keeps_source_owner_and_inclusive_boundaries() {
        let mut sources = SourceMap::new();
        let first = sources.add_source("first", "0123456789").unwrap();
        let second = sources.add_source("second", "0123456789").unwrap();
        let mut foreign = SourceMap::new();
        let other = foreign.add_source("first", "0123456789").unwrap();
        let outer = sources.span(first, 2, 8).unwrap();
        for (start, end) in [(2, 8), (3, 7), (2, 2), (8, 8)] {
            assert!(contains(outer, sources.span(first, start, end).unwrap()));
        }
        for (start, end) in [(1, 7), (3, 9), (0, 10)] {
            assert!(!contains(outer, sources.span(first, start, end).unwrap()));
        }
        assert!(!contains(outer, sources.span(second, 2, 8).unwrap()));
        assert!(!contains(outer, foreign.span(other, 2, 8).unwrap()));
    }

    #[test]
    fn real_function_scopes_keep_self_ancestors_and_exclude_siblings() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("scopes.ko", "fun left(): Unit {}\nfun right(): Unit {}")
            .unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, _) = standard_environments();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let functions = names
            .scopes()
            .iter()
            .filter(|scope| scope.kind() == ScopeKind::Function)
            .collect::<Vec<_>>();
        let [left, right] = functions.as_slice() else {
            panic!("two real function scopes");
        };
        let root = left.parent().unwrap();
        assert_eq!(right.parent(), Some(root));
        assert_eq!(names.scopes()[root.index()].kind(), ScopeKind::File);
        assert_eq!(
            ancestors(left.id(), &names).collect::<Vec<_>>(),
            [left.id(), root]
        );
        assert!(is_descendant(left.id(), left.id(), &names));
        assert!(is_descendant(left.id(), root, &names));
        assert!(!is_descendant(left.id(), right.id(), &names));
        assert!(!is_descendant(root, left.id(), &names));
    }
}
