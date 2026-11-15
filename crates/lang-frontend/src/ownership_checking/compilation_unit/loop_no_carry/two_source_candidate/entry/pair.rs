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
            Shape::Late => different_late_pair::finish(first, second, owned, parsed),
            Shape::Other => None,
            Shape::Empty => different_pair::finish(first, second, owned, parsed),
            Shape::Shared | Shape::Staggered => {
                let owners = first_owner?;
                if second_owner? != owners {
                    return None;
                }
                if matches!(shape, Shape::Staggered) {
                    let roots: Vec<_> = (0..4)
                        .filter(|i| second.argument_role(*i).is_some_and(|r| r < 2))
                        .filter_map(|i| second.named_argument_root(i))
                        .collect();
                    if roots.len() == 2 && roots[0] != roots[1] {
                        different_joint_final_pair::finish(
                            owners.try_into().ok()?,
                            first,
                            second,
                            owned,
                        )
                    } else {
                        different_staggered_pair::finish(
                            owners.try_into().ok()?,
                            first,
                            second,
                            owned,
                            parsed,
                        )
                    }
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
                let initializers = owners.try_into().ok()?;
                let borrowed: Vec<_> = (0..4)
                    .filter(|i| second.argument_role(*i).is_some_and(|role| role < 2))
                    .filter_map(|i| second.named_argument_root(i))
                    .collect();
                if borrowed.len() == 2 && borrowed[0] != borrowed[1] {
                    joint_final_pair::finish(initializers, first, second, owned)
                } else {
                    staggered_pair::finish(initializers, first, second, owned)
                }
            } else {
                let [owner]: [_; 1] = owners.try_into().ok()?;
                shared_pair::finish(owner, first, second, owned)
            }
        } else {
            Some(EntryCalls::Pair(Box::new([first, second])))
        }
    }
}
