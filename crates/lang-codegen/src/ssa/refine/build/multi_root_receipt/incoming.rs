//! Both symbolic header alternatives must consume the same root-indexed facts.
//! This is an edge-substitution check, not a runtime iteration witness.
use super::*;

pub(super) fn audit(boundary: &UnitOwnerMultiRootBoundaryV1) -> Option<()> {
    let control = boundary.statement();
    let entries: Vec<_> = boundary
        .slots()
        .iter()
        .map(|slot| (slot.root(), UnitOwnerHeaderSourceV1::Entry(slot.entry())))
        .collect();
    let backedges: Vec<_> = boundary
        .slots()
        .iter()
        .map(|slot| {
            (
                slot.root(),
                UnitOwnerHeaderSourceV1::Backedge(slot.first_body_output()),
            )
        })
        .collect();
    // Never compare catalog-local definition numbers across catalogs. Resolve
    // each complete source to its boundary slot, then substitute that slot.
    let substitute = |source: UnitOwnerHeaderSourceV1| {
        let index = entries.iter().position(|(_, entry)| *entry == source)?;
        Some(backedges[index].1.clone())
    };
    let substitute_facts = |facts: Vec<(UnitOwnerHeaderSourceV1, UnitDropFact)>| {
        facts
            .into_iter()
            .map(|(source, fact)| Some((substitute(source)?, fact)))
            .collect::<Option<Vec<_>>>()
    };
    let entry_normal = boundary.normal_header_retirements(control, &entries)?;
    let backedge_normal = boundary.normal_header_retirements(control, &backedges)?;
    if substitute_facts(entry_normal)? != backedge_normal {
        return None;
    }
    let entry_exit = boundary.condition_false_partition(control, &entries)?;
    let backedge_exit = boundary.condition_false_partition(control, &backedges)?;
    if substitute_facts(entry_exit.retirements().to_vec())? != backedge_exit.retirements() {
        return None;
    }
    let live = entry_exit
        .live_roots()
        .iter()
        .map(|(root, source)| Some((*root, substitute(source.clone())?)))
        .collect::<Option<Vec<_>>>()?;
    if live != backedge_exit.live_roots() {
        return None;
    }
    if !live.is_empty() {
        let entry_suffix = boundary.condition_false_suffix_retirements(control, &entries)?;
        let backedge_suffix = boundary.condition_false_suffix_retirements(control, &backedges)?;
        if substitute_facts(entry_suffix)? != backedge_suffix {
            return None;
        }
    }
    Some(())
}
