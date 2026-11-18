//! Original structural capability and compiler-bound effect observations.
use super::*;
use lang_frontend::{
    name_resolution::{
        SourceUnitId, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    type_checking::{CompilationUnitTypes, check_compilation_unit_types},
};

fn visit(
    text: &str,
    external: bool,
    inspect: impl Fn(&NameResolution, SourceUnitId, &CompilationUnitTypes, &CompilationUnitOwnership),
) {
    for rooted in [false, true] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("closures.ko", text).unwrap();
        let parsed = parse_file_twice(&sources, source, "closure capability Unit");
        assert!(parsed.diagnostics().is_empty());
        assert_eq!(sources.source_text(source), Ok(text));
        let (environment, types) = if external {
            cross_thread_environments()
        } else {
            environments()
        };
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
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        inspect(
            names.names().source_units()[0].resolution(),
            unit,
            typed.types(),
            &owned,
        );
    }
}

pub(super) fn structural(text: &str) {
    visit(text, false, |names, unit, typed, owned| {
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let ability = |name| {
            // Preserve the original first matching local symbol, including fields
            // before same-named callable parameters. Do not substitute its namesake.
            let original = symbol(names, name);
            let mut identities = Vec::new();
            for declaration in typed.signatures().declarations() {
                if let Some(callable) = declaration.callable() {
                    identities.extend(
                        callable
                            .parameters()
                            .iter()
                            .filter_map(|parameter| parameter.symbol()),
                    );
                }
                if let Some(nominal) = declaration.nominal() {
                    identities.extend(nominal.fields().iter().map(|field| field.symbol()));
                    identities.extend(
                        nominal
                            .enum_cases()
                            .iter()
                            .flat_map(|case| case.payloads())
                            .map(|field| field.symbol()),
                    );
                }
            }
            identities.retain(|id| id.source_unit() == unit && id.symbol() == original);
            identities.sort();
            identities.dedup();
            let [identity] = identities.as_slice() else {
                panic!("one original symbol for {name}")
            };
            let ty = typed
                .symbol_type(*identity)
                .unwrap_or_else(|| panic!("missing type for {name}"));
            owned
                .transferability(ty)
                .unwrap_or_else(|| panic!("missing transferability for {name}"))
        };
        for name in ["packet", "text", "maybe", "packets", "choice", "proven"] {
            assert_eq!(ability(name), Transferability::Transferable, "{name}");
        }
        for name in [
            "callback",
            "callbacks",
            "shared",
            "action",
            "unknown",
            "top",
        ] {
            assert_eq!(ability(name), Transferability::NotTransferable, "{name}");
        }
        assert_eq!(
            ability("packet"),
            Transferability::Transferable,
            "ordinary classes can be transferable while remaining MoveOnly"
        );
    });
}

pub(super) fn cross_thread(external: &str, source_same_name: &str) {
    visit(external, true, |_, _, typed, owned| {
        assert_eq!(unit_codes(owned), ["L0139", "L0139"]);
        assert!(
            typed
                .calls()
                .iter()
                .filter(|call| !call.arguments().is_empty())
                .all(|call| call.arguments()[0].crosses_thread())
        );
    });
    visit(source_same_name, false, |_, _, _, owned| {
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    });
}
