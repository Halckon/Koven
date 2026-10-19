//! 普通 fixture、真实 AST 位置和阶段化结果；无生产 facts 构造器。

use super::super::{
    LoweringError,
    model::{FunctionId, Origin, Program},
    verify::verify_program,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    diagnostic::Diagnostic,
    lexer::lex,
    parser::{ForBinding, Item, NameMarker, ParsedFile, Statement, parse_file},
    source::{SourceMap, Span},
    type_checking::{IntrinsicTypeConstructor, MapIterationKind},
};

pub(super) const FOR_TEXT: &str =
    "for ((key, value) in source) { val copied: Int = key; println(value) }";

#[derive(Clone, Copy)]
pub(super) struct SourceCase {
    pub path: &'static str,
    pub text: &'static str,
    pub constructor: Option<(IntrinsicTypeConstructor, MapIterationKind)>,
}

#[derive(Debug)]
pub(super) struct Stopped {
    pub stage: &'static str,
    pub original: String,
}

pub(super) fn stopped(stage: &'static str, error: impl std::fmt::Debug) -> Stopped {
    Stopped {
        stage,
        original: format!("{error:?}"),
    }
}

pub(super) fn clean(
    sources: &SourceMap,
    stage: &'static str,
    diagnostics: &[Diagnostic],
) -> Result<(), Stopped> {
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(stopped(
            stage,
            diagnostics
                .iter()
                .map(|item| {
                    (
                        item,
                        sources.source_name(item.primary_span().source_id()),
                        sources.slice(item.primary_span()),
                    )
                })
                .collect::<Vec<_>>(),
        ))
    }
}

pub(super) struct File {
    pub path: String,
    pub parsed: ParsedFile,
    pub constructor: Option<(IntrinsicTypeConstructor, MapIterationKind)>,
}

pub(super) struct Fixture {
    pub sources: SourceMap,
    pub files: Vec<File>,
}

impl Fixture {
    pub fn new(cases: &[SourceCase]) -> Result<Self, Stopped> {
        let mut fixture = Self {
            sources: SourceMap::new(),
            files: Vec::new(),
        };
        for case in cases {
            fixture.add(case.path, case.text, case.constructor)?;
        }
        Ok(fixture)
    }

    pub fn single_whole(text: &str) -> Result<Self, Stopped> {
        let mut fixture = Self {
            sources: SourceMap::new(),
            files: Vec::new(),
        };
        fixture.add("whole.ko", text, None)?;
        Ok(fixture)
    }

    pub fn function_location(&self, name_span: Span) -> Option<Location> {
        let spans = self
            .files
            .iter()
            .filter(|file| file.parsed.source_id() == name_span.source_id())
            .flat_map(|file| file.parsed.ast().items().iter())
            .filter_map(|(_, node)| {
                matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. } if *name == name_span)
                    .then_some(node.span())
            })
            .collect::<Vec<_>>();
        let [span] = spans.as_slice() else {
            return None;
        };
        location(&self.sources, *span)
    }

    fn add(
        &mut self,
        path: &str,
        text: &str,
        constructor: Option<(IntrinsicTypeConstructor, MapIterationKind)>,
    ) -> Result<(), Stopped> {
        let source = self
            .sources
            .add_source(path, text)
            .map_err(|error| stopped("source infrastructure", error))?;
        let lexed =
            lex(&self.sources, source).map_err(|error| stopped("lex infrastructure", error))?;
        clean(&self.sources, "lex diagnostics", lexed.diagnostics())?;
        let parsed = parse_file(&self.sources, &lexed)
            .map_err(|error| stopped("parse infrastructure", error))?;
        clean(&self.sources, "parse diagnostics", parsed.diagnostics())?;
        self.files.push(File {
            path: path.to_owned(),
            parsed,
            constructor,
        });
        Ok(())
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

pub(super) fn loop_sites(file: &File) -> Result<Vec<LoopSite>, Stopped> {
    let ast = file.parsed.ast();
    let mut sites = Vec::new();
    for (statement, node) in ast.statements().iter() {
        let Statement::For {
            binding, source, ..
        } = node.payload()
        else {
            continue;
        };
        let ForBinding::Destructuring { names, .. } = binding else {
            return Err(stopped("AST query", "For requires destructuring"));
        };
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(stopped("AST query", "For requires two real bindings"));
        };
        let source_span = ast
            .expressions()
            .get(*source)
            .map_err(|error| stopped("AST query", error))?
            .span();
        sites.push(LoopSite {
            statement,
            span: node.span(),
            source: *source,
            source_span,
            bindings: [*key, *value],
        });
    }
    Ok(sites)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Location {
    pub span: Span,
    pub path: String,
    pub text: String,
}

pub(super) fn location(sources: &SourceMap, span: Span) -> Option<Location> {
    Some(Location {
        span,
        path: sources.source_name(span.source_id()).ok()?.to_owned(),
        text: sources.slice(span).ok()?.to_owned(),
    })
}

/// 每条原始观察现在只接受真实 SSA/LLVM 成功；前置 query 失败仍独立失败。
#[derive(Debug)]
pub(super) struct LowerOutcome {
    error: Option<LoweringError>,
    success: Option<SuccessfulLower>,
}

#[derive(Debug)]
struct SuccessfulLower {
    entry: FunctionId,
    expected_entry: Option<Location>,
    actual_entry: Option<Location>,
    entry_identity: Option<FunctionId>,
    ssa_verifier: Result<(), String>,
    verified_llvm_bytes: Result<usize, String>,
}

impl LowerOutcome {
    pub fn new(
        sources: &SourceMap,
        expected_entry: Option<Location>,
        result: Result<(Program, FunctionId), LoweringError>,
    ) -> Self {
        let (error, success) = match result {
            Err(error) => (Some(error), None),
            Ok((program, entry)) => {
                // 使用 lower 返回的真实 ID 查询本 Program，不按函数名或固定槽索引替代。
                let function = program
                    .module(entry.module())
                    .and_then(|module| module.function(entry));
                let actual_entry = function.and_then(|function| match &function.origin {
                    Origin::Source(span) => location(sources, *span),
                    Origin::Synthetic { .. } => None,
                });
                let entry_identity = function
                    .filter(|_| expected_entry.is_some() && actual_entry == expected_entry)
                    .map(|function| function.id());
                // 来源 query 即使失败仍执行两条真实 verifier 路径；LLVM 入口会再次验证 SSA。
                let ssa_verifier = verify_program(&program).map_err(|error| format!("{error:?}"));
                let verified_llvm_bytes = crate::llvm::render_verified_program(&program)
                    .map(|llvm| llvm.len())
                    .map_err(|error| format!("{error:?}"));
                (
                    None,
                    Some(SuccessfulLower {
                        entry,
                        expected_entry,
                        actual_entry,
                        entry_identity,
                        ssa_verifier,
                        verified_llvm_bytes,
                    }),
                )
            }
        };
        Self { error, success }
    }

    pub fn matches(&self) -> bool {
        self.error.is_none()
            && self.success.as_ref().is_some_and(|success| {
                // 同一分析的真实返回 ID 与源身份；无跨分析裸 ID 比较。
                success
                    .actual_entry
                    .as_ref()
                    .is_some_and(|actual| Some(actual) == success.expected_entry.as_ref())
                    && success.entry_identity == Some(success.entry)
                    && success.ssa_verifier.is_ok()
                    && success
                        .verified_llvm_bytes
                        .as_ref()
                        .is_ok_and(|bytes| *bytes > 0)
            })
    }
}
