//! 两份完整原 enum 源码的普通 Unit 成功合同；全采后才统一断言。
//! 首证只运行 pipeline exact。native exact 须在双源 SSA/LLVM 真绿后独立准入。
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod discriminant_support;
mod queries {
    //! 只读真实 frontend descriptor/Source origin/SSA def-use；不新建图或拼造 IR。
    use lang_frontend::{
        name_resolution::ValidatedCompilationUnitNames,
        parser::{Expression, Item, ParsedFile, WhenCondition},
        source::{SourceMap, Span},
        type_checking::{
            CompilationUnitTypes, UnitCallTarget, UnitConstructionTarget, UnitExpressionId,
            UnitTypeKind,
        },
    };

    use super::{Case, require};
    use crate::ssa::model::{
        ComparisonOperator, Definition, EntityId, EntityType, Function, FunctionId, Instruction,
        Operation, Origin, Program, ScalarConstant, SsaTypeKind, TerminatorKind, ValueId,
    };

    fn contains(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() <= inner.start()
            && inner.end() <= outer.end()
    }

    fn definition(function: &Function, value: ValueId) -> Result<&Instruction, String> {
        let entity = function
            .entity(EntityId::Value(value))
            .ok_or_else(|| format!("missing real value definition: {value:?}"))?;
        let Definition::InstructionResult { instruction, .. } = entity.definition else {
            return Err(format!("expected real instruction result: {value:?}"));
        };
        function
            .instruction(instruction)
            .ok_or_else(|| format!("missing defining instruction: {instruction:?}"))
    }

    pub(super) fn check(
        case: Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        program: &Program,
        driver: FunctionId,
    ) -> Result<(), String> {
        let driver_function = program
            .module(driver.module())
            .and_then(|module| module.function(driver))
            .ok_or_else(|| "missing actual lowered driver entry".to_owned())?;
        let [source_unit] = names.names().index().source_units() else {
            return Err("query needs the same unique original source unit".to_owned());
        };
        let source_unit = source_unit.id();
        let entries = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.source_unit() == source_unit
                    && declaration.name() == "entry"
                    && (
                        declaration.name_span().start(),
                        declaration.name_span().end(),
                    ) == case.entry_span
            })
            .collect::<Vec<_>>();
        let [entry] = entries.as_slice() else {
            return Err("query lost the unique original driver declaration".to_owned());
        };
        let entry_item = parsed
            .ast()
            .items()
            .get(entry.root())
            .map_err(|error| format!("original driver root Item: {error:?}"))?;
        require(
            matches!(entry_item.payload(), Item::Function { .. })
                && driver_function.origin == Origin::Source(entry_item.span()),
            "returned driver is the exact original entry Function",
            entry,
        )?;
        let driver_span = entry_item.span();
        let mut all_conditions = Vec::new();
        for (_, node) in parsed.ast().expressions().iter() {
            let Expression::When { entries, .. } = node.payload() else {
                continue;
            };
            for entry in entries {
                for condition in &entry.conditions {
                    let (span, expression) = match condition {
                        WhenCondition::Expression(expression) => {
                            let node = parsed
                                .ast()
                                .expressions()
                                .get(*expression)
                                .map_err(|error| format!("original condition AST: {error:?}"))?;
                            (
                                node.span(),
                                Some(UnitExpressionId::new(source_unit, *expression)),
                            )
                        }
                        WhenCondition::TypeTest {
                            operator_span,
                            type_ref,
                            ..
                        } => {
                            let target =
                                parsed.ast().type_refs().get(*type_ref).map_err(|error| {
                                    format!("original type condition AST: {error:?}")
                                })?;
                            (
                                sources
                                    .span(
                                        parsed.source_id(),
                                        operator_span.start(),
                                        target.span().end(),
                                    )
                                    .map_err(|error| {
                                        format!("original type condition span: {error:?}")
                                    })?,
                                None,
                            )
                        }
                        _ => return Err("unexpected condition in frozen enum sources".to_owned()),
                    };
                    all_conditions.push((entry.span, span, expression));
                }
            }
        }
        require(
            all_conditions.len() == case.conditions
                && all_conditions.iter().filter(|row| row.2.is_some()).count()
                    == case.expression_conditions,
            "all original when conditions enumerated",
            &all_conditions,
        )?;

        // All original condition spans are inspected, including the Guide's two `is` arms.
        // The full original driver remains present and performs its ordinary constructions.
        for &(entry_span, condition_span, expression) in &all_conditions {
            let mut compares = Vec::new();
            for module in &program.modules {
                for function in &module.functions {
                    for instruction in &function.instructions {
                        let origin = instruction.origin.span();
                        if matches!(instruction.operation, Operation::TaggedConstruct { .. })
                            && (contains(condition_span, origin) || origin == entry_span)
                        {
                            return Err(format!(
                                "condition constructed an enum value: {:?}; {instruction:?}",
                                sources.slice(condition_span)
                            ));
                        }
                        if matches!(instruction.operation, Operation::Compare { .. })
                            && matches!(instruction.origin, Origin::Source(span)
                                if span == condition_span || span == entry_span)
                        {
                            compares.push((module, function, instruction));
                        }
                    }
                }
            }
            let [(module, function, compare)] = compares.as_slice() else {
                return Err(format!(
                    "one source-anchored tag compare per original condition: {:?}; count={}",
                    sources.slice(condition_span),
                    compares.len()
                ));
            };
            let Operation::Compare {
                operator: ComparisonOperator::Equal,
                left,
                right,
            } = &compare.operation
            else {
                return Err(format!("expected tag equality: {compare:?}"));
            };
            let [EntityId::Value(result)] = compare.results.as_slice() else {
                return Err(format!("one Boolean comparison result: {compare:?}"));
            };
            let result_type = function
                .entity(EntityId::Value(*result))
                .map(|entity| entity.ty);
            require(
                matches!(result_type, Some(EntityType::Value(ty))
                if module.types.get(ty.index()) == Some(&SsaTypeKind::Boolean)),
                "tag comparison Boolean result",
                result_type,
            )?;
            let terminator = function
                .block(compare.block)
                .and_then(|block| block.terminator.as_ref())
                .ok_or_else(|| format!("missing actual when terminator: {:?}", compare.block))?;
            require(
                terminator.origin == Origin::Source(entry_span)
                    && matches!(terminator.kind, TerminatorKind::Conditional { condition, .. } if condition == *result),
                "actual when block consumes this exact Boolean tag comparison",
                terminator,
            )?;

            // The queried function must be the actual callee of an original driver Call.
            // Planner templates take the complete root Item span (unit_plan.rs), not its name alone.
            let mut actual_calls = 0;
            for instruction in &driver_function.instructions {
                let Operation::DirectCall { callee, .. } = &instruction.operation else {
                    continue;
                };
                if *callee != function.id {
                    continue;
                }
                let Origin::Source(call_span) = instruction.origin else {
                    return Err("original driver Call has no source identity".to_owned());
                };
                require(
                    contains(driver_span, call_span),
                    "Call belongs to the original driver body",
                    call_span,
                )?;
                let descriptors = typed
                    .calls()
                    .iter()
                    .filter(|call| {
                        call.expression().source_unit() == source_unit
                            && parsed
                                .ast()
                                .expressions()
                                .get(call.expression().expression())
                                .is_ok_and(|node| {
                                    node.span() == call_span
                                        && matches!(node.payload(), Expression::Call { .. })
                                })
                    })
                    .collect::<Vec<_>>();
                let [call] = descriptors.as_slice() else {
                    return Err(format!(
                        "unique original driver Call descriptor at {call_span:?}"
                    ));
                };
                let UnitCallTarget::Declaration(target) = call.target() else {
                    return Err(format!(
                        "original enum helper Call is not declaration-directed: {call:?}"
                    ));
                };
                let declarations = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|declaration| declaration.id() == target)
                    .collect::<Vec<_>>();
                let [declaration] = declarations.as_slice() else {
                    return Err(format!("missing original helper declaration: {target:?}"));
                };
                require(
                    declaration.source_unit() == source_unit,
                    "callee original source unit",
                    declaration,
                )?;
                let item = parsed
                    .ast()
                    .items()
                    .get(declaration.root())
                    .map_err(|error| format!("callee original root Item: {error:?}"))?;
                require(
                    parsed.roots().contains(&declaration.root())
                        && matches!(item.payload(), Item::Function { .. })
                        && function.origin == Origin::Source(item.span())
                        && contains(item.span(), entry_span),
                    "driver Call target is the exact original when-containing Function",
                    declaration,
                )?;
                actual_calls += 1;
            }
            require(
                actual_calls != 0,
                "original driver reaches the queried enum helper",
                function.id,
            )?;
            let Operation::TaggedDiscriminant { owner } = &definition(function, *left)?.operation
            else {
                return Err(format!(
                    "comparison must read the actual subject tag: {compare:?}"
                ));
            };
            let Some(expression) = expression else {
                continue;
            };

            // The expected variant originates in the same analysis's actual descriptor and
            // nominal declaration, not an SSA integer/type/arena-order guess.
            let descriptor = typed.construction(expression).ok_or_else(|| {
                format!("missing original condition construction: {expression:?}")
            })?;
            let UnitConstructionTarget::EnumCase(symbol) = descriptor.target() else {
                return Err(format!("condition is not an enum case: {descriptor:?}"));
            };
            let Some(UnitTypeKind::Nominal { declaration, .. }) =
                typed.types().get(descriptor.result_type())
            else {
                return Err(format!("condition has no nominal root: {descriptor:?}"));
            };
            let nominal = typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .ok_or_else(|| format!("missing actual enum declaration: {declaration:?}"))?;
            let selected = nominal
                .enum_cases()
                .iter()
                .enumerate()
                .filter(|(_, case)| case.value_symbol() == symbol)
                .collect::<Vec<_>>();
            let [(variant, selected)] = selected.as_slice() else {
                return Err(format!(
                    "unique actual declaration/case identity: {symbol:?}"
                ));
            };
            require(
                descriptor.expression() == expression
                    && descriptor.arguments().is_empty()
                    && selected.payloads().is_empty(),
                "original payloadless case descriptor",
                descriptor,
            )?;
            require(
                matches!(&definition(function, *right)?.operation,
                Operation::Constant(ScalarConstant::Integer(actual)) if *actual == *variant as i128),
                "tag constant equals descriptor-selected declaration order",
                (*declaration, symbol, variant),
            )?;

            // Independently anchor the tagged root to the same case's original driver
            // construction. Both enums reuse On/Off spelling with reversed declaration order.
            let mut births = Vec::new();
            for birth in typed.constructions().iter().filter(|birth| {
                birth.expression().source_unit() == source_unit
                    && birth.expression() != expression
                    && birth.target() == descriptor.target()
                    && birth.result_type() == descriptor.result_type()
            }) {
                let span = parsed
                    .ast()
                    .expressions()
                    .get(birth.expression().expression())
                    .map_err(|error| format!("original driver construction AST: {error:?}"))?
                    .span();
                for instruction in &driver_function.instructions {
                    if instruction.origin == Origin::Source(span)
                        && let Operation::TaggedConstruct {
                            tagged, variant, ..
                        } = &instruction.operation
                    {
                        births.push((*tagged, *variant, birth.expression()));
                    }
                }
            }
            let [(tagged, actual_variant, birth)] = births.as_slice() else {
                return Err(format!(
                    "one real same-case construction in original driver: {births:?}"
                ));
            };
            require(
                actual_variant == variant
                    && function
                        .entity(EntityId::Value(*owner))
                        .map(|entity| entity.ty)
                        == Some(EntityType::Value(*tagged)),
                "subject tagged root equals real descriptor-matched driver construction",
                (*declaration, symbol, variant, birth, tagged),
            )?;
        }
        Ok(())
    }
}

use std::{
    fmt::Debug,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
    parser::{Item, NameMarker, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use crate::emit_native_owned_unit_object;

struct NativeDirectory {
    path: PathBuf,
    cleaned: bool,
}

impl NativeDirectory {
    fn create() -> Result<Self, String> {
        static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-unit-enum-native-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).map_err(|error| format!("native directory create: {error}"))?;
        Ok(Self {
            path,
            cleaned: false,
        })
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let result = fs::remove_dir_all(&self.path)
            .map_err(|error| format!("native directory cleanup: {error}"));
        self.cleaned = result.is_ok();
        result
    }
}

impl Drop for NativeDirectory {
    fn drop(&mut self) {
        if !self.cleaned {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[derive(Clone, Copy)]
struct Case {
    id: &'static str,
    path: &'static str,
    package: &'static [&'static str],
    entry_span: (usize, usize),
    text: &'static str,
    stdout: &'static [u8],
    conditions: usize,
    expression_conditions: usize,
}

fn cases() -> [Case; 2] {
    [
        Case {
            id: "enum_payloadless",
            path: "enum-tag-identity.ko",
            package: &[],
            entry_span: (246, 251),
            text: include_str!(
                "../../tests/fixtures/unit-enum-regression-0321/enum_payloadless.ko"
            ),
            stdout: include_bytes!(
                "../../tests/fixtures/unit-enum-regression-0321/enum_payloadless.stdout"
            ),
            conditions: 4,
            expression_conditions: 4,
        },
        Case {
            id: "enum_payload_guide",
            path: "demo/enums/litmus4.ko",
            package: &["demo", "enums"],
            entry_span: (302, 307),
            text: include_str!(
                "../../tests/fixtures/unit-enum-regression-0321/enum_payload_guide.ko"
            ),
            stdout: include_bytes!(
                "../../tests/fixtures/unit-enum-regression-0321/enum_payload_guide.stdout"
            ),
            conditions: 3,
            expression_conditions: 1,
        },
    ]
}

fn require(ok: bool, stage: &str, detail: impl Debug) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("{stage}: {detail:?}"))
    }
}

fn at(sources: &SourceMap, stage: &str, error: impl Debug, span: Option<Span>) -> String {
    let source = span.map(|span| {
        (
            span,
            sources.source_name(span.source_id()),
            sources.slice(span),
        )
    });
    format!("{stage}: {error:?}; original_source={source:?}")
}

fn clean(sources: &SourceMap, stage: &str, diagnostics: &[Diagnostic]) -> Result<(), String> {
    require(
        diagnostics.is_empty(),
        stage,
        diagnostics
            .iter()
            .map(|diagnostic| at(sources, stage, diagnostic, Some(diagnostic.primary_span())))
            .collect::<Vec<_>>(),
    )
}

/// 单一原 source；不补 package/wrapper、不重选 entry、不制造或替换任何事实。
/// 现有 Map helper 锁定默认包/provider.ko，旧 unit analyze 又没有 complete 准入；
/// 此处仅按已有公开 API 顺序做 Result 适配，保证首份失败不遮住第二份。
fn observe(case: Case, native: bool, stages: &mut Vec<&'static str>) -> Result<(), String> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(case.path, case.text)
        .map_err(|error| format!("source: {error:?}"))?;
    let lexed = lex(&sources, source).map_err(|error| format!("lex: {error:?}"))?;
    clean(&sources, "lex diagnostics", lexed.diagnostics())?;
    stages.push("lex:ok");
    let parsed = parse_file(&sources, &lexed).map_err(|error| format!("parse: {error:?}"))?;
    clean(&sources, "parse diagnostics", parsed.diagnostics())?;
    require(
        parsed.source_id() == source && sources.source_text(source) == Ok(case.text),
        "original source bytes",
        case.path,
    )?;
    stages.push("parse:ok");
    let inputs = [SourceUnitInput::new("root", case.path, source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs)
        .map_err(|error| format!("unit index: {error:?}"))?;
    clean(&sources, "index diagnostics", index.diagnostics())?;
    stages.push("index:ok");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .map_err(|error| format!("unit names: {error:?}"))?;
    clean(&sources, "names diagnostics", names.diagnostics())?;
    let names = names
        .validate()
        .map_err(|error| format!("names validate: {error:?}"))?;
    stages.push("names validate:ok");
    let [source_unit] = names.names().index().source_units() else {
        return Err("expected exactly one original source unit".to_owned());
    };
    require(
        source_unit.source_id() == source
            && source_unit.key().root().expect("rooted source").as_str() == "root"
            && source_unit
                .key()
                .logical_path()
                .expect("rooted source")
                .as_str()
                == case.path,
        "source-unit identity",
        source_unit,
    )?;
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "entry" && declaration.kind() == SymbolKind::Function
            && declaration.source_unit() == source_unit.id()
            && declaration.package() == source_unit.package()
            && declaration.name_span().source_id() == source
            && (declaration.name_span().start(), declaration.name_span().end()) == case.entry_span
            && names.names().index().packages().get(declaration.package().index())
                .is_some_and(|package| package.name().expect("named package").segments().iter().map(String::as_str)
                    .eq(case.package.iter().copied()))
            && parsed.roots().contains(&declaration.root())
            && parsed.ast().items().get(declaration.root()).is_ok_and(|node| {
                matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                    if *name == declaration.name_span())
            })
        })
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        return Err(format!("original driver entry selection: {entries:?}"));
    };
    stages.push("original driver entry:ok");
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .map_err(|error| format!("unit raw types: {error:?}"))?;
    clean(&sources, "raw type diagnostics", raw.diagnostics())?;
    {
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw)
                .map_err(|error| format!("complete type admission: {error:?}"))?;
        require(
            complete.is_same_analysis(&raw)
                && std::ptr::eq(complete.types(), &raw)
                && raw.is_compatible_with(&sources, &inputs, &names, &environment),
            "complete original analysis identity",
            case.id,
        )?;
    }
    stages.push("complete type admission:ok");
    let typed = raw
        .validate()
        .map_err(|error| format!("basic type validate: {error:?}"))?;
    stages.push("basic type validate:ok");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .map_err(|error| format!("unit ownership: {error:?}"))?;
    clean(&sources, "ownership diagnostics", owned.diagnostics())?;
    require(
        owned.deferred().is_empty(),
        "ownership deferred",
        owned.deferred(),
    )?;
    require(
        owned.is_compatible_with(&typed),
        "ownership original analysis identity",
        case.id,
    )?;
    let owned = owned
        .validate()
        .map_err(|error| format!("ownership validate: {error:?}"))?;
    stages.push("ownership validate:ok");
    let view = owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
        .map_err(|error| format!("sealed owned view: {error:?}"))?;
    require(
        std::ptr::eq(view.sources(), &sources)
            && std::ptr::eq(view.inputs(), inputs.as_slice())
            && std::ptr::eq(view.names(), &names)
            && std::ptr::eq(view.types(), typed.types())
            && std::ptr::eq(view.ownership(), owned.ownership()),
        "sealed view retains original references",
        case.id,
    )?;
    stages.push("sealed owned view:ok");
    if native {
        // Same ordinary object/link/run flow as native_tests::emit_link_and_run;
        // this entry uses the original ordinary owned-unit view, never a const adapter.
        let mut directory = NativeDirectory::create()?;
        let result: Result<(), String> = (|| {
            let object = directory.join("program.o");
            let executable = directory.join("program");
            emit_native_owned_unit_object(&view, entry.id(), &object)
                .map_err(|error| at(&sources, "public native object", &error, error.span()))?;
            stages.push("public native object:ok");
            let linked = Command::new(crate::test_support::clang())
                .arg(&object)
                .arg("-o")
                .arg(&executable)
                .output()
                .map_err(|error| format!("link launch: {error:?}"))?;
            require(linked.status.success(), "link status", &linked)?;
            stages.push("link:ok");
            let run = Command::new(&executable)
                .output()
                .map_err(|error| format!("run launch: {error:?}"))?;
            require(
                run.status.code() == Some(0) && run.stdout == case.stdout && run.stderr.is_empty(),
                "exit=0, exact original stdout, empty stderr",
                run,
            )?;
            stages.push("native golden:ok");
            Ok(())
        })();
        let cleanup = directory.cleanup();
        match (result, cleanup) {
            (Ok(()), Ok(())) => stages.push("native cleanup:ok"),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => return Err(error),
            (Err(error), Err(cleanup)) => return Err(format!("{error}; {cleanup}")),
        }
    } else {
        stages.push("ordinary unit lower:entered");
        let (program, driver) =
            super::unit_lower::lower_owned_unit_with_entry(&view, entry.id())
                .map_err(|error| at(&sources, "ordinary unit lower", error, error.span))?;
        stages.push("ordinary unit lower:ok");
        super::verify::verify_program(&program)
            .map_err(|error| format!("SSA verifier: {error:?}"))?;
        stages.push("SSA verifier:ok");
        crate::llvm::render_verified_program_with_entry(&program, driver)
            .map_err(|error| format!("LLVM verifier: {error:?}"))?;
        stages.push("LLVM verifier:ok");
        queries::check(
            case,
            &sources,
            &parsed,
            &names,
            typed.types(),
            &program,
            driver,
        )?;
        stages.push("source-anchored enum identity:ok");
    }
    Ok(())
}

fn collect_all(native: bool) {
    let observations = cases()
        .into_iter()
        .map(|case| {
            let mut stages = Vec::new();
            let result = observe(case, native, &mut stages);
            (case.id, case.path, case.entry_span, stages, result)
        })
        .collect::<Vec<_>>();
    // Neither stop point becomes an oracle: both original sources must succeed.
    for observation in &observations {
        diagnostic!("ENUM ORIGINAL UNIT native={native} {observation:#?}");
    }
    assert_eq!(observations.len(), 2);
    assert_diagnostic!(
        observations.iter().all(|row| row.4.is_ok()),
        "both complete original enum sources must succeed: {observations:#?}"
    );
}

#[test]
fn original_enum_sources_unit_pipeline_exact() {
    collect_all(false);
}

#[test]
fn original_enum_sources_unit_native_exact() {
    collect_all(true);
}
