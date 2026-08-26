use std::{collections::BTreeMap, sync::Arc};

use crate::{
    diagnostic::{Diagnostic, Severity},
    name_resolution::{
        DeclarationId, ExternalSymbolId, SourceUnitInput, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    source::SourceMap,
    type_checking::{ExpressionCategory, ParameterMode, TypeEnvironment},
};

use super::{
    CompilationUnitSignatures, CompilationUnitTypeError, UnitExpressionId, UnitTypeId,
    UnitTypeRefId, UnitTypeTable, ValidatedCompilationUnitSignatures,
};

mod checker;

pub use checker::check_compilation_unit_types;

/// 一个 unit body 中成功选择的静态 call target。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnitCallTarget {
    /// compilation unit 顶层源码函数。
    Declaration(DeclarationId),
    /// source-local callable。
    Symbol(UnitSymbolId),
    /// 编译器绑定的外部函数。
    External(ExternalSymbolId),
    /// 由函数类型值提供的调用目标。
    FunctionValue,
    /// `value class` 自动结构分量。
    StructuralComponent(UnitSymbolId),
}

/// unit call 的静态 target 与完整类型实参 identity。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitCallableInstanceKey {
    target: UnitCallTarget,
    type_arguments: Vec<UnitTypeId>,
}

impl UnitCallableInstanceKey {
    /// 返回唯一静态 call target。
    #[must_use]
    pub const fn target(&self) -> UnitCallTarget {
        self.target
    }

    /// 返回 target 实例化后的完整类型实参。
    #[must_use]
    pub fn type_arguments(&self) -> &[UnitTypeId] {
        &self.type_arguments
    }
}

/// unit call 中一个源码实参到声明参数的映射。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitCallArgumentDescriptor {
    argument_index: usize,
    parameter_index: usize,
    category: ExpressionCategory,
    mode: ParameterMode,
    parameter_type: UnitTypeId,
    cross_thread: bool,
}

impl UnitCallArgumentDescriptor {
    /// 返回实参在源码顺序中的下标。
    #[must_use]
    pub const fn argument_index(self) -> usize {
        self.argument_index
    }

    /// 返回实参映射到的声明参数下标。
    #[must_use]
    pub const fn parameter_index(self) -> usize {
        self.parameter_index
    }

    /// 返回实参的类型层面 place/temporary 类别。
    #[must_use]
    pub const fn category(self) -> ExpressionCategory {
        self.category
    }

    /// 返回声明参数的规范化交付模式。
    #[must_use]
    pub const fn mode(self) -> ParameterMode {
        self.mode
    }

    /// 返回实例化后的声明参数类型。
    #[must_use]
    pub const fn parameter_type(self) -> UnitTypeId {
        self.parameter_type
    }

    /// 返回参数是否由 compiler-bound effect 跨线程交付。
    #[must_use]
    pub const fn crosses_thread(self) -> bool {
        self.cross_thread
    }
}

/// 一个已唯一选择并完成首批 unit body 契约检查的 call。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitCallDescriptor {
    expression: UnitExpressionId,
    instance: UnitCallableInstanceKey,
    return_type: UnitTypeId,
    arguments: Vec<UnitCallArgumentDescriptor>,
    aborts: bool,
    prints_line: bool,
}

impl UnitCallDescriptor {
    /// 返回带 source-unit 限定的 call expression identity。
    #[must_use]
    pub const fn expression(&self) -> UnitExpressionId {
        self.expression
    }

    /// 返回唯一静态 call target。
    #[must_use]
    pub const fn target(&self) -> UnitCallTarget {
        self.instance.target()
    }

    /// 返回 target 与完整类型实参组成的实例 identity。
    #[must_use]
    pub const fn instance(&self) -> &UnitCallableInstanceKey {
        &self.instance
    }

    /// 返回 call 的结果类型。
    #[must_use]
    pub const fn return_type(&self) -> UnitTypeId {
        self.return_type
    }

    /// 返回源码实参顺序的参数映射。
    #[must_use]
    pub fn arguments(&self) -> &[UnitCallArgumentDescriptor] {
        &self.arguments
    }

    /// 返回 call target 是否具有编译器绑定的 abort effect。
    #[must_use]
    pub const fn aborts(&self) -> bool {
        self.aborts
    }

    /// 返回 call target 是否具有编译器绑定的 stdout 行输出 effect。
    #[must_use]
    pub const fn prints_line(&self) -> bool {
        self.prints_line
    }
}

/// body checker 交给 recovery product 的最小、source-qualified facts。
#[derive(Default)]
pub(crate) struct CompilationUnitTypeParts {
    pub(crate) expression_types: BTreeMap<UnitExpressionId, UnitTypeId>,
    pub(crate) expression_categories: BTreeMap<UnitExpressionId, ExpressionCategory>,
    pub(crate) type_ref_types: BTreeMap<UnitTypeRefId, UnitTypeId>,
    pub(crate) symbol_types: BTreeMap<UnitSymbolId, UnitTypeId>,
    pub(crate) calls: Vec<UnitCallDescriptor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BodyTypeProvenance {
    analysis_owner: Arc<()>,
}

/// SPEC-0197 body 阶段的 recovery typed product。
///
/// 类型表仍由内含的 signature product 唯一拥有；body facts 只使用
/// [`UnitTypeId`] 并且所有源码 identity 都带 [`SourceUnitId`] 限定。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilationUnitTypes {
    provenance: BodyTypeProvenance,
    signatures: CompilationUnitSignatures,
    expression_types: BTreeMap<UnitExpressionId, UnitTypeId>,
    expression_categories: BTreeMap<UnitExpressionId, ExpressionCategory>,
    type_ref_types: BTreeMap<UnitTypeRefId, UnitTypeId>,
    symbol_types: BTreeMap<UnitSymbolId, UnitTypeId>,
    calls: Vec<UnitCallDescriptor>,
    body_diagnostics: Vec<Diagnostic>,
    diagnostics: Vec<Diagnostic>,
}

impl CompilationUnitTypes {
    pub(crate) fn new(
        signatures: CompilationUnitSignatures,
        parts: CompilationUnitTypeParts,
        body_diagnostics: Vec<Diagnostic>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        let provenance = BodyTypeProvenance {
            analysis_owner: Arc::new(()),
        };
        Self {
            provenance,
            signatures,
            expression_types: parts.expression_types,
            expression_categories: parts.expression_categories,
            type_ref_types: parts.type_ref_types,
            symbol_types: parts.symbol_types,
            calls: parts.calls,
            body_diagnostics,
            diagnostics,
        }
    }

    /// 检查该产物是否来自给定 inputs、names 与 type environment 身份链。
    #[must_use]
    pub fn is_compatible_with(
        &self,
        sources: &SourceMap,
        inputs: &[SourceUnitInput<'_>],
        names: &ValidatedCompilationUnitNames,
        environment: &TypeEnvironment,
    ) -> bool {
        self.signatures
            .is_compatible_with(sources, inputs, names, environment)
    }

    /// 判断两个 body typed product 是否来自同一次分析；克隆保留身份。
    #[must_use]
    pub fn is_same_analysis(&self, other: &Self) -> bool {
        Arc::ptr_eq(
            &self.provenance.analysis_owner,
            &other.provenance.analysis_owner,
        )
    }

    /// 返回 body 阶段沿用的 unit-wide signatures。
    #[must_use]
    pub const fn signatures(&self) -> &CompilationUnitSignatures {
        &self.signatures
    }

    /// 返回 signature 与 body 共享的唯一 unit-global type table。
    #[must_use]
    pub const fn types(&self) -> &UnitTypeTable {
        self.signatures.types()
    }

    /// 查询 source-qualified expression 的规范类型。
    #[must_use]
    pub fn expression_type(&self, expression: UnitExpressionId) -> Option<UnitTypeId> {
        self.expression_types.get(&expression).copied()
    }

    /// 返回 source-qualified expression typed facts。
    #[must_use]
    pub const fn expression_types(&self) -> &BTreeMap<UnitExpressionId, UnitTypeId> {
        &self.expression_types
    }

    /// 查询 source-qualified expression 的类型层面类别。
    #[must_use]
    pub fn expression_category(&self, expression: UnitExpressionId) -> Option<ExpressionCategory> {
        self.expression_categories.get(&expression).copied()
    }

    /// 返回源码稳定顺序的成功 call facts。
    #[must_use]
    pub fn calls(&self) -> &[UnitCallDescriptor] {
        &self.calls
    }

    /// 查询一个成功 call expression 的 descriptor。
    #[must_use]
    pub fn call(&self, expression: UnitExpressionId) -> Option<&UnitCallDescriptor> {
        self.calls
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 查询 source-qualified type reference 的规范类型。
    #[must_use]
    pub fn type_ref_type(&self, type_ref: UnitTypeRefId) -> Option<UnitTypeId> {
        self.type_ref_types
            .get(&type_ref)
            .copied()
            .or_else(|| self.signatures.type_ref_type(type_ref))
    }

    /// 返回 source-qualified type-reference typed facts。
    #[must_use]
    pub const fn type_ref_types(&self) -> &BTreeMap<UnitTypeRefId, UnitTypeId> {
        &self.type_ref_types
    }

    /// 查询 source-qualified symbol 的 body 类型，再回退到 signature 类型。
    #[must_use]
    pub fn symbol_type(&self, symbol: UnitSymbolId) -> Option<UnitTypeId> {
        self.symbol_types
            .get(&symbol)
            .copied()
            .or_else(|| self.signatures.symbol_type(symbol))
    }

    /// 返回 body 阶段新增的 source-qualified symbol typed facts。
    #[must_use]
    pub const fn body_symbol_types(&self) -> &BTreeMap<UnitSymbolId, UnitTypeId> {
        &self.symbol_types
    }

    /// 返回已经按 stable source key、byte span 与 code 排序的 body 类型诊断。
    ///
    /// signature 诊断仍由 [`Self::signatures`] 暴露；完整 driver 接线后再统一发布聚合集合。
    #[must_use]
    pub fn body_diagnostics(&self) -> &[Diagnostic] {
        &self.body_diagnostics
    }

    /// 返回 signature 与 body 类型阶段统一排序后的诊断。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 只有 signature 与 body 诊断都无 error 时才发布 ownership 可消费的 view。
    pub fn validate(self) -> Result<ValidatedCompilationUnitTypes, Box<Self>> {
        let has_error = self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error);
        if has_error {
            Err(Box::new(self))
        } else {
            Ok(ValidatedCompilationUnitTypes(self))
        }
    }
}

/// 不可伪造的无错误 compilation-unit body typed product。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCompilationUnitTypes(CompilationUnitTypes);

impl ValidatedCompilationUnitTypes {
    /// 返回 recovery product 的只读视图。
    #[must_use]
    pub const fn types(&self) -> &CompilationUnitTypes {
        &self.0
    }

    /// 解包 recovery product。
    #[must_use]
    pub fn into_types(self) -> CompilationUnitTypes {
        self.0
    }
}

/// 核对 body 阶段的 source inputs、validated names、type environment 与 signature owner。
///
/// 该入口只验证分析身份链，不执行 body 类型检查。
pub fn validate_compilation_unit_body_inputs(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    signatures: &ValidatedCompilationUnitSignatures,
) -> Result<(), CompilationUnitTypeError> {
    let unit_names = names.names();
    let rebuilt = crate::name_resolution::index_compilation_unit(sources, inputs)
        .map_err(|_| CompilationUnitTypeError::MismatchedInputs)?;
    if &rebuilt != unit_names.index() {
        return Err(CompilationUnitTypeError::MismatchedInputs);
    }
    if unit_names.source_units().len() != unit_names.index().source_units().len()
        || unit_names.source_units().iter().any(|source| {
            source.resolution().source_id()
                != unit_names.index().source_units()[source.source_unit().index()].source_id()
                || !Arc::ptr_eq(source.resolution().environment_owner(), environment.owner())
        })
    {
        return Err(CompilationUnitTypeError::MismatchedNameEnvironment);
    }
    if !signatures
        .signatures()
        .is_compatible_with(sources, inputs, names, environment)
    {
        return Err(CompilationUnitTypeError::MismatchedSignatures);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        diagnostic::{Diagnostic, Severity, codes},
        lexer::lex,
        name_resolution::{
            NameEnvironment, SourceUnitInput, index_compilation_unit,
            resolve_compilation_unit_names,
        },
        parser::{ParsedFile, parse_file},
        source::{SourceId, SourceMap},
        type_checking::{
            BuiltinType, TypeEnvironment, UnitStatementId, collect_compilation_unit_signatures,
            standard_environments,
        },
    };

    use super::*;

    fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
        let source = sources.add_source(name, text).expect("unique source");
        let lexed = lex(sources, source).expect("lexing succeeds internally");
        let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        (source, parsed)
    }

    fn validated_names<'a>(
        sources: &SourceMap,
        inputs: &[SourceUnitInput<'a>],
        environment: &NameEnvironment,
    ) -> crate::name_resolution::ValidatedCompilationUnitNames {
        let index = index_compilation_unit(sources, inputs).expect("valid unit input");
        resolve_compilation_unit_names(sources, inputs, &index, environment)
            .expect("name resolution succeeds internally")
            .validate()
            .expect("valid names")
    }

    #[test]
    fn source_qualified_ast_identities_do_not_collide() {
        let mut sources = SourceMap::new();
        let (left_source, left) = parsed(
            &mut sources,
            "left.ko",
            "package p\nfun left(): Int { return 1 }",
        );
        let (right_source, right) = parsed(
            &mut sources,
            "right.ko",
            "package p\nfun right(): Int { return 2 }",
        );
        let inputs = [
            SourceUnitInput::new("root", "p/left.ko", left_source, &left),
            SourceUnitInput::new("root", "p/right.ko", right_source, &right),
        ];
        let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
        let left_unit = index.source_units()[0].id();
        let right_unit = index.source_units()[1].id();
        let left_expression = left.ast().expressions().iter().next().expect("literal").0;
        let right_expression = right.ast().expressions().iter().next().expect("literal").0;
        let left_statement = left.ast().statements().iter().next().expect("body").0;
        let right_statement = right.ast().statements().iter().next().expect("body").0;
        let left_type_ref = left.ast().type_refs().iter().next().expect("return type").0;
        let right_type_ref = right
            .ast()
            .type_refs()
            .iter()
            .next()
            .expect("return type")
            .0;

        assert_eq!(left_expression.index(), right_expression.index());
        assert_ne!(
            UnitExpressionId::new(left_unit, left_expression),
            UnitExpressionId::new(right_unit, right_expression)
        );
        assert_eq!(left_statement.index(), right_statement.index());
        assert_ne!(
            UnitStatementId::new(left_unit, left_statement),
            UnitStatementId::new(right_unit, right_statement)
        );
        assert_eq!(left_type_ref.index(), right_type_ref.index());
        assert_ne!(
            UnitTypeRefId::new(left_unit, left_type_ref),
            UnitTypeRefId::new(right_unit, right_type_ref)
        );
    }

    #[test]
    fn product_queries_use_the_unit_type_space_and_preserve_analysis_identity() {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "main.ko",
            "package app\nfun answer(): Int = 42",
        );
        let inputs = [SourceUnitInput::new("root", "app/main.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let signatures =
            collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
                .expect("signature collection succeeds");
        let source_unit = names.names().index().source_units()[0].id();
        let expression = file.ast().expressions().iter().next().expect("literal").0;
        let type_ref = file.ast().type_refs().iter().next().expect("return type").0;
        let int = signatures
            .types()
            .builtin(BuiltinType::Int)
            .expect("Int seed");
        let mut parts = CompilationUnitTypeParts::default();
        parts
            .expression_types
            .insert(UnitExpressionId::new(source_unit, expression), int);
        let product = CompilationUnitTypes::new(signatures, parts, Vec::new(), Vec::new());

        assert_eq!(
            product.expression_type(UnitExpressionId::new(source_unit, expression)),
            Some(int)
        );
        assert_eq!(
            product.type_ref_type(UnitTypeRefId::new(source_unit, type_ref)),
            Some(int)
        );
        assert!(product.type_ref_types().is_empty());
        assert_eq!(
            product.types().get(int),
            Some(&super::super::UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert!(product.is_compatible_with(&sources, &inputs, &names, &type_environment));
        assert!(product.is_same_analysis(&product.clone()));
        assert!(product.clone().validate().is_ok());
    }

    #[test]
    fn body_input_gate_rejects_foreign_names_environment_inputs_and_signatures() {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "main.ko",
            "package app\nfun answer(): Int = 42",
        );
        let inputs = [SourceUnitInput::new("root", "app/main.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let signatures =
            collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
                .expect("signature collection succeeds")
                .validate()
                .expect("valid signatures");

        assert!(
            validate_compilation_unit_body_inputs(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &signatures,
            )
            .is_ok()
        );

        let repeated_names = validated_names(&sources, &inputs, &name_environment);
        assert!(matches!(
            validate_compilation_unit_body_inputs(
                &sources,
                &inputs,
                &repeated_names,
                &type_environment,
                &signatures,
            ),
            Err(CompilationUnitTypeError::MismatchedSignatures)
        ));

        let unrelated_types = TypeEnvironment::new(&NameEnvironment::new());
        assert!(matches!(
            validate_compilation_unit_body_inputs(
                &sources,
                &inputs,
                &names,
                &unrelated_types,
                &signatures,
            ),
            Err(CompilationUnitTypeError::MismatchedNameEnvironment)
        ));

        let changed_inputs = [SourceUnitInput::new("root", "other/main.ko", source, &file)];
        assert!(matches!(
            validate_compilation_unit_body_inputs(
                &sources,
                &changed_inputs,
                &names,
                &type_environment,
                &signatures,
            ),
            Err(CompilationUnitTypeError::MismatchedInputs)
        ));
    }

    #[test]
    fn validated_gate_rejects_body_and_signature_errors() {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "main.ko",
            "package app\nfun answer(): Int = 42",
        );
        let inputs = [SourceUnitInput::new("root", "app/main.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let signatures =
            collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
                .expect("signature collection succeeds");
        let primary = file
            .ast()
            .expressions()
            .iter()
            .next()
            .expect("literal")
            .1
            .span();
        let diagnostic = Diagnostic::new(
            &sources,
            Severity::Error,
            codes::catalog()
                .expect("production catalog")
                .resolve(codes::TYPE_MISMATCH)
                .expect("registered code"),
            "body type mismatch",
            primary,
        )
        .expect("valid diagnostic");
        let product = CompilationUnitTypes::new(
            signatures,
            CompilationUnitTypeParts::default(),
            vec![diagnostic.clone()],
            vec![diagnostic],
        );

        assert_eq!(product.body_diagnostics().len(), 1);
        assert!(product.validate().is_err());

        let (duplicate_source, duplicate_file) = parsed(
            &mut sources,
            "duplicate.ko",
            "package duplicate\nfun same(input: Int): Int { return input }\nfun same(input: Int): String { return \"x\" }",
        );
        let duplicate_inputs = [SourceUnitInput::new(
            "root",
            "duplicate/duplicate.ko",
            duplicate_source,
            &duplicate_file,
        )];
        let duplicate_names = validated_names(&sources, &duplicate_inputs, &name_environment);
        let invalid_signatures = collect_compilation_unit_signatures(
            &sources,
            &duplicate_inputs,
            &duplicate_names,
            &type_environment,
        )
        .expect("signature errors remain a recovery product");
        assert!(invalid_signatures.clone().validate().is_err());
        let diagnostics = invalid_signatures.diagnostics().to_vec();
        assert!(
            CompilationUnitTypes::new(
                invalid_signatures,
                CompilationUnitTypeParts::default(),
                Vec::new(),
                diagnostics,
            )
            .validate()
            .is_err()
        );
    }

    #[test]
    fn body_model_does_not_reuse_the_single_file_type_id() {
        fn accepts_unit(_: UnitTypeId) {}
        fn accepts_local(_: crate::type_checking::TypeId) {}

        let _ = accepts_unit as fn(UnitTypeId);
        let _ = accepts_local as fn(crate::type_checking::TypeId);
    }
}
