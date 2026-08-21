use std::{collections::BTreeMap, sync::Arc};

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::Diagnostic,
    name_resolution::{ExternalSymbolId, ExternalSymbolKind, NameEnvironment, SymbolId},
    source::SourceId,
};

use super::TypeCheckingError;

/// v0.22 基础阶段识别的内建类型身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuiltinType {
    /// 8-bit signed integer.
    Byte,
    /// 16-bit signed integer.
    Short,
    /// 32-bit signed integer.
    Int,
    /// 64-bit signed integer.
    Long,
    /// 8-bit unsigned integer.
    UByte,
    /// 16-bit unsigned integer.
    UShort,
    /// 32-bit unsigned integer.
    UInt,
    /// 64-bit unsigned integer.
    ULong,
    /// 32-bit IEEE-754 floating point.
    Float,
    /// 64-bit IEEE-754 floating point.
    Double,
    /// Boolean value.
    Boolean,
    /// Unicode scalar value.
    Char,
    /// String value.
    String,
    /// Unit value.
    Unit,
    /// Bottom type.
    Nothing,
    /// Top value type whose representation remains deferred.
    Any,
}

impl BuiltinType {
    /// 返回规范源码名称。
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Byte => "Byte",
            Self::Short => "Short",
            Self::Int => "Int",
            Self::Long => "Long",
            Self::UByte => "UByte",
            Self::UShort => "UShort",
            Self::UInt => "UInt",
            Self::ULong => "ULong",
            Self::Float => "Float",
            Self::Double => "Double",
            Self::Boolean => "Boolean",
            Self::Char => "Char",
            Self::String => "String",
            Self::Unit => "Unit",
            Self::Nothing => "Nothing",
            Self::Any => "Any",
        }
    }
}

/// callable 参数的类型级模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParameterMode {
    /// Passed by value.
    Value,
    /// Shared borrow.
    Borrow,
    /// Exclusive inout borrow.
    Inout,
}

/// 外部签名使用的递归类型描述；进入 typed 产物后会被规范化为 [`TypeId`]。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvironmentType {
    /// Builtin scalar or special type.
    Builtin(BuiltinType),
    /// Nullable wrapper.
    Nullable(Box<EnvironmentType>),
    /// Function type.
    Function {
        /// Whether the callable owns all captures.
        move_only: bool,
        /// Ordered parameter types.
        parameters: Vec<EnvironmentParameter>,
        /// Return type.
        return_type: Box<EnvironmentType>,
    },
}

/// 外部函数类型中的一个参数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentParameter {
    /// Parameter passing mode.
    pub mode: ParameterMode,
    /// Parameter value type.
    pub ty: EnvironmentType,
}

/// 外部具名函数的单态签名。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentFunction {
    /// Ordered parameters.
    pub parameters: Vec<EnvironmentParameter>,
    /// Declared return type.
    pub return_type: EnvironmentType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExternalTypeBinding {
    Builtin(BuiltinType),
    Value(EnvironmentType),
    Function(EnvironmentFunction),
}

/// 与一个显式 [`NameEnvironment`] 绑定的不可变类型环境构建器。
#[derive(Clone, Debug)]
pub struct TypeEnvironment {
    owner: Arc<()>,
    symbol_kinds: Vec<ExternalSymbolKind>,
    bindings: BTreeMap<ExternalSymbolId, ExternalTypeBinding>,
}

impl TypeEnvironment {
    /// 创建与给定名称环境共享身份的空类型环境。
    #[must_use]
    pub fn new(names: &NameEnvironment) -> Self {
        Self {
            owner: names.owner(),
            symbol_kinds: names.symbols().iter().map(|symbol| symbol.kind()).collect(),
            bindings: BTreeMap::new(),
        }
    }

    /// 把外部 type symbol 绑定为内建类型身份。
    pub fn bind_builtin(
        &mut self,
        symbol: ExternalSymbolId,
        builtin: BuiltinType,
    ) -> Result<(), TypeCheckingError> {
        self.bind(
            symbol,
            ExternalTypeBinding::Builtin(builtin),
            ExternalSymbolKind::Type,
        )
    }

    /// 为外部 value symbol 提供单态类型。
    pub fn bind_value(
        &mut self,
        symbol: ExternalSymbolId,
        ty: EnvironmentType,
    ) -> Result<(), TypeCheckingError> {
        self.bind(
            symbol,
            ExternalTypeBinding::Value(ty),
            ExternalSymbolKind::Value,
        )
    }

    /// 为外部 function symbol 提供一个单态签名。
    pub fn bind_function(
        &mut self,
        symbol: ExternalSymbolId,
        signature: EnvironmentFunction,
    ) -> Result<(), TypeCheckingError> {
        self.bind(
            symbol,
            ExternalTypeBinding::Function(signature),
            ExternalSymbolKind::Function,
        )
    }

    fn bind(
        &mut self,
        symbol: ExternalSymbolId,
        binding: ExternalTypeBinding,
        expected: ExternalSymbolKind,
    ) -> Result<(), TypeCheckingError> {
        if self.symbol_kinds.get(symbol.index()).copied() != Some(expected)
            || self.bindings.contains_key(&symbol)
        {
            return Err(TypeCheckingError::InvalidExternalBinding);
        }
        self.bindings.insert(symbol, binding);
        Ok(())
    }

    pub(crate) fn owner(&self) -> &Arc<()> {
        &self.owner
    }

    pub(crate) fn binding(&self, id: ExternalSymbolId) -> Option<&ExternalTypeBinding> {
        self.bindings.get(&id)
    }
}

/// 一次 typed 产物内稳定的规范化类型身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(usize);

impl TypeId {
    /// 返回 typed 产物类型表中的稳定下标。
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// 后续阶段必须精确承接的 deferred 原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeferredReason {
    /// `Any` representation awaits the later nominal model.
    AnyValueRepresentation,
    /// Nominal or type-parameter identity is not in this Spec.
    NominalOrTypeParameter,
    /// Qualified type lookup awaits member/type resolution.
    QualifiedType,
    /// Function structure contains a deferred component.
    FunctionContainsDeferred,
    /// External symbol has no type binding.
    UnboundExternalType,
    /// Value type depends on a later declaration.
    ForwardValueType,
    /// An overload set needs candidate selection.
    OverloadSelection,
    /// `this` needs classifier typing.
    ThisType,
    /// Member lookup is deferred.
    MemberAccess,
    /// Call resolution is deferred.
    Call,
    /// Index contract resolution is deferred.
    Index,
    /// Assignment/place checking is deferred.
    Assignment,
    /// Cast or type-test semantics are deferred.
    CastOrTypeTest,
    /// Result propagation is deferred.
    ErrorPropagation,
    /// `when` typing and exhaustiveness are deferred.
    WhenTyping,
    /// A control-flow join depends on later typing.
    ControlJoin,
    /// Destructuring component typing is deferred.
    Destructuring,
    /// Iteration source typing is deferred.
    LoopSource,
}

/// 整数字面量在默认或 expected-type 定型前的约束族。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntegerConstraint {
    /// Unsuffixed signed literal family.
    Signed,
    /// Unsuffixed unsigned literal family.
    Unsigned,
}

/// 函数类型中的规范化参数。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionParameterType {
    /// Parameter passing mode.
    pub mode: ParameterMode,
    /// Parameter type identity.
    pub ty: TypeId,
}

/// 类型表中的规范化结构。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeKind {
    /// Builtin scalar or special type.
    Builtin(BuiltinType),
    /// Nullable wrapper.
    Nullable(TypeId),
    /// Function type.
    Function {
        /// Whether the callable owns all captures.
        move_only: bool,
        /// Ordered parameters.
        parameters: Vec<FunctionParameterType>,
        /// Return type.
        return_type: TypeId,
    },
    /// Integer literal constraint before contextual selection.
    IntegerLiteral(IntegerConstraint),
    /// Error recovery type.
    Error,
    /// Precisely classified work for a later Spec.
    Deferred(DeferredReason),
}

/// 插入顺序稳定并按结构去重的类型表。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeTable {
    kinds: Vec<TypeKind>,
    ids: BTreeMap<TypeKind, TypeId>,
}

impl TypeTable {
    pub(crate) fn new() -> Self {
        let mut table = Self {
            kinds: Vec::new(),
            ids: BTreeMap::new(),
        };
        for builtin in [
            BuiltinType::Byte,
            BuiltinType::Short,
            BuiltinType::Int,
            BuiltinType::Long,
            BuiltinType::UByte,
            BuiltinType::UShort,
            BuiltinType::UInt,
            BuiltinType::ULong,
            BuiltinType::Float,
            BuiltinType::Double,
            BuiltinType::Boolean,
            BuiltinType::Char,
            BuiltinType::String,
            BuiltinType::Unit,
            BuiltinType::Nothing,
            BuiltinType::Any,
        ] {
            table.intern(TypeKind::Builtin(builtin));
        }
        table.intern(TypeKind::IntegerLiteral(IntegerConstraint::Signed));
        table.intern(TypeKind::IntegerLiteral(IntegerConstraint::Unsigned));
        table.intern(TypeKind::Error);
        table
    }

    pub(crate) fn intern(&mut self, kind: TypeKind) -> TypeId {
        if let Some(id) = self.ids.get(&kind).copied() {
            return id;
        }
        let id = TypeId(self.kinds.len());
        self.kinds.push(kind.clone());
        self.ids.insert(kind, id);
        id
    }

    /// 按身份读取规范化结构。
    #[must_use]
    pub fn get(&self, id: TypeId) -> Option<&TypeKind> {
        self.kinds.get(id.index())
    }

    /// 返回类型表大小。
    #[must_use]
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    /// 返回类型表是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}

/// SPEC-0019 的单文件 typed 产物。
#[derive(Clone, Debug)]
pub struct TypedFile {
    source_id: SourceId,
    types: TypeTable,
    expression_types: Vec<TypeId>,
    type_ref_types: Vec<TypeId>,
    symbol_types: Vec<TypeId>,
    diagnostics: Vec<Diagnostic>,
}

impl TypedFile {
    pub(crate) fn new(
        source_id: SourceId,
        types: TypeTable,
        expression_types: Vec<TypeId>,
        type_ref_types: Vec<TypeId>,
        symbol_types: Vec<TypeId>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            source_id,
            types,
            expression_types,
            type_ref_types,
            symbol_types,
            diagnostics,
        }
    }

    #[must_use]
    /// 返回来源文件身份。
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    #[must_use]
    /// 返回本产物的规范化类型表。
    pub const fn types(&self) -> &TypeTable {
        &self.types
    }

    #[must_use]
    /// 查询表达式类型。
    pub fn expression_type(&self, id: ExpressionId) -> Option<TypeId> {
        self.expression_types.get(id.index()).copied()
    }

    #[must_use]
    /// 查询 TypeRef 类型。
    pub fn type_ref_type(&self, id: TypeRefId) -> Option<TypeId> {
        self.type_ref_types.get(id.index()).copied()
    }

    #[must_use]
    /// 查询源码 symbol 类型。
    pub fn symbol_type(&self, id: SymbolId) -> Option<TypeId> {
        self.symbol_types.get(id.index()).copied()
    }

    #[must_use]
    /// 返回稳定排序的类型诊断。
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
