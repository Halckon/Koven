//! 原构造顺序、具名实参映射与同一分析输入的重复检查消费者。
use super::super::{environments, parse};
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SourceUnitKey, index_compilation_unit, resolve_compilation_unit_names,
    },
    type_checking::{UnitConstructionTarget, check_compilation_unit_types},
};

#[test]
fn nominal_and_payload_constructions_publish_instantiated_value_mappings() {
    let text = "class Resource {}
                class Holder<T>(val item: T)
                value class Pair<T>(val first: T, val second: T)
                enum class Maybe<T> { Some(item: T), None }
                fun resource(): Resource = Resource()
                fun holder(): Holder<Int> = Holder(1)
                fun pair(): Pair<Int> = Pair(second = 2, first = 1)
                fun some(): Maybe<Int> = Maybe.Some(1)";
    let (sources, parsed) = parse(text);
    let (environment, types) = environments();
    for rooted in [false, true] {
        let input = if rooted {
            SourceUnitInput::new("root", "types.ko", parsed.source_id(), &parsed)
        } else {
            SourceUnitInput::memory_document(parsed.source_id(), &parsed)
        };
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [source] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(source.source_id(), parsed.source_id());
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

        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("types");
        let repeated = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .expect("repeated types");

        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.constructions(), repeated.constructions());
        assert_eq!(typed.constructions().len(), 4);
        assert!(typed.calls().is_empty());
        assert_eq!(typed.constructions()[0].arguments(), []);
        assert_eq!(
            typed.constructions()[1].instance().type_arguments().len(),
            1
        );
        let pair = &typed.constructions()[2];
        assert_eq!(pair.arguments()[0].parameter_name(), "first");
        assert_eq!(pair.arguments()[0].evaluation_index(), 1);
        assert_eq!(pair.arguments()[1].parameter_name(), "second");
        assert_eq!(pair.arguments()[1].evaluation_index(), 0);
        assert!(matches!(
            typed.constructions()[3].target(),
            UnitConstructionTarget::EnumCase(_)
        ));
        for construction in typed.constructions() {
            assert_eq!(construction.expression().source_unit(), source.id());
            assert!(
                construction
                    .arguments()
                    .iter()
                    .all(|argument| argument.argument().source_unit() == source.id())
            );
            assert_eq!(
                typed.expression_type(construction.expression()),
                Some(construction.result_type())
            );
        }
    }
}
