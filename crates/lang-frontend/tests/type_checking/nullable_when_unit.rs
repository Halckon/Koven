//! 同一重复解析 AST、自定义环境和真实 source 身份的双 raw type 入口。
#[path = "nullable_when_unit/flow.rs"]
mod flow;

#[path = "nullable_when_unit/mutation.rs"]
mod mutation;

#[path = "nullable_when_unit/identity.rs"]
mod identity;

#[path = "nullable_when_unit/remaining_case.rs"]
mod remaining_case;

use lang_frontend::{
    name_resolution::{
        SourceUnitId, SourceUnitInput, SourceUnitKey, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{CompilationUnitTypes, check_compilation_unit_types},
};

fn checked_entries(
    text: &str,
) -> (
    SourceMap,
    ParsedFile,
    [(SourceUnitId, CompilationUnitTypes); 2],
) {
    let (sources, file) = super::parse(text);
    let (environment, types) = super::environments();
    let entries = [false, true].map(|rooted| {
        let input = if rooted {
            SourceUnitInput::new("root", "types.ko", file.source_id(), &file)
        } else {
            SourceUnitInput::memory_document(file.source_id(), &file)
        };
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
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
        let names = names.validate().expect("valid unit names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .expect("raw unit types");
        assert!(
            typed
                .expression_types()
                .keys()
                .all(|id| id.source_unit() == source.id())
        );
        (source.id(), typed)
    });
    (sources, file, entries)
}
