//! SPEC-0226 runtime plans must retain source identity and exclude initializer/recovery work.
use super::{analysis, constants_tests::analyze};
use crate::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::ConstantMaterializationKind,
    parser::parse_file,
    source::SourceMap,
    type_checking::{ConstValue, check_compilation_unit_types, standard_environments},
};

#[test]
fn all_constant_types_keep_exact_values_and_source_qualified_runtime_identity() {
    let types = [
        ("Boolean", "true"),
        ("Byte", "1"),
        ("Short", "2"),
        ("Int", "3"),
        ("Long", "4"),
        ("UByte", "5u"),
        ("UShort", "6u"),
        ("UInt", "7u"),
        ("ULong", "8uL"),
        ("Char", "'文'"),
        ("String", "\"字串\""),
    ];
    let body = types
        .iter()
        .enumerate()
        .map(|(i, (ty, value))| {
            format!("const val C{i}: {ty} = {value}\nfun f{i}(): {ty} = OTHER.C{i}\n")
        })
        .collect::<String>();
    let mut sources = SourceMap::new();
    let a = sources
        .add_source("a.ko", format!("package a\n{}", body.replace("OTHER", "b")))
        .unwrap();
    let b = sources
        .add_source("b.ko", format!("package b\n{}", body.replace("OTHER", "a")))
        .unwrap();
    let pa = parse_file(&sources, &lex(&sources, a).unwrap()).unwrap();
    let pb = parse_file(&sources, &lex(&sources, b).unwrap()).unwrap();
    let inputs = [
        SourceUnitInput::new("root", "a/source.ko", a, &pa),
        SourceUnitInput::new("root", "b/source.ko", b, &pb),
    ];
    let (ne, te) = standard_environments();
    let mut previous = None;
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te)
            .unwrap()
            .validate_constants()
            .unwrap();
        let owned = analysis::analyze(&sources, &inputs, &names, &te, typed.types()).unwrap();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let plans = owned
            .constant_materializations
            .expect("complete runtime plans");
        assert_eq!(plans.len(), 22);
        for (plan, expected) in plans.iter().zip(typed.constants().uses()) {
            assert_ne!(
                plan.descriptor.expression().source_unit(),
                plan.descriptor.target().source_unit(),
                "runtime reads must target the other source"
            );
            assert_eq!(
                &plan.descriptor, expected,
                "preserve the exact typed value and target"
            );
            assert_eq!(
                plan.kind,
                if matches!(expected.value(), ConstValue::String(_)) {
                    ConstantMaterializationKind::StringTemporary
                } else {
                    ConstantMaterializationKind::InlineCopy
                }
            );
        }
        assert_eq!(
            plans[0].descriptor.expression().expression(),
            plans[11].descriptor.expression().expression()
        );
        assert_ne!(
            plans[0].descriptor.expression(),
            plans[11].descriptor.expression()
        );
        if let Some(previous) = &previous {
            assert_eq!(&plans, previous);
        }
        previous = Some(plans);
    }
}

#[test]
fn only_runtime_reads_on_executable_paths_are_materialized() {
    let owned = analyze(
        "package a\nconst val TEXT = \"s\"\nconst val COPY = TEXT\nfun stop(): Nothing = stop()\nfun read(flag: Boolean): String = if (flag) { COPY } else { TEXT }\nfun dead(): Unit { return\nval unused = TEXT }\nfun abort(): Unit { val stopped = stop()\nval unused = TEXT }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let plans = owned.constant_materializations.expect("runtime plans");
    assert_eq!(plans.len(), 2, "only the two dynamic branch reads execute");
    assert_ne!(
        plans[0].descriptor.expression(),
        plans[1].descriptor.expression()
    );
}

#[test]
fn ownership_error_or_deferred_fact_prevents_partial_materialization_publication() {
    for (text, has_error) in [
        (
            "package a\nconst val TEXT = \"s\"\nfun take(own s: String): Unit {}\nfun bad(s: String): Unit { val first = take(TEXT)\nval second = take(s) }",
            true,
        ),
        (
            "package a\nconst val TEXT = \"s\"\nclass Resource {}\nclass Holder(var payload: Resource)\nfun read(): String = TEXT\nfun deferred(holders: List<Holder>): Unit { val projected = holders[0].payload }",
            false,
        ),
    ] {
        let owned = analyze(text);
        assert_eq!(!owned.diagnostics().is_empty(), has_error);
        if !has_error {
            assert!(!owned.deferred().is_empty());
        }
        assert!(
            owned.constant_materializations.is_none(),
            "no partial executable capability"
        );
    }
}
