use std::{collections::BTreeMap, error::Error, fmt};

use crate::{
    ast::{AstError, ItemId},
    diagnostic::{Diagnostic, DiagnosticCodeError, DiagnosticError, Severity, codes},
    lexer::{LexemeKind, LexerInternalError, TokenKind, lex},
    name_resolution::{Namespace, SymbolKind},
    parser::{Item, NameMarker, ParsedFile, VisibilityModifier},
    source::{SourceError, SourceId, SourceMap},
};

use super::model::{
    CompilationUnitIndex, DeclarationId, DeclarationVisibility, LogicalSourcePath, Package,
    PackageId, PackageName, PackagePathMismatch, SourceRootIdentity, SourceUnit, SourceUnitId,
    SourceUnitInput, SourceUnitKey, UnitDeclaration,
};
use super::{UnitDiagnosticOrderError, ordered_unit_diagnostics};

/// compilation-unit 输入契约失败；这些错误不能归因于 Koven 源码，不使用 L-code。
#[derive(Debug)]
pub enum CompilationUnitInputError {
    /// 逻辑路径不满足规范相对路径约束。
    InvalidLogicalPath {
        /// 被拒绝的原始路径。
        path: String,
        /// 具体失败原因。
        reason: LogicalPathError,
    },
    /// 两项输入使用相同的稳定源码键。
    DuplicateSourceKey {
        /// 重复 root identity。
        root: String,
        /// 重复逻辑路径。
        logical_path: String,
    },
    /// 同一 SourceId 被重复加入 unit。
    DuplicateSource {
        /// 重复的源码身份。
        source_id: SourceId,
    },
    /// 输入 SourceId 与 ParsedFile 所属源码不同。
    MismatchedParsedSource {
        /// 输入声明的源码身份。
        source_id: SourceId,
        /// ParsedFile 实际源码身份。
        parsed_source_id: SourceId,
    },
    /// SourceMap owner/range 校验失败。
    Source(SourceError),
    /// ParsedFile AST 关系无效。
    Ast(AstError),
    /// 生产诊断码目录无效。
    DiagnosticCode(DiagnosticCodeError),
    /// 无法构造受检 unit 诊断。
    Diagnostic(DiagnosticError),
    /// 无法按稳定源码键排序 unit 诊断。
    DiagnosticOrder(UnitDiagnosticOrderError),
    /// 复用语言 lexer 校验 package segment 时发生内部失败。
    Lexer(LexerInternalError),
}

/// 逻辑路径违反 ADR-0005 规范的原因。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogicalPathError {
    /// 路径为空。
    Empty,
    /// 路径以 `/` 开始。
    Absolute,
    /// 路径包含空 segment。
    EmptySegment,
    /// 路径包含 `.` segment。
    CurrentSegment,
    /// 路径包含 `..` segment。
    ParentSegment,
    /// 父目录 segment 不是 Koven Identifier。
    InvalidPackageSegment {
        /// 被拒绝的 segment。
        segment: String,
    },
}

impl fmt::Display for CompilationUnitInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLogicalPath { path, reason } => {
                write!(formatter, "invalid logical source path {path:?}: {reason}")
            }
            Self::DuplicateSourceKey { root, logical_path } => write!(
                formatter,
                "duplicate source key ({root:?}, {logical_path:?})"
            ),
            Self::DuplicateSource { source_id } => {
                write!(formatter, "source {source_id:?} appears more than once")
            }
            Self::MismatchedParsedSource {
                source_id,
                parsed_source_id,
            } => write!(
                formatter,
                "source input {source_id:?} carries parsed file {parsed_source_id:?}"
            ),
            Self::Source(error) => write!(formatter, "invalid source identity: {error}"),
            Self::Ast(error) => write!(formatter, "invalid parser AST: {error}"),
            Self::DiagnosticCode(error) => write!(formatter, "invalid diagnostic catalog: {error}"),
            Self::Diagnostic(error) => write!(formatter, "invalid unit diagnostic: {error}"),
            Self::DiagnosticOrder(error) => {
                write!(formatter, "could not order unit diagnostics: {error}")
            }
            Self::Lexer(error) => write!(formatter, "package-segment lexer failed: {error}"),
        }
    }
}

impl fmt::Display for LogicalPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "path is empty",
            Self::Absolute => "path is absolute",
            Self::EmptySegment => "path contains an empty segment",
            Self::CurrentSegment => "path contains a current-directory segment",
            Self::ParentSegment => "path contains a parent-directory segment",
            Self::InvalidPackageSegment { .. } => "parent segment is not a Koven identifier",
        })
    }
}

impl Error for CompilationUnitInputError {}
impl From<SourceError> for CompilationUnitInputError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}
impl From<AstError> for CompilationUnitInputError {
    fn from(error: AstError) -> Self {
        Self::Ast(error)
    }
}
impl From<DiagnosticCodeError> for CompilationUnitInputError {
    fn from(error: DiagnosticCodeError) -> Self {
        Self::DiagnosticCode(error)
    }
}
impl From<DiagnosticError> for CompilationUnitInputError {
    fn from(error: DiagnosticError) -> Self {
        Self::Diagnostic(error)
    }
}
impl From<UnitDiagnosticOrderError> for CompilationUnitInputError {
    fn from(error: UnitDiagnosticOrderError) -> Self {
        Self::DiagnosticOrder(error)
    }
}
impl From<LexerInternalError> for CompilationUnitInputError {
    fn from(error: LexerInternalError) -> Self {
        Self::Lexer(error)
    }
}

struct CanonicalInput<'parsed> {
    key: SourceUnitKey,
    source_id: SourceId,
    parsed: &'parsed ParsedFile,
    expected_package: PackageName,
}

/// 校验并规范化 compilation-unit 输入，建立 package/source/declaration identity。
///
/// 此入口只完成 SPEC-0025 的首个 identity 边界；import lookup 由后续 resolver 接入。
pub fn index_compilation_unit(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
) -> Result<CompilationUnitIndex, CompilationUnitInputError> {
    // fail-fast 错误不能继承调用方枚举顺序：先排序原始借用输入，再按既定优先级校验
    // path/source/owner，这样正序与反序调用会选择同一个输入错误。
    let mut ordered_inputs = inputs.to_vec();
    ordered_inputs.sort_by(|left, right| {
        (left.root_identity(), left.logical_path())
            .cmp(&(right.root_identity(), right.logical_path()))
    });
    let mut canonical = Vec::with_capacity(inputs.len());
    let mut source_ids = Vec::with_capacity(inputs.len());
    for input in ordered_inputs {
        validate_logical_path(input.logical_path())?;
        sources.source_text(input.source_id())?;
        if input.source_id() != input.parsed().source_id() {
            return Err(CompilationUnitInputError::MismatchedParsedSource {
                source_id: input.source_id(),
                parsed_source_id: input.parsed().source_id(),
            });
        }
        if source_ids.contains(&input.source_id()) {
            return Err(CompilationUnitInputError::DuplicateSource {
                source_id: input.source_id(),
            });
        }
        source_ids.push(input.source_id());

        let logical_path = LogicalSourcePath::from_validated(input.logical_path().to_owned());
        let expected_package =
            PackageName::new(logical_path.package_segments().map(str::to_owned).collect());
        canonical.push(CanonicalInput {
            key: SourceUnitKey::new(SourceRootIdentity::new(input.root_identity()), logical_path),
            source_id: input.source_id(),
            parsed: input.parsed(),
            expected_package,
        });
    }
    canonical.sort_by(|left, right| left.key.cmp(&right.key));
    if let Some(pair) = canonical.windows(2).find(|pair| pair[0].key == pair[1].key) {
        return Err(CompilationUnitInputError::DuplicateSourceKey {
            root: pair[0].key.root().as_str().to_owned(),
            logical_path: pair[0].key.logical_path().as_str().to_owned(),
        });
    }

    let mut package_names: Vec<_> = canonical
        .iter()
        .map(|input| input.expected_package.clone())
        .collect();
    package_names.sort();
    package_names.dedup();
    let packages: Vec<_> = package_names
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, name)| Package::new(PackageId(index), name))
        .collect();
    let package_ids: BTreeMap<_, _> = packages
        .iter()
        .map(|package| (package.name().clone(), package.id()))
        .collect();

    let mut source_units = Vec::with_capacity(canonical.len());
    let mut mismatches = Vec::new();
    let mut declarations = Vec::new();
    for (index, input) in canonical.iter().enumerate() {
        let source_unit_id = SourceUnitId(index);
        let package_id = package_ids[&input.expected_package];
        source_units.push(SourceUnit::new(
            source_unit_id,
            input.key.clone(),
            input.source_id,
            package_id,
        ));
        if let Some(mismatch) = package_mismatch(sources, input, source_unit_id)? {
            mismatches.push(mismatch);
        }
        collect_declarations(
            sources,
            input.parsed,
            source_unit_id,
            package_id,
            &mut declarations,
        )?;
    }

    let mut diagnostics: Vec<_> = canonical
        .iter()
        .flat_map(|input| input.parsed.diagnostics().iter().cloned())
        .collect();
    diagnostics.extend(build_index_diagnostics(
        sources,
        &mismatches,
        &declarations,
    )?);
    let diagnostics = ordered_unit_diagnostics(sources, &source_units, &diagnostics)?
        .into_iter()
        .cloned()
        .collect();
    Ok(CompilationUnitIndex::new(
        packages,
        source_units,
        declarations,
        mismatches,
        diagnostics,
    ))
}

fn validate_logical_path(path: &str) -> Result<(), CompilationUnitInputError> {
    let invalid = |reason| CompilationUnitInputError::InvalidLogicalPath {
        path: path.to_owned(),
        reason,
    };
    if path.is_empty() {
        return Err(invalid(LogicalPathError::Empty));
    }
    if path.starts_with('/') {
        return Err(invalid(LogicalPathError::Absolute));
    }
    let segments: Vec<_> = path.split('/').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(invalid(LogicalPathError::EmptySegment));
    }
    if segments.contains(&".") {
        return Err(invalid(LogicalPathError::CurrentSegment));
    }
    if segments.contains(&"..") {
        return Err(invalid(LogicalPathError::ParentSegment));
    }
    for segment in &segments[..segments.len() - 1] {
        if !is_koven_identifier(segment)? {
            return Err(invalid(LogicalPathError::InvalidPackageSegment {
                segment: (*segment).to_owned(),
            }));
        }
    }
    Ok(())
}

fn is_koven_identifier(segment: &str) -> Result<bool, LexerInternalError> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("<logical-package-segment>", segment)
        .expect("a fresh source map accepts its first source");
    let lexed = lex(&sources, source)?;
    Ok(lexed.diagnostics().is_empty()
        && matches!(
            lexed.lexemes(),
            [lexeme, eof]
                if lexeme.kind() == LexemeKind::Token(TokenKind::Identifier)
                    && eof.kind() == LexemeKind::Eof
        ))
}

fn package_mismatch(
    sources: &SourceMap,
    input: &CanonicalInput<'_>,
    source_unit: SourceUnitId,
) -> Result<Option<PackagePathMismatch>, CompilationUnitInputError> {
    if has_malformed_package_header(input.parsed) {
        return Ok(None);
    }
    let declared = input
        .parsed
        .package()
        .map(|directive| {
            directive
                .segments
                .iter()
                .map(|segment| sources.slice(segment.span).map(str::to_owned))
                .collect::<Result<Vec<_>, _>>()
                .map(PackageName::new)
        })
        .transpose()?;
    let matches = declared
        .as_ref()
        .map_or(input.expected_package.is_default(), |name| {
            name == &input.expected_package
        });
    if matches {
        return Ok(None);
    }
    let primary_span = input.parsed.package().map_or_else(
        || sources.span(input.source_id, 0, 0),
        |directive| Ok(directive.span),
    )?;
    Ok(Some(PackagePathMismatch::new(
        source_unit,
        input.expected_package.clone(),
        declared,
        primary_span,
    )))
}

fn collect_declarations(
    sources: &SourceMap,
    parsed: &ParsedFile,
    source_unit: SourceUnitId,
    package: PackageId,
    declarations: &mut Vec<UnitDeclaration>,
) -> Result<(), CompilationUnitInputError> {
    for &root in parsed.roots() {
        let (item, visibility) = unwrap_item(parsed, root)?;
        match item {
            Item::Variable { name, .. } => push_declaration(
                sources,
                root,
                name,
                Namespace::Value,
                SymbolKind::Variable,
                visibility,
                source_unit,
                package,
                declarations,
            )?,
            Item::Constant { name, .. } => push_declaration(
                sources,
                root,
                name,
                Namespace::Value,
                SymbolKind::Constant,
                visibility,
                source_unit,
                package,
                declarations,
            )?,
            Item::Function { name, .. } => push_declaration(
                sources,
                root,
                name,
                Namespace::Value,
                SymbolKind::Function,
                visibility,
                source_unit,
                package,
                declarations,
            )?,
            Item::Classifier(classifier) => {
                push_declaration(
                    sources,
                    root,
                    classifier.name,
                    Namespace::Type,
                    SymbolKind::Classifier,
                    visibility,
                    source_unit,
                    package,
                    declarations,
                )?;
                if matches!(
                    classifier.kind,
                    crate::parser::ClassifierKind::Object { .. }
                ) {
                    push_declaration(
                        sources,
                        root,
                        classifier.name,
                        Namespace::Value,
                        SymbolKind::ObjectValue,
                        visibility,
                        source_unit,
                        package,
                        declarations,
                    )?;
                }
            }
            Item::Error | Item::Companion(_) | Item::Modified { .. } | Item::Deinit { .. } => {}
        }
    }
    Ok(())
}

fn unwrap_item(
    parsed: &ParsedFile,
    mut item_id: ItemId,
) -> Result<(Item, DeclarationVisibility), AstError> {
    let mut visibility = DeclarationVisibility::Public;
    loop {
        let item = parsed.ast().items().get(item_id)?.payload().clone();
        match item {
            Item::Modified {
                modifiers,
                declaration,
            } => {
                if let Some(explicit) = modifiers.visibility {
                    visibility = match explicit {
                        VisibilityModifier::Public(_) => DeclarationVisibility::Public,
                        VisibilityModifier::Internal(_) => DeclarationVisibility::Internal,
                        VisibilityModifier::Private(_) => DeclarationVisibility::Private,
                    };
                }
                item_id = declaration;
            }
            item => return Ok((item, visibility)),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_declaration(
    sources: &SourceMap,
    root: ItemId,
    marker: NameMarker,
    namespace: Namespace,
    kind: SymbolKind,
    visibility: DeclarationVisibility,
    source_unit: SourceUnitId,
    package: PackageId,
    declarations: &mut Vec<UnitDeclaration>,
) -> Result<(), SourceError> {
    let NameMarker::Present(name_span) = marker else {
        return Ok(());
    };
    declarations.push(UnitDeclaration::new(
        DeclarationId(declarations.len()),
        source_unit,
        root,
        package,
        sources.slice(name_span)?.to_owned(),
        name_span,
        namespace,
        kind,
        visibility,
    ));
    Ok(())
}

fn has_malformed_package_header(parsed: &ParsedFile) -> bool {
    parsed
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code().to_string() == codes::EXPECTED_PACKAGE_NAME)
}

fn build_index_diagnostics(
    sources: &SourceMap,
    mismatches: &[PackagePathMismatch],
    declarations: &[UnitDeclaration],
) -> Result<Vec<Diagnostic>, CompilationUnitInputError> {
    let catalog = codes::catalog()?;
    let package_path_code = catalog.resolve(codes::PACKAGE_PATH_MISMATCH)?;
    let declaration_conflict_code = catalog.resolve(codes::PACKAGE_DECLARATION_CONFLICT)?;
    let mut diagnostics = Vec::new();

    for mismatch in mismatches {
        diagnostics.push(Diagnostic::new(
            sources,
            Severity::Error,
            package_path_code,
            "package declaration does not match logical source path",
            mismatch.primary_span(),
        )?);
    }

    let mut bindings = BTreeMap::<(PackageId, u8, String), Vec<usize>>::new();
    for (index, declaration) in declarations.iter().enumerate() {
        let key = (
            declaration.package(),
            match declaration.namespace() {
                Namespace::Type => 0,
                Namespace::Value => 1,
            },
            declaration.name().to_owned(),
        );
        let prior = bindings.entry(key).or_default();
        // 同文件冲突仍由 L0079 负责，但历史必须保留全部声明：后续文件的函数即使与首个
        // 函数兼容，仍可能和紧随其后的同文件非函数声明冲突。
        let conflicting = prior.iter().copied().find(|&prior_index| {
            let candidate = &declarations[prior_index];
            candidate.source_unit() != declaration.source_unit()
                && !(candidate.kind() == SymbolKind::Function
                    && declaration.kind() == SymbolKind::Function)
        });
        prior.push(index);
        let Some(first_index) = conflicting else {
            continue;
        };
        let first = &declarations[first_index];
        let mut diagnostic = Diagnostic::new(
            sources,
            Severity::Error,
            declaration_conflict_code,
            "conflicting declarations in package",
            declaration.name_span(),
        )?;
        diagnostic.add_label(
            sources,
            first.name_span(),
            "first declaration with this package name",
        )?;
        diagnostics.push(diagnostic);
    }
    Ok(diagnostics)
}
