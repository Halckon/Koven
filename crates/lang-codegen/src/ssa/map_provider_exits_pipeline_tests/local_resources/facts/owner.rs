//! Resource owners remain live at Exit until the actual scanner's lexical Return.
use super::*;

pub(super) fn lexical(site: &Site, parsed: &ParsedFile) -> Result<DropPoint, Failure> {
    one(
        parsed.ast().expressions().iter().filter_map(|(id, node)| {
            (matches!(node.payload(), Expression::Return { .. })
                && within(site.scan, node.span())
                && node.span().start() > site.span.end())
            .then_some(DropPoint::ControlTransfer(id))
        }),
        "local Resource actual lexical Return",
    )
}
