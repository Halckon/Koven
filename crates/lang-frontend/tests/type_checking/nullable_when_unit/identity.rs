//! 原同一 subject 求值与重复类型检查的稳定身份消费者。
use super::super::{environments, parse};
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SourceUnitKey, index_compilation_unit, resolve_compilation_unit_names,
    },
    type_checking::check_compilation_unit_types,
};

#[test]
fn nullable_when_plans_have_stable_identity_and_single_subject_evaluation() {
    let text = "fun subject(): Int? = null
        fun extract(): Int = when (subject()) { null -> 0; else -> 1 }";
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
        let names = names.validate().expect("valid names");
        // 两次 raw typing 重用同一 AST、输入、名称产物与类型环境。
        let typed =
            check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit types");
        let repeated = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .expect("repeat unit types");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.nullable_whens(), repeated.nullable_whens());
        assert_eq!(typed.nullable_whens().len(), 1);
        let plan = &typed.nullable_whens()[0];
        assert_eq!(plan.expression().source_unit(), source.id());
        assert_eq!(plan.subject().source_unit(), source.id());
        assert_eq!(
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(plan.subject().expression())
                        .expect("subject")
                        .span()
                )
                .expect("text"),
            "subject()"
        );
        assert_eq!(
            typed
                .calls()
                .iter()
                .filter(|call| call.expression() == plan.subject())
                .count(),
            1
        );
        assert_eq!(plan.entries().len(), 2);
    }
}
