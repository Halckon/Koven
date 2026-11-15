//! Bounded pair assembly after per-call source contracts have been proved.
use super::*;
pub(super) enum Shape {
    Late,
    Staggered,
    Shared,
    Empty,
    Other,
}
type Initializer = (UnitSymbolId, UnitExpressionId, Span, Vec<u8>);
type CollectedCall = (EntryCallDescriptor, Option<Vec<Initializer>>);
pub(super) fn finish(
    (first, first_owner): CollectedCall,
    (second, second_owner): CollectedCall,
    shape: Shape,
    owned: &CompilationUnitOwnership,
    parsed: &crate::parser::ParsedFile,
) -> Option<EntryCalls> {
    if first.instance() != second.instance() {
        match shape {
            Shape::Late | Shape::Other => None,
            Shape::Empty => different_pair::finish(first, second, owned, parsed),
            Shape::Shared | Shape::Staggered => {
                let owners = first_owner?;
                if second_owner? != owners {
                    return None;
                }
                if matches!(shape, Shape::Staggered) {
                    different_staggered_pair::finish(
                        owners.try_into().ok()?,
                        first,
                        second,
                        owned,
                        parsed,
                    )
                } else {
                    let [owner]: [_; 1] = owners.try_into().ok()?;
                    if matches!(
                        (&first.storage, &second.storage),
                        (
                            StringStorage::SharedMixed { .. },
                            StringStorage::SharedMixed { .. }
                        )
                    ) {
                        different_shared_mixed_pair::finish(owner, first, second, owned, parsed)
                    } else {
                        different_shared_pair::finish(owner, first, second, owned, parsed)
                    }
                }
            }
        }
    } else if matches!(shape, Shape::Late) {
        late_pair::finish(first, second, owned, parsed)
    } else {
        if !first.initialization_order().is_empty()
            || !second.initialization_order().is_empty()
            || !first.named_strings().is_empty()
            || !second.named_strings().is_empty()
        {
            return None;
        }
        if matches!(shape, Shape::Shared | Shape::Staggered) {
            let owners = first_owner?;
            if second_owner? != owners {
                return None;
            }
            if matches!(shape, Shape::Staggered) {
                staggered_pair::finish(owners.try_into().ok()?, first, second, owned)
            } else {
                let [owner]: [_; 1] = owners.try_into().ok()?;
                shared_pair::finish(owner, first, second, owned)
            }
        } else {
            Some(EntryCalls::Pair(Box::new([first, second])))
        }
    }
}
