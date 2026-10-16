//! SPEC-0329：以真实 Phase 2 产物检验数值迭代描述符的两条准入边界。
use super::CompilationUnitTypes;
use crate::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::{ParsedFile, Statement, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, IterationProvider, ParameterMode, UnitExpressionId,
        UnitSequentialIterationBinding, UnitSequentialIterationDescriptor, UnitStatementId,
        UnitTypeKind, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique source");
    let lexed = lex(sources, source).expect("lexing succeeds internally");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let file = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    (source, file)
}

fn assert_rejected_by_both_admissions(
    typed: &CompilationUnitTypes,
    case: &str,
    mutate: impl FnOnce(&mut UnitSequentialIterationDescriptor),
) {
    let mut invalid = typed.clone();
    mutate(&mut invalid.iterations[0]);
    assert!(!invalid.numeric_range_iterations_are_valid(), "{case}");
    assert!(invalid.clone().validate().is_err(), "basic: {case}");
    assert!(
        invalid.validate_constants().is_err(),
        "const-enabled: {case}"
    );
}

#[test]
fn numeric_range_descriptor_corruption_is_rejected_by_both_admissions() {
    for (range_name, range, endpoint, other_range, other_endpoint) in [
        (
            "IntRange",
            BuiltinType::IntRange,
            BuiltinType::Int,
            BuiltinType::LongRange,
            BuiltinType::Long,
        ),
        (
            "LongRange",
            BuiltinType::LongRange,
            BuiltinType::Long,
            BuiltinType::IntRange,
            BuiltinType::Int,
        ),
    ] {
        let mut sources = SourceMap::new();
        let (left_source, left) = parsed(
            &mut sources,
            "a.ko",
            &format!("package p\nfun left(input:{range_name}):Unit{{for(item in input){{}}}}"),
        );
        let (right_source, right) = parsed(
            &mut sources,
            "b.ko",
            &format!("package p\nfun right(input:{range_name}):Unit{{for(item in input){{}}}}"),
        );
        let inputs = [
            SourceUnitInput::new("root", "p/a.ko", left_source, &left),
            SourceUnitInput::new("root", "p/b.ko", right_source, &right),
        ];
        let (name_environment, type_environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).expect("valid unit inputs");
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .expect("name resolution succeeds internally")
            .validate()
            .expect("valid names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("type checking succeeds internally");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        complete_compilation_unit_types(&sources, &inputs, &names, &type_environment, &typed)
            .expect("ordinary sources complete type admission");

        let plans = typed.sequential_iterations();
        assert_eq!(plans.len(), 2, "{range_name}");
        for ((source, file), plan) in [(left_source, &left), (right_source, &right)]
            .into_iter()
            .zip(plans)
        {
            let source_unit = names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|unit| unit.source_id() == source)
                .expect("source belongs to this unit")
                .id();
            let loops = file
                .ast()
                .statements()
                .iter()
                .filter_map(|(statement, node)| match node.payload() {
                    Statement::For { source, .. } => Some((statement, *source)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(loops.len(), 1);
            assert_eq!(
                plan.statement(),
                UnitStatementId::new(source_unit, loops[0].0)
            );
            assert_eq!(
                plan.source(),
                UnitExpressionId::new(source_unit, loops[0].1)
            );
            assert_eq!(plan.provider(), IterationProvider::NumericRange(endpoint));
            assert_eq!(plan.delivery(), ParameterMode::Value);
            assert_eq!(
                typed.expression_type(plan.source()),
                Some(plan.source_type())
            );
            assert_eq!(
                typed.types().get(plan.source_type()),
                Some(&UnitTypeKind::Builtin(range))
            );
            assert_eq!(
                typed.types().get(plan.element_type()),
                Some(&UnitTypeKind::Builtin(endpoint))
            );
            let UnitSequentialIterationBinding::Name(symbol) = plan.binding() else {
                panic!("ordinary source must publish a named binding");
            };
            assert_eq!(symbol.source_unit(), source_unit);
            assert_eq!(typed.symbol_type(*symbol), Some(plan.element_type()));
        }
        assert!(typed.numeric_range_iterations_are_valid());
        assert!(
            typed.constants().is_some(),
            "empty constant facts are published"
        );
        assert!(typed.clone().validate().is_ok(), "basic baseline");
        assert!(
            typed.clone().validate_constants().is_ok(),
            "const-enabled baseline"
        );

        let other_range_type = typed.types().builtin(other_range).expect("range builtin");
        let other_element_type = typed
            .types()
            .builtin(other_endpoint)
            .expect("endpoint builtin");
        let second_source = plans[1].source();
        let second_binding = plans[1].binding().clone();
        assert_ne!(plans[0].source().source_unit(), second_source.source_unit());
        assert_eq!(plans[0].source_type(), plans[1].source_type());
        assert_eq!(plans[0].element_type(), plans[1].element_type());

        assert_rejected_by_both_admissions(&typed, "unsupported endpoint", |plan| {
            plan.provider = IterationProvider::NumericRange(BuiltinType::Byte);
        });
        assert_rejected_by_both_admissions(&typed, "other numeric provider", |plan| {
            plan.provider = IterationProvider::NumericRange(other_endpoint);
        });
        assert_rejected_by_both_admissions(&typed, "other range source type", |plan| {
            plan.source_type = other_range_type;
        });
        assert_rejected_by_both_admissions(&typed, "other element type", |plan| {
            plan.element_type = other_element_type;
        });
        // 同型替换只检验跨 source-unit 身份，不宣称 guard 比对具体 AST operand。
        assert_rejected_by_both_admissions(&typed, "cross-source expression", |plan| {
            plan.source = second_source;
        });
        assert_rejected_by_both_admissions(&typed, "cross-source binding", |plan| {
            plan.binding = second_binding;
        });
        assert_rejected_by_both_admissions(&typed, "numeric destructuring", |plan| {
            plan.binding = UnitSequentialIterationBinding::Destructure(vec![]);
        });
    }
}
