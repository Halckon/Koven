//! SPEC-0331：固定 Map provider 两源的普通 single/basic native 成功合同。
//! 复用 SSA/LLVM gate 的源码与 golden；运行须等对应四条 pipeline 全绿后独立准入。
//! 既有 object/link/run helper 不提供内置硬超时；此处不另造 timer 框架。

use std::process::{Command, Output};

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
    parser::{Item, NameMarker, parse_file},
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use crate::{
    emit_native_owned_unit_object,
    native_tests::{TestDirectory, emit_link_and_run},
    ssa::map_provider_pipeline_tests::sources as provider_sources,
};

#[test]
fn single_map_provider_normal_sources_exit_normally() {
    for (case, source, expected) in provider_sources() {
        let run = emit_link_and_run("provider.ko", source, "entry");
        assert_result(case, "single", &run, expected);
    }
}

#[test]
fn unit_map_provider_normal_sources_exit_normally() {
    for (case, source, expected) in provider_sources() {
        let run = run_unit(case, source);
        assert_result(case, "unit", &run, expected);
    }
}

fn assert_result(case: &str, entry: &str, run: &Output, expected: &str) {
    assert_eq!(run.status.code(), Some(0), "{case} {entry}: {run:?}");
    assert_eq!(run.stdout, expected.as_bytes(), "{case} {entry}: {run:?}");
    assert!(run.stderr.is_empty(), "{case} {entry}: {run:?}");
    println!("SPEC0331 NATIVE case={case} entry={entry} exit=0 stdout=golden stderr=empty");
}

fn run_unit(case: &str, source_text: &str) -> Output {
    run_unit_with_entry(case, "provider.ko", source_text, "entry")
}

fn run_unit_with_entry(case: &str, path: &str, source_text: &str, entry_name: &str) -> Output {
    // 与四 pipeline gate 保持同一默认包、逻辑路径、原字节及真实 entry。
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(path, source_text)
        .expect("unique source");
    let lexed = lex(&sources, source).expect("lexing succeeds");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let file = parse_file(&sources, &lexed).expect("parsing succeeds");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    assert_eq!(file.source_id(), source);
    assert_eq!(sources.source_name(source), Ok(path));
    assert!(file.package().is_none(), "{case}: expected default package");
    let inputs = [SourceUnitInput::new("root", path, source, &file)];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
    assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("name resolution succeeds")
        .validate()
        .expect("valid names");
    let [source_unit] = names.names().index().source_units() else {
        panic!("{case}: expected exactly one original source unit");
    };
    assert_eq!(source_unit.source_id(), source);
    assert_eq!(
        source_unit.key().root().expect("rooted source").as_str(),
        "root"
    );
    assert_eq!(
        source_unit
            .key()
            .logical_path()
            .expect("rooted source")
            .as_str(),
        path
    );
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == entry_name
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == source_unit.id()
                && declaration.package() == source_unit.package()
                && declaration.name_span().source_id() == source
                && names.names().index().packages().get(declaration.package().index())
                    .is_some_and(|package| package.name().expect("named package").is_default())
                && file.roots().contains(&declaration.root())
                && file.ast().items().get(declaration.root()).is_ok_and(|node| {
                    matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. } if *name == declaration.name_span())
                })
        })
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        panic!("{case}: expected exactly one real default-package entry in {path}: {entries:?}");
    };
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .expect("type checking succeeds");
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    {
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw)
                .expect("complete types");
        assert!(complete.is_same_analysis(&raw));
        assert!(std::ptr::eq(complete.types(), &raw));
        assert!(raw.is_compatible_with(&sources, &inputs, &names, &environment));
    }
    let typed = raw.validate().expect("valid basic types");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("ownership checking succeeds");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(owned.is_compatible_with(&typed));
    let owned = owned.validate().expect("valid complete ownership facts");
    let view = owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
        .expect("matching original facts construct the owned view");
    assert!(std::ptr::eq(view.sources(), &sources));
    assert!(std::ptr::eq(view.inputs(), inputs.as_slice()));
    assert!(std::ptr::eq(view.names(), &names));
    assert!(std::ptr::eq(view.types(), typed.types()));
    assert!(std::ptr::eq(view.ownership(), owned.ownership()));

    let directory = TestDirectory::create();
    let object = directory.join("program.o");
    let executable = directory.join("program");
    emit_native_owned_unit_object(&view, entry.id(), &object)
        .expect("Map provider source must emit a native unit object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{case}: {linked:?}");
    Command::new(&executable)
        .output()
        .expect("linked Map provider executable must launch")
}

#[path = "native_map_provider_exits_tests.rs"]
mod exits_tests;

#[path = "native_map_provider_conditional_tests.rs"]
mod conditional_tests;

#[path = "native_map_provider_temporary_tests.rs"]
mod temporary_tests;

#[path = "native_map_provider_d10_w09_tests.rs"]
mod d10_w09;

#[path = "native_map_provider_nested_tests.rs"]
mod nested_tests;

#[path = "native_map_provider_nested_exits_tests.rs"]
mod nested_exits_tests;

#[path = "native_map_provider_temporary_exits_tests.rs"]
mod temporary_exits_tests;

#[path = "native_map_provider_local_sources_tests.rs"]
mod local_sources_tests;

#[path = "native_map_provider_borrow_exits_tests.rs"]
mod borrow_exits_tests;

#[path = "native_map_provider_resource_tests.rs"]
mod resource_tests;

#[path = "native_map_provider_history_tests.rs"]
mod history_tests;

#[path = "native_map_provider_sequential_tests.rs"]
mod sequential_tests;

#[path = "native_map_provider_resource_continuation_tests.rs"]
mod resource_continuation_tests;

#[path = "native_map_provider_source_forms_tests.rs"]
mod source_forms;

#[path = "native_map_provider_resource_forms_tests.rs"]
mod resource_forms;

#[path = "native_map_provider_future_use_tests.rs"]
mod future_use;

#[path = "native_map_provider_local_exits_tests.rs"]
mod local_exits;

#[path = "native_map_provider_local_resources_tests.rs"]
mod local_resources;

#[path = "native_map_provider_local_future_tests.rs"]
mod local_future;

#[path = "native_map_provider_local_reassignment_tests.rs"]
mod local_reassignment;

#[path = "native_map_provider_local_resource_future_tests.rs"]
mod local_resource_future;

#[path = "native_map_provider_temporary_forms_tests.rs"]
mod temporary_forms;

#[path = "native_map_provider_temporary_arguments_tests.rs"]
mod temporary_arguments;

#[path = "native_map_provider_temporary_delivery_tests.rs"]
mod temporary_delivery;

#[path = "native_map_provider_temporary_argument_cfg_tests.rs"]
mod temporary_argument_cfg;

#[path = "native_map_provider_control_flow_tests.rs"]
mod ordinary_control_flow;

#[path = "native_map_provider_stack_tests.rs"]
mod stack_paths;

#[path = "native_map_provider_formation_tests.rs"]
mod formation_paths;

#[path = "native_map_provider_local_arguments_tests.rs"]
mod local_arguments;

#[path = "native_map_provider_births_tests.rs"]
mod births;

#[path = "native_map_provider_scope_lowering_tests.rs"]
mod scope_lowering;

#[path = "native_map_provider_scope_lambda_body_tests.rs"]
mod scope_lambda_body;

#[path = "native_map_provider_scope_value_births_tests.rs"]
mod scope_value_births;

#[path = "native_map_provider_scope_member_parameter_tests.rs"]
mod scope_member_parameter;

#[path = "native_map_provider_scope_future_tests.rs"]
mod scope_future;
