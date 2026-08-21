//! Parser prefix-truncation matrices shared UTF-8 scalar boundaries.

pub(crate) fn prefix_ends(source: &str) -> Vec<usize> {
    let mut ends = Vec::with_capacity(source.chars().count() + 1);
    ends.push(0);
    ends.extend(
        source
            .char_indices()
            .map(|(start, character)| start + character.len_utf8()),
    );
    ends
}
