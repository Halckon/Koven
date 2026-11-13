//! SPEC-0331 的普通 Map provider 成功合同；两种真实 Unit 输入的四格全收齐后断言。
//! 不执行 native，不以当前 lower 的 UnsupportedNode 债务作为通过条件。

use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
pub(in crate::ssa) mod facts;
mod fixture {
    //! 普通源码、真实 AST 和阶段错误；不制造或改写 frontend facts。
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        diagnostic::Diagnostic,
        lexer::lex,
        parser::{Expression, ForBinding, NameMarker, ParsedFile, Statement, parse_file},
        source::{SourceMap, Span},
        type_checking::{FinalTypeAdmissionError, FinalTypeAdmissionInternalError},
    };

    use crate::ssa::{LoweringError, model::Origin, verify::VerifyErrors};

    pub(super) const FOR_TEXT: &str = "for ((key, value) in source) {\n    read(key, value)\n  }";

    #[derive(Debug)]
    pub(in crate::ssa) struct Failure {
        pub stage: &'static str,
        pub original: String,
    }

    pub(super) fn failure(stage: &'static str, error: impl std::fmt::Debug) -> Failure {
        Failure {
            stage,
            original: format!("{error:?}"),
        }
    }

    pub(super) fn location(sources: &SourceMap, span: Span) -> String {
        format!(
            "span={span:?} path={:?} text={:?}",
            sources.source_name(span.source_id()),
            sources.slice(span)
        )
    }

    pub(super) fn at(
        sources: &SourceMap,
        stage: &'static str,
        error: impl std::fmt::Debug,
        span: Option<Span>,
    ) -> Failure {
        failure(stage, (error, span.map(|span| location(sources, span))))
    }

    fn diagnostics(sources: &SourceMap, diagnostics: &[Diagnostic]) -> String {
        format!(
            "{:#?}",
            diagnostics
                .iter()
                .map(|diagnostic| (diagnostic, location(sources, diagnostic.primary_span())))
                .collect::<Vec<_>>()
        )
    }

    pub(super) fn clean(
        sources: &SourceMap,
        stage: &'static str,
        items: &[Diagnostic],
    ) -> Result<(), Failure> {
        if items.is_empty() {
            Ok(())
        } else {
            Err(failure(stage, diagnostics(sources, items)))
        }
    }

    pub(super) fn complete_failure(sources: &SourceMap, error: FinalTypeAdmissionError) -> Failure {
        let locations = match &error {
            FinalTypeAdmissionError::Diagnostics(items) => diagnostics(sources, items),
            FinalTypeAdmissionError::Internal(
                FinalTypeAdmissionInternalError::MissingRequiredFact { span, .. },
            ) => location(sources, *span),
            _ => "no source span supplied by the original error".to_owned(),
        };
        failure("complete admission", (error, locations))
    }

    pub(super) fn lower_failure(sources: &SourceMap, error: LoweringError) -> Failure {
        at(sources, "lower", error, error.span)
    }

    pub(super) fn verify_failure(sources: &SourceMap, errors: VerifyErrors) -> Failure {
        let locations = errors
            .errors
            .iter()
            .filter_map(|error| match &error.origin {
                Some(Origin::Source(span)) => Some(location(sources, *span)),
                _ => None,
            })
            .collect::<Vec<_>>();
        failure("SSA verifier", (errors, locations))
    }

    pub(in crate::ssa) struct Fixture {
        pub sources: SourceMap,
        pub parsed: ParsedFile,
        pub path: &'static str,
        pub map: bool,
    }

    impl Fixture {
        pub fn new(label: &str, text: &str) -> Result<Self, Failure> {
            let mut sources = SourceMap::new();
            let path = "provider.ko";
            let source = sources
                .add_source(path, text)
                .map_err(|error| failure("source infrastructure", error))?;
            let lexed =
                lex(&sources, source).map_err(|error| failure("lex infrastructure", error))?;
            clean(&sources, "lex diagnostics", lexed.diagnostics())?;
            let parsed = parse_file(&sources, &lexed)
                .map_err(|error| failure("parse infrastructure", error))?;
            clean(&sources, "parse diagnostics", parsed.diagnostics())?;
            let map = match label {
                "map" => true,
                "mutable-map" => false,
                _ => return Err(failure("fixture identity", label)),
            };
            Ok(Self {
                sources,
                parsed,
                path,
                map,
            })
        }
    }

    #[derive(Debug)]
    pub(super) struct LoopSite {
        pub statement: StatementId,
        pub span: Span,
        pub source: ExpressionId,
        pub source_span: Span,
        pub bindings: [Span; 2],
    }

    pub(super) fn loop_site(fixture: &Fixture) -> Result<LoopSite, Failure> {
        let ast = fixture.parsed.ast();
        let sites = ast
            .statements()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Statement::For { .. }))
            .collect::<Vec<_>>();
        let [(statement, node)] = sites.as_slice() else {
            return Err(failure(
                "AST Map input query",
                ("expected one For", sites.len()),
            ));
        };
        let Statement::For {
            binding, source, ..
        } = node.payload()
        else {
            return Err(failure("AST Map input query", "not a real For"));
        };
        let ForBinding::Destructuring { names, .. } = binding else {
            return Err(failure("AST Map input query", "not destructuring"));
        };
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(failure(
                "AST Map input query",
                "expected two named bindings",
            ));
        };
        let expression = ast
            .expressions()
            .get(*source)
            .map_err(|error| failure("AST Map input query", error))?;
        if !matches!(expression.payload(), Expression::Name) {
            return Err(at(
                &fixture.sources,
                "AST Map input query",
                "source must be Name",
                Some(expression.span()),
            ));
        }
        Ok(LoopSite {
            statement: *statement,
            span: node.span(),
            source: *source,
            source_span: expression.span(),
            bindings: [*key, *value],
        })
    }
}
mod identity {
    //! 返回的真实 entry ID、源码 Origin 与从 entry block 出发的 CFG/DirectCall 可达身份。
    use super::fixture::{Failure, Fixture, failure, location};
    use crate::ssa::model::{FunctionId, Operation, Origin, Program};
    use crate::test_support::diagnostics::diagnostic;
    use lang_frontend::{
        parser::{Item, NameMarker},
        source::{SourceMap, Span},
    };

    pub(super) fn source_functions(
        fixture: &Fixture,
    ) -> Result<Vec<(&'static str, Span)>, Failure> {
        let parsed = &fixture.parsed;
        if parsed.roots().len() != 4 {
            return Err(failure(
                "source function identity query",
                ("expected four function roots", parsed.roots().len()),
            ));
        }
        let factory = if fixture.map {
            "makeMap"
        } else {
            "makeMutable"
        };
        let mut functions = Vec::new();
        for expected in ["entry", "scan", "read", factory] {
            let mut spans = Vec::new();
            for root in parsed.roots() {
                let node = parsed
                    .ast()
                    .items()
                    .get(*root)
                    .map_err(|error| failure("source function identity query", error))?;
                if let Item::Function {
                    name: NameMarker::Present(name),
                    ..
                } = node.payload()
                    && fixture.sources.slice(*name).ok() == Some(expected)
                {
                    if node.span().source_id() != parsed.source_id()
                        || fixture.sources.source_name(node.span().source_id()) != Ok(fixture.path)
                    {
                        return Err(failure(
                            "source function identity query",
                            location(&fixture.sources, node.span()),
                        ));
                    }
                    spans.push(node.span());
                }
            }
            let [span] = spans.as_slice() else {
                return Err(failure("source function identity query", (expected, spans)));
            };
            functions.push((expected, *span));
        }
        Ok(functions)
    }

    pub(super) fn entry_sources(
        sources: &SourceMap,
        program: &Program,
        entry: FunctionId,
        expected: &[(&str, Span)],
    ) -> Result<(), Failure> {
        let [module] = program.modules.as_slice() else {
            return Err(failure(
                "SSA source identity query",
                "expected one actual source module",
            ));
        };
        let mut required = Vec::new();
        for (name, span) in expected {
            let matches = module
                .functions
                .iter()
                .filter(
                    |function| matches!(&function.origin, Origin::Source(actual) if actual == span),
                )
                .collect::<Vec<_>>();
            let [function] = matches.as_slice() else {
                return Err(failure(
                    "SSA source identity query",
                    (name, location(sources, *span), matches.len()),
                ));
            };
            diagnostic!(
                "SPEC0331 PROVIDER FUNCTION source_name={name} id={:?} lowered_name={:?} {}",
                function.id,
                function.name,
                location(sources, *span)
            );
            if *name == "entry" && function.id != entry {
                return Err(failure(
                    "SSA entry identity query",
                    (
                        "lower returned a different source function",
                        entry,
                        function.id,
                    ),
                ));
            }
            required.push(function.id);
        }
        let mut pending = vec![entry];
        let mut reached = Vec::new();
        while let Some(id) = pending.pop() {
            if reached.contains(&id) {
                continue;
            }
            let function = module.function(id).ok_or_else(|| {
                failure(
                    "SSA entry call graph query",
                    ("missing actual function", id),
                )
            })?;
            reached.push(id);
            let entry_block = function.entry_block().ok_or_else(|| {
                failure("SSA entry call graph query", ("missing entry block", id))
            })?;
            let mut blocks = vec![entry_block];
            let mut reached_blocks = Vec::new();
            while let Some(block_id) = blocks.pop() {
                if reached_blocks.contains(&block_id) {
                    continue;
                }
                let block = function.block(block_id).ok_or_else(|| {
                    failure(
                        "SSA entry call graph query",
                        ("missing actual block", block_id),
                    )
                })?;
                reached_blocks.push(block_id);
                // 只读当前可达 block 引用的 instructions，不扫描 function instruction arena。
                for instruction_id in &block.instructions {
                    let instruction = function.instruction(*instruction_id).ok_or_else(|| {
                        failure(
                            "SSA entry call graph query",
                            ("missing referenced instruction", instruction_id),
                        )
                    })?;
                    if let Operation::DirectCall { callee, .. } = &instruction.operation {
                        diagnostic!(
                            "SPEC0331 PROVIDER CALL caller={id:?} block={block_id:?} callee={callee:?} origin={:?}",
                            instruction.origin
                        );
                        pending.push(*callee);
                    }
                }
                let terminator = block.terminator.as_ref().ok_or_else(|| {
                    failure(
                        "SSA entry call graph query",
                        ("missing actual terminator", block_id),
                    )
                })?;
                blocks.extend(terminator.kind.targets());
            }
        }
        if !required.iter().all(|id| reached.contains(id)) {
            return Err(failure("SSA entry call graph query", (required, reached)));
        }
        Ok(())
    }
}
mod pipeline {
    //! 原始 lex/parse/names/raw/complete/ownership/公开完整 validator → 普通真实 lower。
    use super::{
        Report, facts,
        fixture::{Failure, Fixture, clean, complete_failure, failure, lower_failure},
    };
    use crate::ssa::{
        model::{FunctionId, Program},
        unit_lower::lower_owned_unit_with_entry,
    };
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, SourceUnitKey, SymbolKind, index_compilation_unit,
            resolve_compilation_unit_names,
        },
        ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
        parser::{Item, NameMarker},
        type_checking::{
            check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
        },
    };

    type Lowered = (Program, FunctionId);

    // 每次调用独立建立整条 Unit 产品链，不复用另一列的 names/types/ownership。
    pub(super) fn single(fixture: &Fixture, report: &mut Report) -> Result<Lowered, Failure> {
        lower(fixture, report, false)
    }

    pub(super) fn unit(fixture: &Fixture, report: &mut Report) -> Result<Lowered, Failure> {
        lower(fixture, report, true)
    }

    fn lower(fixture: &Fixture, report: &mut Report, rooted: bool) -> Result<Lowered, Failure> {
        let sources = &fixture.sources;
        let parsed = &fixture.parsed;
        // 原单源列使用真实 MemoryDocument；原 Unit 列保留 root/provider.ko。
        // SourceMap 路径、源码字节和默认包不变；不加 wrapper 或第二 source。
        let inputs = [if rooted {
            SourceUnitInput::new("root", fixture.path, parsed.source_id(), parsed)
        } else {
            SourceUnitInput::memory_document(parsed.source_id(), parsed)
        }];
        report.check(
            "unit lex/parse and single original input",
            true,
            (rooted, fixture.path, parsed.source_id()),
        );
        let (name_environment, environment) = standard_environments();
        let index = index_compilation_unit(sources, &inputs)
            .map_err(|error| failure("unit index infrastructure", error))?;
        clean(sources, "unit index diagnostics", index.diagnostics())?;
        let names = resolve_compilation_unit_names(sources, &inputs, &index, &name_environment)
            .map_err(|error| failure("unit names infrastructure", error))?;
        clean(sources, "unit names diagnostics", names.diagnostics())?;
        let names = names
            .validate()
            .map_err(|error| failure("unit names qualification", error))?;
        let [source_unit] = names.names().index().source_units() else {
            return Err(failure(
                "unit source qualification",
                "expected exactly one source",
            ));
        };
        let same_input = source_unit.source_id() == parsed.source_id()
            && parsed.package().is_none()
            && if rooted {
                source_unit
                    .key()
                    .root()
                    .is_some_and(|root| root.as_str() == "root")
                    && source_unit
                        .key()
                        .logical_path()
                        .is_some_and(|path| path.as_str() == fixture.path)
            } else {
                source_unit.key() == &SourceUnitKey::MemoryDocument
            };
        report.check(
            "unit original input kind and source identity",
            same_input,
            source_unit,
        );
        if !same_input {
            return Err(failure(
                "unit source qualification",
                "expected original default-package source and selected input kind",
            ));
        }
        let entries = names.names().index().declarations().iter().filter(|declaration| {
            declaration.name() == "entry" && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == source_unit.id()
                && declaration.name_span().source_id() == parsed.source_id()
                && names.names().index().packages().get(declaration.package().index())
                    .is_some_and(|package| package.name().expect("named package").segments().is_empty())
                && parsed.roots().contains(&declaration.root())
                && parsed.ast().items().get(declaration.root()).is_ok_and(|node| {
                    matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. } if *name == declaration.name_span())
                })
        }).collect::<Vec<_>>();
        let [entry] = entries.as_slice() else {
            return Err(failure(
                "unit real default-package entry selection",
                entries,
            ));
        };
        report.check(
            "unit actual source-qualified default-package entry",
            true,
            entry,
        );
        let raw = check_compilation_unit_types(sources, &inputs, &names, &environment)
            .map_err(|error| failure("unit raw type infrastructure", error))?;
        report.check(
            "unit raw diagnostics clean",
            raw.diagnostics().is_empty(),
            raw.diagnostics(),
        );
        {
            let complete =
                complete_compilation_unit_types(sources, &inputs, &names, &environment, &raw)
                    .map_err(|error| complete_failure(sources, error))?;
            let same = complete.is_same_analysis(&raw)
                && std::ptr::eq(complete.types(), &raw)
                && raw.is_compatible_with(sources, &inputs, &names, &environment);
            report.check(
                "unit complete keeps original raw analysis",
                same,
                "same raw product and input chain",
            );
            if !same {
                return Err(failure(
                    "unit complete analysis identity",
                    "original raw identity mismatch",
                ));
            }
        }
        clean(sources, "unit raw diagnostics", raw.diagnostics())?;
        let typed = raw
            .validate()
            .map_err(|error| failure("unit basic type qualification", error))?;
        report.check(
            "unit original raw consumed by basic qualification",
            true,
            "validate: Ok",
        );
        let owned =
            check_compilation_unit_ownership(sources, &inputs, &names, &environment, &typed)
                .map_err(|error| failure("unit ownership infrastructure", error))?;
        clean(sources, "unit ownership diagnostics", owned.diagnostics())?;
        if !owned.deferred().is_empty() {
            return Err(failure(
                "unit ownership deferred",
                owned
                    .deferred()
                    .iter()
                    .map(|deferred| {
                        let expression = deferred.expression();
                        let location = if expression.source_unit() == source_unit.id() {
                            Some(
                                parsed
                                    .ast()
                                    .expressions()
                                    .get(expression.expression())
                                    .map(|node| super::fixture::location(sources, node.span())),
                            )
                        } else {
                            None
                        };
                        (deferred, location)
                    })
                    .collect::<Vec<_>>(),
            ));
        }
        let same = owned.is_compatible_with(&typed);
        report.check(
            "unit ownership original analysis and no deferred",
            same,
            owned.deferred(),
        );
        if !same {
            return Err(failure(
                "unit ownership analysis identity",
                "typed identity mismatch",
            ));
        }
        facts::unit(fixture, &names, typed.types(), &owned, report);
        // 公开 validate 包括完整 iterations_are_valid；不是部分 query 代替生产资格。
        let owned = owned
            .validate()
            .map_err(|error| failure("unit full ownership validator", error))?;
        report.check("unit full ownership validator", true, "validate: Ok");
        let view =
            owned_compilation_unit_view(sources, &inputs, &names, &environment, &typed, &owned)
                .map_err(|error| failure("unit owned view qualification", error))?;
        let same = std::ptr::eq(view.sources(), sources)
            && std::ptr::eq(view.inputs(), inputs.as_slice())
            && std::ptr::eq(view.names(), &names)
            && std::ptr::eq(view.types(), typed.types())
            && std::ptr::eq(view.ownership(), owned.ownership());
        report.check(
            "unit view preserves all original references",
            same,
            "source/input/name/type/ownership",
        );
        if !same {
            return Err(failure(
                "unit view identity",
                "original references mismatch",
            ));
        }
        report.check("unit lower actually attempted", true, entry.id());
        lower_owned_unit_with_entry(&view, entry.id())
            .map_err(|error| lower_failure(sources, error))
    }
}
#[allow(unused_imports)]
pub(in crate::ssa) use facts::query_inputs;
mod query_llvm;
#[allow(unused_imports)]
pub(in crate::ssa) use facts::query_owner;
mod query_ssa;
mod query_test {
    //! 两源 × MemoryDocument/logical-path Unit 的独立 ordinary query；不变异、不额外创建源。
    use super::{
        Observation, Report,
        fixture::{Fixture, failure, verify_failure},
        identity, pipeline, query_llvm, query_ssa,
    };
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};

    fn observe(label: &'static str, text: &str, unit: bool) -> Observation {
        let mut report = Report {
            identity_requested: true,
            ..Report::default()
        };
        let outcome = Fixture::new(label, text).and_then(|fixture| {
            diagnostic!(
                "SPEC0331 PROVIDER IDENTITY INPUT case={label} unit={unit} path={:?} text={text:?}",
                fixture.path
            );
            let expected = identity::source_functions(&fixture);
            report.query("identity original source function AST", &expected);
            let (program, entry) = if unit {
                pipeline::unit(&fixture, &mut report)
            } else {
                pipeline::single(&fixture, &mut report)
            }?;
            diagnostic!(
                "SPEC0331 PROVIDER IDENTITY RAW SSA case={label} unit={unit}\n{}",
                crate::ssa::render::render_program(&program)
            );
            let origin = expected.and_then(|expected| {
                identity::entry_sources(&fixture.sources, &program, entry, &expected)
            });
            report.query(
                "identity actual returned entry and reachable source functions",
                &origin,
            );
            let inputs = report
                .fe_identity
                .take()
                .unwrap_or_else(|| Err(failure("identity FE bridge missing", (label, unit))));
            report.query(
                "identity FE original parameter/call/cleanup bridge",
                &inputs,
            );
            let shape = inputs
                .as_ref()
                .map_err(|e| failure("identity FE prerequisite", e))
                .and_then(|inputs| query_ssa::check(&program, inputs));
            report.query(
                "independent SSA provider entity and full CFG identity",
                &shape,
            );
            // Queries cannot short-circuit the original lower/SSA/LLVM observations.
            let verified = crate::ssa::verify::verify_program(&program)
                .map_err(|errors| verify_failure(&fixture.sources, errors));
            report.query("identity pipeline SSA verifier", &verified);
            let rendered = crate::llvm::render_verified_program(&program)
                .map_err(|error| failure("LLVM render/verify", error));
            report.query(
                "identity pipeline LLVM render/verify",
                &rendered
                    .as_ref()
                    .map(|s| s.len())
                    .map_err(|e| failure(e.stage, &e.original)),
            );
            if let Ok(llvm) = &rendered {
                diagnostic!("SPEC0331 PROVIDER IDENTITY RAW LLVM case={label} unit={unit}\n{llvm}");
                let identity = match (&inputs, &shape) {
                    (Ok(inputs), Ok(shape)) => query_llvm::check(&program, inputs, shape, llvm),
                    _ => Err(failure(
                        "LLVM identity anchors unavailable",
                        (&inputs, &shape),
                    )),
                };
                report.query(
                    "independent LLVM actual operands phi and checked-slot CFG",
                    &identity,
                );
            } else {
                report.check(
                    "independent LLVM actual operands phi and checked-slot CFG",
                    false,
                    "render failed: no artifact to inspect",
                );
            }
            verified?;
            rendered?;
            Ok(())
        });
        Observation {
            label,
            unit,
            report,
            outcome,
        }
    }

    #[test]
    fn normal_map_provider_preserves_source_component_cursor_identity() {
        let mut observations = Vec::new();
        for (label, source, _golden) in super::sources() {
            for unit in [false, true] {
                observations.push(observe(label, source, unit));
            }
        }
        for observation in &observations {
            diagnostic!(
                "SPEC0331 PROVIDER IDENTITY OUTCOME case={} unit={} report={:#?} result={:#?}",
                observation.label,
                observation.unit,
                observation.report,
                observation.outcome
            );
        }
        assert_diagnostic!(
            observations.len() == 4
                && observations
                    .iter()
                    .all(|o| o.report.passed() && o.outcome.is_ok()),
            "原两source×MemoryDocument/logical-path Unit必须完整FE/lower/verify/LLVM及独立实体query全绿；不以verifier替代身份，不跳过失败格。\n{observations:#?}"
        );
    }
}

use fixture::{Failure, Fixture};

/// 同字节源码和拟议 stdout，供后续独立获准的 native 测试复用。
/// 本 exact 只消费源码；golden 是期望，不是执行结果。
pub(crate) fn sources() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            "map",
            include_str!("../../tests/fixtures/map-provider-0331/map.ko"),
            include_str!("../../tests/fixtures/map-provider-0331/map.stdout"),
        ),
        (
            "mutable-map",
            include_str!("../../tests/fixtures/map-provider-0331/mutable-map.ko"),
            include_str!("../../tests/fixtures/map-provider-0331/mutable-map.stdout"),
        ),
    ]
}

#[derive(Debug, Default)]
struct Report {
    identity_requested: bool,
    fe_identity: Option<Result<query_inputs::Inputs, Failure>>,
    checks: Vec<(&'static str, bool, String)>,
}

impl Report {
    fn check(&mut self, name: &'static str, passed: bool, detail: impl std::fmt::Debug) {
        self.checks.push((name, passed, format!("{detail:?}")));
    }

    fn query<T: std::fmt::Debug>(&mut self, name: &'static str, result: &Result<T, Failure>) {
        self.check(name, result.is_ok(), result);
    }

    fn passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|(_, passed, _)| *passed)
    }
}

#[derive(Debug)]
struct Observation {
    label: &'static str,
    unit: bool,
    report: Report,
    outcome: Result<(), Failure>,
}

fn observe(label: &'static str, text: &str, unit: bool) -> Observation {
    let mut report = Report::default();
    let outcome = Fixture::new(label, text).and_then(|fixture| {
        // false 是 MemoryDocument，true 是原 logical-path Unit；各自保持原始单源字节。
        diagnostic!(
            "SPEC0331 PROVIDER INPUT case={label} unit={unit} path={:?} text={text:?}",
            fixture.path
        );
        let expected_functions = identity::source_functions(&fixture);
        report.query("source function AST identity", &expected_functions);
        let lowered = if unit {
            pipeline::unit(&fixture, &mut report)
        } else {
            pipeline::single(&fixture, &mut report)
        }?;
        let (program, entry) = lowered;
        diagnostic!(
            "SPEC0331 PROVIDER SSA case={label} unit={unit}\n{}",
            super::render::render_program(&program)
        );
        // 只读 query 的失败留在独立 report，不吞 lower 已到达的 verify/LLVM 原错误。
        let identity = expected_functions.and_then(|expected| {
            identity::entry_sources(&fixture.sources, &program, entry, &expected)
        });
        report.query("returned entry and reachable source functions", &identity);
        super::verify::verify_program(&program)
            .map_err(|errors| fixture::verify_failure(&fixture.sources, errors))?;
        report.check("SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|error| fixture::failure("LLVM render/verify", error))?;
        report.check("LLVM render/verify", true, llvm.len());
        diagnostic!("SPEC0331 PROVIDER LLVM case={label} unit={unit}\n{llvm}");
        Ok(())
    });
    Observation {
        label,
        unit,
        report,
        outcome,
    }
}

#[test]
fn map_and_mutable_map_normal_sources_reach_verified_llvm() {
    let mut observations = Vec::new();
    for (label, source, _golden) in sources() {
        for unit in [false, true] {
            observations.push(observe(label, source, unit));
        }
    }
    // 所有真实观察结束后只有这一个硬成功断言，首红不能短路另外三格。
    for observation in &observations {
        diagnostic!(
            "SPEC0331 PROVIDER OUTCOME case={} unit={} report={:#?}",
            observation.label,
            observation.unit,
            observation.report
        );
        match &observation.outcome {
            Ok(()) => diagnostic!("stage=LLVM render/verify result=ok"),
            Err(error) => diagnostic!("stage={} original={}", error.stage, error.original),
        }
    }
    assert_diagnostic!(
        observations.len() == 4
            && observations
                .iter()
                .all(|observation| { observation.report.passed() && observation.outcome.is_ok() }),
        "四格必须是同源完整 frontend、真实 entry/CFG 可达身份、SSA 与 LLVM 全成功；\
         原阶段失败保持失败，UnsupportedNode 不是成功 oracle。\n{observations:#?}"
    );
}
