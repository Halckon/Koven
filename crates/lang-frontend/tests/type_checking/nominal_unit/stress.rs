//! 原 128 cases / 96 conditions 输入的 Unit 默认栈回归。
//! 此输入历史上曾 Abort，受限制的本机验证只编译；正式 CI 实跑。
use super::super::{environments, parse};
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    type_checking::check_compilation_unit_types,
};

#[test]
fn long_enum_domains_and_deep_smart_cast_conditions_stay_deterministic() {
    let mut text = String::from("enum class Large { ");
    for index in 0..128 {
        if index != 0 {
            text.push_str(", ");
        }
        text.push_str(&format!("C{index}"));
    }
    text.push_str(" }\nfun select(input: Large): Int = when (input) {\n");
    for index in 0..128 {
        text.push_str(&format!("Large.C{index} -> {index}\n"));
    }
    text.push_str("}\n");
    text.push_str("enum class Shape { Circle(radius: Int), Point }\n");
    text.push_str("fun deep(shape: Shape): Boolean = shape is Shape.Circle");
    for _ in 0..96 {
        text.push_str(" && shape.radius > 0");
    }
    text.push('\n');

    let (sources, parsed) = parse(&text);
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
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("unit names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid names");
        let first = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("first");
        let second =
            check_compilation_unit_types(&sources, &inputs, &names, &types).expect("second");
        assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
        assert_eq!(first.diagnostics(), second.diagnostics());
        assert_eq!(first.expression_types(), second.expression_types());
        let cases = |typed: &lang_frontend::type_checking::CompilationUnitTypes| {
            typed
                .signatures()
                .declarations()
                .iter()
                .filter_map(|declaration| declaration.nominal())
                .filter(|nominal| !nominal.enum_cases().is_empty())
                .map(|nominal| nominal.enum_cases().to_vec())
                .collect::<Vec<_>>()
        };
        let first_cases = cases(&first);
        assert_eq!(
            first_cases.iter().map(Vec::len).collect::<Vec<_>>(),
            [128, 2]
        );
        assert_eq!(first_cases, cases(&second));
    }
}
