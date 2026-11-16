//! Original nested normal-scope cleanup facts from the same Unit consumed by lowering.
use super::ResourceUnit;
use lang_frontend::{
    name_resolution::SourceUnitKey,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
};

pub(super) fn assert_original_facts(unit: &ResourceUnit) {
    let source = &unit.source;
    source.assert_frontend_clean();
    assert!(source.ownership().deferred().is_empty());
    let [document] = source.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    let [names] = source.names().names().source_units() else {
        panic!("one original local symbol table");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.id(), names.source_unit());
    assert_eq!(document.source_id(), source.parsed().source_id());
    let resource_drops = source
        .ownership()
        .drops()
        .iter()
        .filter_map(|fact| {
            let UnitDropTarget::Named(symbol) = fact.target() else {
                return None;
            };
            assert_eq!(symbol.source_unit(), document.id());
            let symbol = names
                .resolution()
                .symbols()
                .iter()
                .find(|entry| entry.id() == symbol.symbol())
                .expect("drop names an actual original symbol");
            if !matches!(symbol.name(), "first" | "second") {
                return None;
            }
            assert_eq!(
                source.sources().slice(fact.value_origin()),
                Ok(symbol.name())
            );
            Some((symbol.name(), *fact))
        })
        .collect::<Vec<_>>();
    assert_eq!(resource_drops.len(), 2);
    assert_eq!(resource_drops[0].0, "second");
    assert_eq!(resource_drops[1].0, "first");
    assert_ne!(resource_drops[0].1.target(), resource_drops[1].1.target());
    assert_eq!(resource_drops[0].1.point(), resource_drops[1].1.point());
    let UnitDropPoint::AfterStatement(point) = resource_drops[0].1.point() else {
        panic!("the original shared normal-scope AfterStatement boundary is preserved");
    };
    assert_eq!(point.source_unit(), document.id());
    let boundary = source
        .parsed()
        .ast()
        .statements()
        .get(point.statement())
        .unwrap()
        .span();
    assert_eq!(boundary.source_id(), document.source_id());
    assert_eq!(
        source.sources().source_name(boundary.source_id()),
        Ok("lowering.ko")
    );
    assert!(source.sources().slice(boundary).is_ok());
}
