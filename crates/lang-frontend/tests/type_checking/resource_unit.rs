//! Original resource type sources and environments in both Unit entries.
use super::codes;
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SourceUnitKey, index_compilation_unit, resolve_compilation_unit_names,
    },
    type_checking::{CompilationUnitTypes, check_compilation_unit_types},
};

fn checked_entries(text: &str) -> [CompilationUnitTypes; 2] {
    let (sources, file) = super::parse(text);
    let (environment, types) = super::environments();
    [false, true].map(|rooted| {
        let input = if rooted {
            SourceUnitInput::new("root", "types.ko", file.source_id(), &file)
        } else {
            SourceUnitInput::memory_document(file.source_id(), &file)
        };
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        let [source] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(source.source_id(), file.source_id());
        match (rooted, source.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "types.ko");
            }
            _ => panic!("requested entry identity"),
        }
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("unit names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid names");
        check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit types")
    })
}

#[test]
fn class_with_deinit_passes_type_checking() {
    let text = "class Resource(val fd: Int) {\n\
                    deinit() {}\n\
                }\n\
                fun use_resource(): Unit {\n\
                    val r = Resource(42)\n\
                }";
    for typed in checked_entries(text) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let resource_desc = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.nominal())
            .find(|d| d.has_deinit());
        assert!(
            resource_desc.is_some(),
            "Resource should have has_deinit = true"
        );
    }
}

#[test]
fn class_with_duplicate_deinit_is_rejected() {
    let text = "class Resource {\n\
                    deinit() {}\n\
                    deinit() {}\n\
                }";
    for typed in checked_entries(text) {
        assert_eq!(codes(typed.diagnostics()), ["L0097"]);
    }
}

#[test]
fn intrinsic_replace_and_swap_pass_type_checking() {
    let text = "class Item(val value: Int)\n\
                fun test_replace_and_swap(): Unit {\n\
                    var a = Item(1)\n\
                    var b = Item(2)\n\
                    val old = replace(&a, Item(3))\n\
                    swap(&a, &b)\n\
                }";
    for typed in checked_entries(text) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.calls().len(), 2);
    }
}

#[test]
fn intrinsic_replace_rejects_missing_inout_marker() {
    let text = "class Item(val value: Int)\n\
                fun test(): Unit {\n\
                    var a = Item(1)\n\
                    val old = replace(a, Item(2))\n\
                }";
    for typed in checked_entries(text) {
        assert_eq!(codes(typed.diagnostics()), ["L0122"]);
    }
}

#[test]
fn intrinsic_replace_rejects_temporary_place() {
    let text = "class Item(val value: Int)\n\
                fun test(): Unit {\n\
                    val old = replace(&Item(1), Item(2))\n\
                }";
    for typed in checked_entries(text) {
        assert_eq!(codes(typed.diagnostics()), ["L0122"]);
    }
}

#[test]
fn intrinsic_swap_rejects_mismatched_types() {
    let text = "fun test(): Unit {\n\
                    var a = 1\n\
                    var b = \"string\"\n\
                    swap(&a, &b)\n\
                }";
    for typed in checked_entries(text) {
        assert_eq!(codes(typed.diagnostics()), ["L0084"]);
    }
}
