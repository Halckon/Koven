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

mod exits_tests {
    //! SPEC-0331：固定六源无条件 Map 退出的普通 single/basic native 成功合同。
    //! 同源十二条 frontend→SSA verifier→LLVM pipeline 真绿后才能另行准入执行。
    //! 原 helper 没有内置硬超时；本模块只复用普通 object/link/run，不扩展执行框架。

    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::sources as exit_sources,
    };

    #[test]
    fn single_map_provider_exit_sources_exit_normally() {
        for (case, source, expected) in exit_sources() {
            let run = emit_link_and_run("provider.ko", source, "entry");
            assert_result(case, "single", &run, expected);
        }
    }

    #[test]
    fn unit_map_provider_exit_sources_exit_normally() {
        for (case, source, expected) in exit_sources() {
            let run = run_unit(case, source);
            assert_result(case, "unit", &run, expected);
        }
    }
}

mod conditional_tests {
    //! SPEC-0331：固定两源条件 Map 退出的普通 single/basic native 成功合同。
    //! 同源四条 frontend→SSA verifier→LLVM pipeline 真绿后才能另行准入执行。
    //! 原 helper 没有内置硬超时；本模块只复用普通 object/link/run，不扩展执行框架。

    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::conditional_sources,
    };

    #[test]
    fn single_map_provider_conditional_sources_exit_normally() {
        for (case, _map, _source_hash, source, expected) in conditional_sources() {
            let run = emit_link_and_run("provider.ko", source, "entry");
            assert_result(case, "single", &run, expected);
        }
    }

    #[test]
    fn unit_map_provider_conditional_sources_exit_normally() {
        for (case, _map, _source_hash, source, expected) in conditional_sources() {
            let run = run_unit(case, source);
            assert_result(case, "unit", &run, expected);
        }
    }
}

mod temporary_tests {
    //! SPEC-0331：固定两源 temporary Map 正常耗尽的普通 single/basic native 成功合同。
    //! 同源四条 frontend→SSA verifier→LLVM pipeline 真绿后才能另行准入执行。
    //! 原 helper 没有内置硬超时；本模块只复用普通 object/link/run，不扩展执行框架。

    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::temporary_sources,
    };

    #[test]
    fn single_map_provider_temporary_sources_exit_normally() {
        for (case, _map, _source_hash, source, expected) in temporary_sources() {
            let run = emit_link_and_run("provider.ko", source, "entry");
            assert_result(case, "single", &run, expected);
        }
    }

    #[test]
    fn unit_map_provider_temporary_sources_exit_normally() {
        for (case, _map, _source_hash, source, expected) in temporary_sources() {
            let run = run_unit(case, source);
            assert_result(case, "unit", &run, expected);
        }
    }
}

#[path = "native_map_provider_d10_w09_tests.rs"]
pub(crate) mod d10_w09;

mod nested_tests {
    //! SPEC-0331：固定两源同根双层 Borrow Map 正常耗尽的普通 single/basic native 成功合同。
    //! 同源四条 frontend→SSA verifier→LLVM pipeline 真绿后才能另行准入执行。
    //! 原 helper 没有内置硬超时；本模块只复用普通 object/link/run，不扩展执行框架。

    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::nested_sources,
    };

    #[test]
    fn single_map_provider_nested_sources_exit_normally() {
        for (case, _map, _source_hash, source, expected) in nested_sources() {
            let run = emit_link_and_run("provider.ko", source, "entry");
            assert_result(case, "single", &run, expected);
        }
    }

    #[test]
    fn unit_map_provider_nested_sources_exit_normally() {
        for (case, _map, _source_hash, source, expected) in nested_sources() {
            let run = run_unit(case, source);
            assert_result(case, "unit", &run, expected);
        }
    }
}

mod nested_exits_tests {
    //! SPEC-0331：固定六源双层 inner continue/break/return 的普通 single/basic native 合同。
    //! 同源十二条 frontend→SSA verifier→LLVM pipeline 真绿后才能另行准入执行。
    //! 原 helper 没有内置硬超时；本模块只复用普通 object/link/run，不扩展执行框架。

    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::nested_exit_sources,
    };

    #[test]
    fn single_map_provider_nested_exit_sources_exit_normally() {
        for (case, source, expected) in nested_exit_sources() {
            let run = emit_link_and_run("provider.ko", source, "entry");
            assert_result(case, "single", &run, expected);
        }
    }

    #[test]
    fn unit_map_provider_nested_exit_sources_exit_normally() {
        for (case, source, expected) in nested_exit_sources() {
            let run = run_unit(case, source);
            assert_result(case, "unit", &run, expected);
        }
    }
}

mod temporary_exits_tests {
    //! SPEC-0331：固定六源 temporary Call source continue/break/return 的普通 single/basic native 合同。
    //! 同源十二条 frontend→SSA verifier→LLVM pipeline 真绿后才能另行准入执行。
    //! 原 helper 没有内置硬超时；本模块只复用普通 object/link/run，不扩展执行框架。

    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::temporary_exit_sources,
    };

    #[test]
    fn single_map_provider_temporary_exit_sources_exit_normally() {
        for (case, source, expected) in temporary_exit_sources() {
            let run = emit_link_and_run("provider.ko", source, "entry");
            assert_result(case, "single", &run, expected);
        }
    }

    #[test]
    fn unit_map_provider_temporary_exit_sources_exit_normally() {
        for (case, source, expected) in temporary_exit_sources() {
            let run = run_unit(case, source);
            assert_result(case, "unit", &run, expected);
        }
    }
}

mod local_sources_tests {
    //! SPEC-0331：固定两源 local named A/B 的普通 single/basic native 合同。
    //! 同源四条 frontend→SSA verifier→LLVM pipeline 真绿后才能另行准入执行。
    //! 原 helper 没有内置硬超时；本模块只复用普通 object/link/run，不扩展执行框架。

    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::local_native_sources,
    };

    #[test]
    fn single_map_provider_local_sources_exit_normally() {
        for (case, source, expected) in local_native_sources() {
            let run = emit_link_and_run("provider.ko", source, "entry");
            assert_result(case, "single", &run, expected);
        }
    }

    #[test]
    fn unit_map_provider_local_sources_exit_normally() {
        for (case, source, expected) in local_native_sources() {
            let run = run_unit(case, source);
            assert_result(case, "unit", &run, expected);
        }
    }
}

mod borrow_exits_tests {
    //! Four newly rebuilt Borrow Map sources, accepted only after their eight real pipelines pass.
    //! Ordinary emit/link/run only; no injected failure, instrumentation or sanitizer controls.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::borrowed_exit_sources,
    };

    #[test]
    fn single_borrowed_map_exit_sources_exit_normally() {
        for (case, source, golden) in borrowed_exit_sources() {
            let run = emit_link_and_run("provider.ko", source, "entry");
            assert_result(case, "single", &run, golden);
        }
    }

    #[test]
    fn unit_borrowed_map_exit_sources_exit_normally() {
        for (case, source, golden) in borrowed_exit_sources() {
            let run = run_unit(case, source);
            assert_result(case, "unit", &run, golden);
        }
    }
}

mod resource_tests {
    //! Finite Resource Map source ownership; gated first by both real SSA/LLVM pipelines.
    //! Ordinary outputs prove ordering, not dynamic release counts or absence of leaks.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::resource_sources,
    };

    #[test]
    fn single_resource_map_sources_exit_normally() {
        for (case, source, golden) in resource_sources() {
            let run = emit_link_and_run("provider.ko", source, "entry");
            assert_result(case, "single", &run, golden);
        }
    }

    #[test]
    fn unit_resource_map_sources_exit_normally() {
        for (case, source, golden) in resource_sources() {
            let run = run_unit(case, source);
            assert_result(case, "unit", &run, golden);
        }
    }
}

mod history_tests {
    //! Ordinary unmodified-source histories, paired with independently specified stdout.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::history_sources,
    };

    #[test]
    fn single_storage_histories_exit_normally_in_insertion_order() {
        for case in history_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }

    #[test]
    fn unit_storage_histories_exit_normally_in_insertion_order() {
        for case in history_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }
}

mod sequential_tests {
    //! Original ordinary sequential Borrow sources; no instrumentation or replacement products.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::sequential_sources,
    };

    #[test]
    fn single_sequential_borrowed_maps_exit_with_exact_cleanup_and_order() {
        for case in sequential_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }

    #[test]
    fn unit_sequential_borrowed_maps_exit_with_exact_cleanup_and_order() {
        for case in sequential_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }
}

#[allow(unused_imports)]
pub(crate) use d10_w09::resource_continuation_tests;

mod source_forms {
    //! Ordinary source-form native checks, after both real SSA/LLVM entries succeed.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::source_form_sources,
    };
    #[test]
    fn single_owned_mutable_and_group_sources_exit_normally() {
        for case in source_form_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }
    #[test]
    fn unit_owned_mutable_and_group_sources_exit_normally() {
        for case in source_form_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }
}

mod resource_forms {
    //! Ordinary native checks for represented Resource payloads, after the SSA/LLVM gate.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::resource_form_sources,
    };
    #[test]
    fn single_existing_resource_representations_exit_normally() {
        for case in resource_form_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }
    #[test]
    fn unit_existing_resource_representations_exit_normally() {
        for case in resource_form_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }
}

mod future_use {
    //! Ordinary Pure future-use native checks after the real two-entry pipeline succeeds.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::future_use_sources,
    };
    #[test]
    fn single_pure_owned_future_uses_exit_normally() {
        for case in future_use_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }
    #[test]
    fn unit_pure_owned_future_uses_exit_normally() {
        for case in future_use_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }
}

mod local_exits {
    //! Ordinary native checks for real local formation and moved-alias exits.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::local_exits_sources,
    };

    #[test]
    fn single_local_formation_exits_normally() {
        for case in local_exits_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }

    #[test]
    fn unit_local_formation_exits_normally() {
        for case in local_exits_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }

    #[test]
    fn unit_rebound_original_root_keeps_both_owners_separate() {
        let (label, source, golden) =
            crate::ssa::map_provider_exits_pipeline_tests::local_alias_rebound_source();
        let run = run_unit(label, &source);
        assert_result(label, "unit", &run, golden);
    }
}

mod local_resources {
    //! Strict ordinary native acceptance for factory-formed Resource Map local exits.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::local_resource_sources,
    };

    #[test]
    fn single_resource_local_formation_exits_normally() {
        for case in local_resource_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }

    #[test]
    fn unit_resource_local_formation_exits_normally() {
        for case in local_resource_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }
}

mod local_future {
    //! Normal local-formation/future-use products; no fault calibration or sanitizers.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::local_future_sources,
    };

    #[test]
    fn single_local_sources_follow_future_uses() {
        for case in local_future_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }

    #[test]
    fn unit_local_sources_follow_future_uses() {
        for case in local_future_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }
}

#[allow(unused_imports)]
pub(crate) use d10_w09::local_reassignment;

mod local_resource_future {
    //! Strict native checks for Resource local future-use corners and real empty inputs.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::local_resource_future_sources,
    };
    #[test]
    fn single_resource_future_locals_exit_normally() {
        for case in local_resource_future_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }
    #[test]
    fn unit_resource_future_locals_exit_normally() {
        for case in local_resource_future_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }
}

mod temporary_forms {
    //! Real represented temporary source programs, after both SSA/LLVM entries pass.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::temporary_form_sources,
    };

    #[test]
    fn single_represented_temporary_sources_exit_normally() {
        for case in temporary_form_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }

    #[test]
    fn unit_represented_temporary_sources_exit_normally() {
        for case in temporary_form_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }
}

#[allow(unused_imports)]
pub(crate) use d10_w09::temporary_arguments;

#[allow(unused_imports)]
pub(crate) use d10_w09::temporary_delivery;

mod temporary_argument_cfg {
    //! Both runtime argument CFG arms execute the original empty temporary-source cases.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::temporary_argument_cfg_sources,
    };

    fn run(unit: bool) {
        for case in temporary_argument_cfg_sources() {
            let output = if unit {
                run_unit(&case.label, &case.source)
            } else {
                emit_link_and_run("provider.ko", &case.source, "entry")
            };
            assert_result(
                &case.label,
                if unit { "unit" } else { "single" },
                &output,
                &case.golden,
            );
        }
    }

    #[test]
    fn single_argument_cfg_edges_exit_normally() {
        run(false);
    }

    #[test]
    fn unit_argument_cfg_edges_exit_normally() {
        run(true);
    }
}

#[allow(unused_imports)]
pub(crate) use d10_w09::ordinary_control_flow;

#[allow(unused_imports)]
pub(crate) use d10_w09::stack_paths;

#[allow(unused_imports)]
pub(crate) use d10_w09::formation_paths;

#[allow(unused_imports)]
pub(crate) use d10_w09::local_arguments;

mod births {
    //! Exact fixture bytes and full golden output for intrinsic/Unit FunctionValue births.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::{MapBirthKind, map_birth_sources},
    };
    fn run(unit: bool) {
        for c in map_birth_sources() {
            if !unit && c.kind == MapBirthKind::FunctionValue {
                continue;
            }
            let output = if unit {
                run_unit(c.label, c.source)
            } else {
                emit_link_and_run("provider.ko", c.source, "entry")
            };
            assert_result(
                c.label,
                if unit { "unit" } else { "single" },
                &output,
                c.golden,
            );
        }
    }
    #[test]
    fn single_intrinsic_births_keep_full_native_golden() {
        run(false);
    }
    #[test]
    fn unit_intrinsic_and_function_value_births_keep_full_native_golden() {
        run(true);
    }
}

mod scope_lowering {
    //! Every fixed A1 program executes its original entry once, including encoded true/false arms.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::scope_a1_sources,
    };
    fn run(unit: bool) {
        for c in scope_a1_sources() {
            let out = if unit {
                run_unit(c.label, c.source)
            } else {
                emit_link_and_run("provider.ko", c.source, "entry")
            };
            assert_result(
                c.label,
                if unit { "unit" } else { "single" },
                &out,
                c.golden,
            );
        }
    }
    #[test]
    fn single_scope_a1_frozen_programs_keep_full_golden() {
        run(false);
    }
    #[test]
    fn unit_scope_a1_frozen_programs_keep_full_golden() {
        run(true);
    }
}

mod scope_lambda_body {
    //! Original P15 entries execute once per pipeline with their complete frozen goldens.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::scope_a2_sources,
    };
    fn run(unit: bool) {
        for c in scope_a2_sources() {
            let output = if unit {
                run_unit(c.label, c.source)
            } else {
                emit_link_and_run("provider.ko", c.source, "entry")
            };
            assert_result(
                c.label,
                if unit { "unit" } else { "single" },
                &output,
                c.golden,
            );
        }
    }
    #[test]
    fn single_scope_a2_lambda_bodies_keep_full_golden() {
        run(false);
    }
    #[test]
    fn unit_scope_a2_lambda_bodies_keep_full_golden() {
        run(true);
    }
}

mod scope_value_births {
    //! Only the fixed Int storage group is registered in this bounded repair.
    mod int_storage {
        use super::super::{assert_result, run_unit};
        use crate::{
            native_tests::emit_link_and_run,
            ssa::map_provider_exits_pipeline_tests::scope_int_storage_sources,
        };
        fn run(unit: bool) {
            for c in scope_int_storage_sources() {
                let output = if unit {
                    run_unit(c.label, c.source)
                } else {
                    emit_link_and_run("provider.ko", c.source, "entry")
                };
                assert_result(
                    c.label,
                    if unit { "unit" } else { "single" },
                    &output,
                    c.golden,
                );
            }
        }
        #[test]
        fn single_original_int_storage_keeps_full_golden() {
            run(false);
        }
        #[test]
        fn unit_original_int_storage_keeps_full_golden() {
            run(true);
        }
    }

    mod borrow_alias {
        use super::super::{assert_result, run_unit};
        use crate::{
            native_tests::emit_link_and_run,
            ssa::map_provider_exits_pipeline_tests::scope_borrow_alias_sources,
        };
        fn run(unit: bool) {
            for c in scope_borrow_alias_sources() {
                let output = if unit {
                    run_unit(c.label, c.source)
                } else {
                    emit_link_and_run("provider.ko", c.source, "entry")
                };
                assert_result(
                    c.label,
                    if unit { "unit" } else { "single" },
                    &output,
                    c.golden,
                );
            }
        }
        #[test]
        fn single_original_borrow_alias_keeps_full_golden() {
            run(false);
        }
        #[test]
        fn unit_original_borrow_alias_keeps_full_golden() {
            run(true);
        }
    }

    mod conditional_owner {
        use super::super::{assert_result, run_unit};
        use crate::ssa::map_provider_exits_pipeline_tests::scope_if_owner_sources;
        fn run(prefix: &str) {
            let cases = scope_if_owner_sources()
                .into_iter()
                .filter(|c| c.label.starts_with(prefix))
                .collect::<Vec<_>>();
            assert_eq!(cases.len(), 2);
            for c in cases {
                let output = run_unit(c.label, c.source);
                assert_result(c.label, "unit", &output, c.golden);
            }
        }
        #[test]
        fn unit_original_named_if_owner_keeps_full_golden() {
            run("P03-");
        }
        #[test]
        fn unit_original_temporary_if_owner_keeps_full_golden() {
            run("P04-");
        }
    }

    mod owner_update {
        use super::super::{assert_result, run_unit};
        use crate::{
            native_tests::emit_link_and_run,
            ssa::map_provider_exits_pipeline_tests::scope_owner_update_sources,
        };
        fn run(unit: bool) {
            let mut count = 0;
            for c in scope_owner_update_sources() {
                if !unit && c.code <= 8 {
                    continue;
                }
                let output = if unit {
                    run_unit(c.label, c.source)
                } else {
                    emit_link_and_run("provider.ko", c.source, "entry")
                };
                assert_result(
                    c.label,
                    if unit { "unit" } else { "single" },
                    &output,
                    c.golden,
                );
                count += 1;
            }
            assert_eq!(count, if unit { 10 } else { 6 });
        }
        #[test]
        fn single_original_owner_updates_keep_full_golden() {
            run(false);
        }
        #[test]
        fn unit_original_owner_updates_keep_full_golden() {
            run(true);
        }
    }
}

mod scope_member_parameter {
    //! The two unchanged P24 Unit entries retain their complete original goldens.
    use super::{assert_result, run_unit};
    use crate::ssa::map_provider_exits_pipeline_tests::scope_member_parameter_sources;
    fn run(label: &str) {
        let c = scope_member_parameter_sources()
            .into_iter()
            .find(|c| c.label == label)
            .expect("one original P24 source");
        let output = run_unit(c.label, c.source);
        assert_result(c.label, "unit", &output, c.golden);
    }
    #[test]
    fn unit_original_member_parameter_for_keeps_full_golden() {
        run("P24-for");
    }
    #[test]
    fn unit_original_member_parameter_control_keeps_full_golden() {
        run("P24-control");
    }
}

mod scope_future {
    //! All six original D sources retain their frozen full stdout under both entries.
    use super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::scope_future_sources,
    };
    #[test]
    fn single_original_future_handoffs_keep_full_goldens() {
        let cases = scope_future_sources();
        assert_eq!(cases.len(), 6);
        for c in cases {
            let output = emit_link_and_run("provider.ko", c.source, "entry");
            assert_result(c.label, "single", &output, c.golden);
        }
    }
    #[test]
    fn unit_original_future_handoffs_keep_full_goldens() {
        let cases = scope_future_sources();
        assert_eq!(cases.len(), 6);
        for c in cases {
            let output = run_unit(c.label, c.source);
            assert_result(c.label, "unit", &output, c.golden);
        }
    }
}

#[allow(unused_imports)]
pub(crate) use d10_w09::scalar_routing_tests;
