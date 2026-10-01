//! SPEC-0236 / SPEC-0232 的 Phase 2 交叉验收；不声明原子置换的 ownership/native 能力。

use lang_frontend::{
    ast::ExpressionId,
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::{Expression, ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, CallableTarget, CompilationUnitTypes, Copyability, DeferredReason,
        ExpressionCategory, OwnershipPrimitiveKind, ParameterMode, StringOperationKind, TypeKind,
        TypedFile, UnitCallTarget, UnitExpressionId, UnitTypeKind, check_compilation_unit_types,
        check_types, standard_environments,
    },
};

const INT_OVERLOAD: &str = "fun choose(callback: (Int) -> Int): Int = 1";
const STRING_OVERLOAD: &str = "fun choose(callback: (String) -> String): String = \"chosen\"";
const SELECTED_CALL: &str = r#"
fun run(): String = choose({ value ->
    var current = "left"
    var spare = "right"
    val old = replace(&current, value.clone())
    swap(&current, &spare)
    old.clone()
})
"#;

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn parse(sources: &mut SourceMap, path: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(path, text).expect("source");
    let lexed = lex(sources, source).expect("lex");
    let file = parse_file(sources, &lexed).expect("parse");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    (source, file)
}

fn single(text: &str) -> (ParsedFile, TypedFile) {
    let mut sources = SourceMap::new();
    let (_, file) = parse(&mut sources, "integration.ko", text);
    let (names, environment) = standard_environments();
    let names = resolve_names(&sources, &file, &names).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &environment).expect("types");
    let repeated = check_types(&sources, &file, &names, &environment).expect("repeated types");
    assert_eq!(typed.diagnostics(), repeated.diagnostics());
    assert_eq!(typed.string_operations(), repeated.string_operations());
    assert_eq!(
        typed.ownership_primitives(),
        repeated.ownership_primitives()
    );
    assert_eq!(typed.calls(), repeated.calls());
    (file, typed)
}

fn unit_pair(texts: &[(&str, &str)]) -> (CompilationUnitTypes, CompilationUnitTypes) {
    let mut sources = SourceMap::new();
    let files = texts
        .iter()
        .map(|(path, text)| parse(&mut sources, path, text))
        .collect::<Vec<_>>();
    let mut inputs = texts
        .iter()
        .zip(&files)
        .map(|((path, _), (source, file))| SourceUnitInput::new("root", path, *source, file))
        .collect::<Vec<_>>();
    let (names, environment) = standard_environments();
    // 两次分析共享 SourceMap 和环境，诊断 Span 才可比较来源身份。
    let check = |inputs: &[SourceUnitInput<'_>]| {
        let index = index_compilation_unit(&sources, inputs).expect("index");
        let names = resolve_compilation_unit_names(&sources, inputs, &index, &names)
            .expect("unit names")
            .validate()
            .expect("valid unit names");
        let typed = check_compilation_unit_types(&sources, inputs, &names, &environment)
            .expect("unit types");

        // 跨文件 facts 必须指回原文件内的真实 AST operand，不能只保留相同的局部 ID。
        for source in names.names().index().source_units() {
            let (_, file) = files
                .iter()
                .find(|(id, _)| *id == source.source_id())
                .expect("source file");
            for fact in typed
                .ownership_primitives()
                .iter()
                .filter(|fact| fact.expression().source_unit() == source.id())
            {
                assert_eq!(
                    fact.operands(),
                    operands(file, fact.expression().expression())
                        .map(|operand| UnitExpressionId::new(source.id(), operand))
                );
            }
            for fact in typed
                .string_operations()
                .iter()
                .filter(|fact| fact.expression().source_unit() == source.id())
            {
                assert_eq!(
                    fact.receiver(),
                    UnitExpressionId::new(
                        source.id(),
                        receiver(file, fact.expression().expression())
                    )
                );
            }
        }
        typed
    };
    let forward = check(&inputs);
    inputs.reverse();
    let reverse = check(&inputs);
    (forward, reverse)
}

fn operands(file: &ParsedFile, expression: ExpressionId) -> [ExpressionId; 2] {
    let Expression::Call { arguments, .. } = file
        .ast()
        .expressions()
        .get(expression)
        .expect("call")
        .payload()
    else {
        panic!("primitive must be a call");
    };
    assert_eq!(arguments.len(), 2);
    [arguments[0].value, arguments[1].value]
}

fn receiver(file: &ParsedFile, expression: ExpressionId) -> ExpressionId {
    let Expression::Call { callee, .. } = file
        .ast()
        .expressions()
        .get(expression)
        .expect("call")
        .payload()
    else {
        panic!("clone must be a call");
    };
    let Expression::Member { receiver, .. } = file
        .ast()
        .expressions()
        .get(*callee)
        .expect("member")
        .payload()
    else {
        panic!("clone must have a receiver");
    };
    *receiver
}

fn modes(kind: OwnershipPrimitiveKind) -> [ParameterMode; 2] {
    match kind {
        OwnershipPrimitiveKind::Replace => [ParameterMode::Inout, ParameterMode::Value],
        OwnershipPrimitiveKind::Swap => [ParameterMode::Inout, ParameterMode::Inout],
    }
}

fn assert_single_contracts(file: &ParsedFile, typed: &TypedFile) {
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let clones = typed.string_operations();
    let primitives = typed.ownership_primitives();
    assert_eq!(clones.len(), 2);
    assert_eq!(primitives.len(), 2);
    assert!(clones[0].expression().index() < clones[1].expression().index());
    assert!(primitives[0].expression().index() < primitives[1].expression().index());
    assert_eq!(primitives[0].kind(), OwnershipPrimitiveKind::Replace);
    assert_eq!(primitives[1].kind(), OwnershipPrimitiveKind::Swap);
    assert_eq!(primitives[0].operands()[1], clones[0].expression());

    for fact in clones {
        assert_eq!(typed.string_operation(fact.expression()), Some(*fact));
        assert_eq!(fact.receiver(), receiver(file, fact.expression()));
        assert_eq!(fact.kind(), StringOperationKind::Clone);
        assert_eq!(fact.receiver_mode(), ParameterMode::Borrow);
        assert_eq!(fact.result_mode(), ParameterMode::Value);
        assert_eq!(
            typed.expression_type(fact.receiver()),
            Some(fact.receiver_type())
        );
        assert_eq!(
            typed.expression_type(fact.expression()),
            Some(fact.result_type())
        );
        assert_eq!(
            typed.types().get(fact.result_type()),
            Some(&TypeKind::Builtin(BuiltinType::String))
        );
        assert_eq!(
            typed.copyability(fact.result_type()),
            Some(Copyability::MoveOnly)
        );
        assert_eq!(
            typed.expression_category(fact.expression()),
            Some(ExpressionCategory::Temporary)
        );
    }
    for fact in primitives {
        assert_eq!(typed.ownership_primitive(fact.expression()), Some(*fact));
        assert_eq!(fact.operands(), operands(file, fact.expression()));
        assert_eq!(fact.value_type(), clones[0].result_type());
        let call = typed
            .call(fact.expression())
            .expect("primitive call contract");
        assert!(matches!(
            call.instance().target(),
            CallableTarget::External(_)
        ));
        assert_eq!(call.instance().type_arguments(), &[fact.value_type()]);
        assert_eq!(
            call.arguments()
                .iter()
                .map(|argument| argument.mode())
                .collect::<Vec<_>>(),
            modes(fact.kind())
        );
    }
}

fn assert_unit_contracts(typed: &CompilationUnitTypes, bodies: usize) {
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let clones = typed.string_operations();
    let primitives = typed.ownership_primitives();
    assert_eq!(
        clones.len(),
        bodies * 2,
        "unit expression types: {:?}; type table: {:?}",
        typed.expression_types(),
        typed.types()
    );
    assert_eq!(primitives.len(), bodies * 2);
    assert!(
        clones
            .windows(2)
            .all(|pair| pair[0].expression() < pair[1].expression())
    );
    assert!(
        primitives
            .windows(2)
            .all(|pair| pair[0].expression() < pair[1].expression())
    );
    for (clones, primitives) in clones.chunks_exact(2).zip(primitives.chunks_exact(2)) {
        assert_eq!(primitives[0].kind(), OwnershipPrimitiveKind::Replace);
        assert_eq!(primitives[1].kind(), OwnershipPrimitiveKind::Swap);
        assert_eq!(primitives[0].operands()[1], clones[0].expression());
    }
    for fact in clones {
        assert_eq!(typed.string_operation(fact.expression()), Some(*fact));
        assert_eq!(
            fact.receiver().source_unit(),
            fact.expression().source_unit()
        );
        assert_eq!(fact.kind(), StringOperationKind::Clone);
        assert_eq!(fact.receiver_mode(), ParameterMode::Borrow);
        assert_eq!(fact.result_mode(), ParameterMode::Value);
        assert_eq!(
            typed.expression_type(fact.receiver()),
            Some(fact.receiver_type())
        );
        assert_eq!(
            typed.expression_type(fact.expression()),
            Some(fact.result_type())
        );
        assert_eq!(
            typed.types().get(fact.result_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::String))
        );
        assert_eq!(typed.copyability(fact.result_type()), Copyability::MoveOnly);
        assert_eq!(
            typed.expression_category(fact.expression()),
            Some(ExpressionCategory::Temporary)
        );
    }
    for fact in primitives {
        assert_eq!(typed.ownership_primitive(fact.expression()), Some(*fact));
        assert_eq!(
            typed.types().get(fact.value_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::String))
        );
        let call = typed
            .call(fact.expression())
            .expect("primitive call contract");
        assert!(matches!(
            call.instance().target(),
            UnitCallTarget::External(_)
        ));
        assert_eq!(call.instance().type_arguments(), &[fact.value_type()]);
        assert_eq!(
            call.arguments()
                .iter()
                .map(|argument| argument.mode())
                .collect::<Vec<_>>(),
            modes(fact.kind())
        );
    }
    let validated = typed.clone().validate().expect("valid unit types");
    assert_eq!(validated.types().string_operations(), clones);
    assert_eq!(validated.types().ownership_primitives(), primitives);
}

#[test]
fn direct_clone_replace_and_swap_keep_both_typed_contracts() {
    let text = r#"
fun run(value: String): String {
    var current = "left"
    var spare = "right"
    val old = replace(&current, value.clone())
    swap(&current, &spare)
    return old.clone()
}
"#;
    let (file, typed) = single(text);
    assert_single_contracts(&file, &typed);
    assert_eq!(typed.calls().len(), 2);
    let (typed, _) = unit_pair(&[("direct.ko", text)]);
    assert_unit_contracts(&typed, 1);
    assert_eq!(typed.calls().len(), 2);
}

#[test]
fn selected_overload_trial_commits_each_intrinsic_once_in_either_candidate_order() {
    for declarations in [
        format!("{INT_OVERLOAD}\n{STRING_OVERLOAD}"),
        format!("{STRING_OVERLOAD}\n{INT_OVERLOAD}"),
    ] {
        let text = format!("{declarations}\n{SELECTED_CALL}");
        let (file, typed) = single(&text);
        assert_single_contracts(&file, &typed);
        assert_eq!(typed.calls().len(), 3);
        let (typed, _) = unit_pair(&[("selected.ko", &text)]);
        assert_unit_contracts(&typed, 1);
        assert_eq!(typed.calls().len(), 3);
    }
}

#[test]
fn failed_and_ambiguous_trials_leak_neither_intrinsic_family() {
    for (declarations, result, expected) in [
        (
            format!("{INT_OVERLOAD}\n{STRING_OVERLOAD}"),
            "true",
            "L0123",
        ),
        (
            "fun choose(callback: (Int) -> String): Int = 1\nfun choose(callback: (String) -> String): Int = 2".to_string(),
            "old.clone()",
            "L0124",
        ),
    ] {
        // clone/replace/swap 均先建立于候选 body 内，最后才失败或形成歧义。
        let use_text = format!(r#"
fun run(): Unit {{
    choose({{ ignored ->
        var current = "left"
        var spare = "right"
        val old = replace(&current, "source".clone())
        swap(&current, &spare)
        {result}
    }})
}}
"#);
        let text = format!("{declarations}\n{use_text}");
        let (_, typed) = single(&text);
        assert_eq!(codes(typed.diagnostics()), [expected]);
        assert!(typed.string_operations().is_empty());
        assert!(typed.ownership_primitives().is_empty());
        // 普通 call facts 不由 primitive 的全局错误清理代替 trial rollback。
        assert!(typed.calls().is_empty());
        for texts in [
            vec![("failed.ko", text.as_str())],
            vec![("z_use.ko", use_text.as_str()), ("a_api.ko", declarations.as_str())],
        ] {
            let (forward, reverse) = unit_pair(&texts);
            assert_eq!(codes(forward.diagnostics()), [expected]);
            assert_eq!(forward.diagnostics(), reverse.diagnostics());
            for typed in [forward, reverse] {
                assert!(typed.string_operations().is_empty());
                assert!(typed.ownership_primitives().is_empty());
                assert!(typed.calls().is_empty());
                assert!(typed.validate().is_err());
            }
        }
    }
}

#[test]
fn cross_file_trials_keep_source_qualified_facts_in_stable_order() {
    let declarations = format!("{INT_OVERLOAD}\n{STRING_OVERLOAD}");
    let other = SELECTED_CALL.replace("fun run()", "fun other()");
    // SourceMap 的装载顺序、输入枚举顺序都刻意不同于逻辑路径顺序。
    let texts = [
        ("z_use.ko", SELECTED_CALL),
        ("a_api.ko", declarations.as_str()),
        ("m_use.ko", other.as_str()),
    ];
    let (forward, reverse) = unit_pair(&texts);
    assert_unit_contracts(&forward, 2);
    assert_unit_contracts(&reverse, 2);
    assert_eq!(forward.string_operations(), reverse.string_operations());
    assert_eq!(
        forward.ownership_primitives(),
        reverse.ownership_primitives()
    );
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls().len(), 6);
    let clones = forward.string_operations();
    let primitives = forward.ownership_primitives();
    assert_ne!(
        clones[0].expression().source_unit(),
        clones[2].expression().source_unit()
    );
    assert_eq!(
        clones[0].expression().expression(),
        clones[2].expression().expression()
    );
    assert_ne!(
        primitives[0].expression().source_unit(),
        primitives[2].expression().source_unit()
    );
    assert_eq!(
        primitives[0].expression().expression(),
        primitives[2].expression().expression()
    );
}

#[test]
fn viable_deferred_candidate_still_prevents_premature_intrinsic_commit() {
    let ready = "fun choose(callback: (Ready) -> String): String = \"ready\"";
    let pending = "fun choose(callback: (Pending) -> String): String = \"pending\"";
    let classes = "class Ready(val text: String)\nclass Pending";
    let use_text = r#"
fun run(): String = choose({ value ->
    var current = "left"
    var spare = "right"
    val old = replace(&current, "source".clone())
    swap(&current, &spare)
    value.text.clone()
})
"#;
    let ready_only = format!("{classes}\n{ready}\n{use_text}");
    let (typed, _) = unit_pair(&[("ready.ko", &ready_only)]);
    assert_unit_contracts(&typed, 1);

    for declarations in [format!("{ready}\n{pending}"), format!("{pending}\n{ready}")] {
        // Pending 的 member 未定且无错误，不能被当成已淘汰候选而选中 Ready。
        let text = format!("{classes}\n{declarations}\n{use_text}");
        let (forward, reverse) = unit_pair(&[("pending.ko", &text)]);
        for typed in [forward, reverse] {
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            assert!(typed.string_operations().is_empty());
            assert!(typed.ownership_primitives().is_empty());
            assert!(typed.calls().is_empty());
            assert!(typed.expression_types().values().any(|ty| {
                matches!(
                    typed.types().get(*ty),
                    Some(UnitTypeKind::Deferred(DeferredReason::Call))
                )
            }));
        }
    }
}
