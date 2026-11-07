//! Match local value-class bindings to declaration-order layout fields.

use std::collections::BTreeMap;

use crate::{
    parser::NameMarker,
    source::{SourceMap, Span},
};

use super::TypeCheckingError;

pub(super) fn local_field_matches<'a>(
    sources: &SourceMap,
    bindings: &[NameMarker],
    fields: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<(Span, Option<usize>)>, TypeCheckingError> {
    let indices = fields
        .into_iter()
        .enumerate()
        .map(|(index, name)| (name, index))
        .collect::<BTreeMap<_, _>>();
    bindings
        .iter()
        .filter_map(|binding| match *binding {
            NameMarker::Present(span) => Some(span),
            NameMarker::Missing(_) | NameMarker::Error(_) => None,
        })
        .map(|span| Ok((span, indices.get(sources.slice(span)?).copied())))
        .collect()
}
