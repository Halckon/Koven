//! Same-source lifetime oracle; expected owners come from names, never from drops.
use super::*;
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, UnitReferenceTarget, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{UnitDropPoint, UnitDropTarget, check_compilation_unit_ownership},
    type_checking::{UnitExpressionId, check_compilation_unit_types},
};

pub(super) fn check(passing: &str, failing: &str, dropping: &str) {
    for rooted in [false, true] {
        let owned = analyzed_unit(passing, rooted);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert_eq!(unit_codes(&analyzed_unit(failing, rooted)), ["L0135"]);
        check_drops(dropping, rooted);
    }
}

fn check_drops(text: &str, rooted: bool) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("closures.ko", text).unwrap();
    let parsed = parse_file_twice(&sources, source, "shared capture lifetime Unit");
    assert!(parsed.diagnostics().is_empty());
    assert_eq!(sources.source_text(source), Ok(text));
    let (environment, types) = environments();
    let input = if rooted {
        SourceUnitInput::new("root", "closures.ko", source, &parsed)
    } else {
        SourceUnitInput::memory_document(source, &parsed)
    };
    let inputs = [input];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    assert!(index.diagnostics().is_empty());
    let [entry] = index.source_units() else {
        panic!("one original source")
    };
    assert_eq!(entry.source_id(), source);
    let unit = entry.id();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let names = names.validate().unwrap();
    let resolution = names.names().source_units()[0].resolution();
    let item = resolution
        .symbols()
        .iter()
        .rfind(|candidate| candidate.name() == "item")
        .expect("releaseOnly item")
        .id();
    let closure = symbol(resolution, "closure");
    let qualify = |original| {
        let mut candidates = names
            .names()
            .references()
            .iter()
            .filter_map(|reference| match reference.target() {
                UnitReferenceTarget::Symbol(id)
                    if id.source_unit() == unit && id.symbol() == original =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        candidates.sort();
        candidates.dedup();
        let [identity] = candidates.as_slice() else {
            panic!("one actual referenced owner")
        };
        *identity
    };
    let expected = [
        UnitDropTarget::Named(qualify(closure)),
        UnitDropTarget::Named(qualify(item)),
    ];
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(owned.is_compatible_with(&typed));
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let call_return_drops = owned
        .drops()
        .iter()
        .filter_map(|drop| match drop.point() {
            UnitDropPoint::CallReturn(_) => Some(drop.target()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(call_return_drops, expected);
    // Strengthen the original global count/order oracle with its actual call identity.
    let calls = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            (sources.slice(node.span()) == Ok("run(closure)")
                && matches!(node.payload(), Expression::Call { .. }))
            .then_some(id)
        })
        .collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        panic!("one actual run call")
    };
    assert!(
        owned
            .drops()
            .iter()
            .filter(|drop| matches!(drop.point(), UnitDropPoint::CallReturn(_)))
            .all(|drop| drop.point()
                == UnitDropPoint::CallReturn(UnitExpressionId::new(unit, *call)))
    );
}
