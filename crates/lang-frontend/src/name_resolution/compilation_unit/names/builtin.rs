//! Guide 02 内建环境：宿主授权的标准源提供 `Result`、`Ok`、`Err`。
//!
//! 内建名称是最低优先级的文件绑定：词法、当前文件、同 package、exact 与 wildcard import
//! 都未决定时才使用；用户任一 package 的同名顶层声明报告 L0150。
use super::*;

const RESULT: &str = "Result";
const OK: &str = "Ok";
const ERR: &str = "Err";

/// 一个内建 enum case 的值与类型身份。
#[derive(Clone, Copy, Debug)]
struct BuiltinCase {
    value: UnitSymbolId,
    type_symbol: UnitSymbolId,
}

/// 授权标准源中 `Result` 及其 case 的 unit 身份。
#[derive(Clone, Copy, Debug)]
pub(super) struct BuiltinResult {
    source_unit: SourceUnitId,
    declaration: DeclarationId,
    ok: BuiltinCase,
    err: BuiltinCase,
}

impl BuiltinResult {
    /// 只按宿主授权的 SourceId 查找；授权存在但导出缺失属于内部输入错误。
    pub(super) fn locate(
        environment: &NameEnvironment,
        index: &CompilationUnitIndex,
        local: &[NameResolution],
        declaration_symbols: &BTreeMap<DeclarationId, UnitSymbolId>,
    ) -> Result<Option<Self>, CompilationUnitNameError> {
        let Some(source) = environment.builtin_source() else {
            return Ok(None);
        };
        let missing = || CompilationUnitNameError::MissingBuiltinExport(RESULT);
        let source_unit = index
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == source)
            .ok_or_else(missing)?
            .id();
        let declaration = index
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.source_unit() == source_unit
                    && declaration.namespace() == Namespace::Type
                    && declaration.name() == RESULT
            })
            .ok_or_else(missing)?
            .id();
        let root = declaration_symbols
            .get(&declaration)
            .copied()
            .ok_or_else(missing)?;
        let resolution = local.get(source_unit.index()).ok_or_else(missing)?;
        let case = |name: &'static str| {
            resolution
                .enum_cases()
                .iter()
                .find(|case| {
                    case.root() == root.symbol()
                        && resolution
                            .symbols()
                            .get(case.value_symbol().index())
                            .is_some_and(|symbol| symbol.name() == name)
                })
                .map(|case| BuiltinCase {
                    value: UnitSymbolId::new(source_unit, case.value_symbol()),
                    type_symbol: UnitSymbolId::new(source_unit, case.type_symbol()),
                })
                .ok_or(CompilationUnitNameError::MissingBuiltinExport(name))
        };
        Ok(Some(Self {
            source_unit,
            declaration,
            ok: case(OK)?,
            err: case(ERR)?,
        }))
    }

    pub(super) const fn declaration(&self) -> DeclarationId {
        self.declaration
    }

    /// `Result` 只在类型命名空间；`Ok`/`Err` 在值空间是构造 case，在类型空间是 case type。
    fn target(&self, namespace: Namespace, name: &str) -> Option<UnitReferenceTarget> {
        let case = match name {
            RESULT if namespace == Namespace::Type => {
                return Some(UnitReferenceTarget::Declaration(self.declaration));
            }
            OK => self.ok,
            ERR => self.err,
            _ => return None,
        };
        Some(UnitReferenceTarget::Symbol(match namespace {
            Namespace::Type => case.type_symbol,
            Namespace::Value => case.value,
        }))
    }
}

impl UnitResolver<'_> {
    /// 所有文件绑定都未决定该命名空间时的最低优先级内建目标。
    pub(super) fn builtin_target(
        &self,
        namespace: Namespace,
        name: &str,
    ) -> Option<UnitReferenceTarget> {
        self.builtin?.target(namespace, name)
    }

    /// 相对静态路径首段（如 `Result.Ok`）只接受内建类型 `Result`。
    pub(super) fn builtin_root(&self, name: &str) -> Option<DeclarationBinding> {
        let builtin = self.builtin?;
        (name == RESULT).then(|| DeclarationBinding(vec![builtin.declaration]))
    }

    /// 用户任一 package 的顶层 `Result`（类型）或 `Ok`/`Err`（任一命名空间）与内建冲突。
    pub(super) fn report_builtin_conflicts(&mut self) -> Result<(), CompilationUnitNameError> {
        let Some(builtin) = self.builtin else {
            return Ok(());
        };
        let builtin_span = self.index.declarations()[builtin.declaration.index()].name_span();
        let mut seen = BTreeSet::new();
        let conflicts = self
            .index
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.source_unit() != builtin.source_unit
                    && match declaration.name() {
                        RESULT => declaration.namespace() == Namespace::Type,
                        OK | ERR => true,
                        _ => false,
                    }
            })
            .filter(|declaration| seen.insert(span_key(declaration.name_span())))
            .map(|declaration| (declaration.name().to_owned(), declaration.name_span()))
            .collect::<Vec<_>>();
        for (name, span) in conflicts {
            let message = format!(
                "top-level declaration `{name}` conflicts with the built-in environment name"
            );
            let mut diagnostic =
                self.new_diagnostic(codes::EXACT_IMPORT_BINDING_CONFLICT, &message, span)?;
            diagnostic
                .add_label(
                    self.sources,
                    builtin_span,
                    "built-in `Result` is declared here",
                )
                .map_err(CompilationUnitNameError::from)?;
            self.diagnostics.push(diagnostic);
        }
        Ok(())
    }
}
