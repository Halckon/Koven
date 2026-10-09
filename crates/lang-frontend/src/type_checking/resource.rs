//! Typed deinit contracts and resource classification; no backend lifetime inference.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::OnceLock,
};

use crate::{
    ast::{ItemId, StatementId},
    name_resolution::{DeclarationId, SymbolId, UnitSymbolId},
};

use super::{
    BuiltinType, CompilationUnitTypes, NominalId, NominalKind, ParameterMode, TypeId, TypeKind,
    TypedFile, UnitItemId, UnitStatementId, UnitTypeId, UnitTypeKind,
};

/// 一个源码 class 的隐式析构入口；不属于用户可调用的 member 集合。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeinitDescriptor {
    owner: NominalId,
    item: ItemId,
    body: StatementId,
    receiver_type: TypeId,
}

impl DeinitDescriptor {
    pub(crate) const fn new(
        owner: NominalId,
        item: ItemId,
        body: StatementId,
        receiver_type: TypeId,
    ) -> Self {
        Self {
            owner,
            item,
            body,
            receiver_type,
        }
    }

    /// 返回声明此析构入口的 nominal。
    #[must_use]
    pub const fn owner(self) -> NominalId {
        self.owner
    }

    /// 返回不可作为普通 callable 调用的 deinit 声明。
    #[must_use]
    pub const fn item(self) -> ItemId {
        self.item
    }

    /// 返回必须先于字段清理执行的用户 body。
    #[must_use]
    pub const fn body(self) -> StatementId {
        self.body
    }

    /// 返回隐藏 receiver 的声明类型模板。
    #[must_use]
    pub const fn receiver_type(self) -> TypeId {
        self.receiver_type
    }

    /// 析构 body 的 this 始终只读；不得再次消费当前 owner。
    #[must_use]
    pub const fn receiver_mode(self) -> ParameterMode {
        ParameterMode::Borrow
    }
}

/// compilation unit 中带 source-unit 身份的隐式析构入口。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitDeinitDescriptor {
    owner: DeclarationId,
    item: UnitItemId,
    body: UnitStatementId,
    receiver_type: UnitTypeId,
}

impl UnitDeinitDescriptor {
    pub(crate) const fn new(
        owner: DeclarationId,
        item: UnitItemId,
        body: UnitStatementId,
        receiver_type: UnitTypeId,
    ) -> Self {
        Self {
            owner,
            item,
            body,
            receiver_type,
        }
    }

    /// 返回声明此析构入口的 source-qualified nominal。
    #[must_use]
    pub const fn owner(self) -> DeclarationId {
        self.owner
    }

    /// 返回带 source-unit 身份的 deinit 声明。
    #[must_use]
    pub const fn item(self) -> UnitItemId {
        self.item
    }

    /// 返回带 source-unit 身份的用户 body。
    #[must_use]
    pub const fn body(self) -> UnitStatementId {
        self.body
    }

    /// 返回隐藏 receiver 的声明类型模板。
    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.receiver_type
    }

    /// 析构 body 的 this 始终只读；不得再次消费当前 owner。
    #[must_use]
    pub const fn receiver_mode(self) -> ParameterMode {
        ParameterMode::Borrow
    }
}

impl TypedFile {
    /// 查询是否持有用户析构义务，包括已替换类型实参的递归字段与 enum payload。
    ///
    /// `None` 表示未实例化类型参数、closure capture 或错误/deferred 类型尚不能证明；
    /// 消费者不得把它当成允许提前析构的纯内存证明。查询不改变类型表，也不重读 AST。
    #[must_use]
    pub fn is_resource_type(&self, ty: TypeId) -> Option<bool> {
        self.resource_classifications
            .get_or_init(|| {
                let nominals = self
                    .nominals()
                    .iter()
                    .map(|nominal| {
                        let fields = nominal
                            .fields()
                            .iter()
                            .map(|field| self.symbol_type(*field))
                            .chain(
                                self.enum_cases()
                                    .iter()
                                    .filter(|case| case.root() == nominal.id())
                                    .flat_map(|case| case.payloads())
                                    .map(|(_, ty)| Some(*ty)),
                            )
                            .collect();
                        (
                            nominal.id(),
                            ResourceNominal {
                                parameters: nominal.type_parameters().to_vec(),
                                fields,
                                resource: nominal.has_deinit(),
                                unknown: nominal.kind() == NominalKind::Interface,
                            },
                        )
                    })
                    .collect();
                ResourceClassifier::new(nominals, |ty| self.resource_kind(ty))
            })
            .classify(ty, |ty| self.resource_kind(ty))
    }

    fn resource_kind(&self, ty: TypeId) -> ResourceType<'_, TypeId, NominalId, SymbolId> {
        match self.types().get(ty) {
            Some(TypeKind::TypeParameter(parameter)) => ResourceType::Parameter(*parameter),
            Some(TypeKind::Nominal { nominal, arguments }) => {
                ResourceType::Nominal(*nominal, arguments)
            }
            Some(TypeKind::Intrinsic {
                constructor: super::IntrinsicTypeConstructor::View,
                ..
            }) => ResourceType::Pure,
            Some(TypeKind::Intrinsic { arguments, .. }) => ResourceType::Aggregate(arguments),
            Some(TypeKind::Nullable(inner) | TypeKind::EnumCase { root: inner, .. }) => {
                ResourceType::Inner(*inner)
            }
            Some(
                TypeKind::Builtin(BuiltinType::Any)
                | TypeKind::StaticSelf(_)
                | TypeKind::Capability(_)
                | TypeKind::Error
                | TypeKind::Deferred(_),
            )
            | None => ResourceType::Unknown,
            Some(TypeKind::Function {
                move_only: true, ..
            }) => ResourceType::Unknown,
            Some(
                TypeKind::Builtin(_)
                | TypeKind::IntegerLiteral(_)
                | TypeKind::Function {
                    move_only: false, ..
                },
            ) => ResourceType::Pure,
        }
    }
}

impl CompilationUnitTypes {
    /// 查询 unit-global 类型是否递归持有用户析构义务，含具体泛型实参替换。
    ///
    /// `None` 是尚未证明的类型，不是允许 ASAP 的纯内存证明；语义与单文件入口相同。
    #[must_use]
    pub fn is_resource_type(&self, ty: UnitTypeId) -> Option<bool> {
        self.resource_classifications
            .get_or_init(|| {
                let nominals = self
                    .signatures()
                    .declarations()
                    .iter()
                    .filter_map(|declaration| declaration.nominal())
                    .map(|nominal| {
                        (
                            nominal.declaration(),
                            ResourceNominal {
                                parameters: nominal.type_parameters().to_vec(),
                                fields: nominal
                                    .fields()
                                    .iter()
                                    .chain(
                                        nominal
                                            .enum_cases()
                                            .iter()
                                            .flat_map(|case| case.payloads()),
                                    )
                                    .map(|field| Some(field.ty()))
                                    .collect(),
                                resource: nominal.has_deinit(),
                                unknown: nominal.kind() == NominalKind::Interface,
                            },
                        )
                    })
                    .collect();
                ResourceClassifier::new(nominals, |ty| self.resource_kind(ty))
            })
            .classify(ty, |ty| self.resource_kind(ty))
    }

    fn resource_kind(
        &self,
        ty: UnitTypeId,
    ) -> ResourceType<'_, UnitTypeId, DeclarationId, UnitSymbolId> {
        match self.types().get(ty) {
            Some(UnitTypeKind::TypeParameter(parameter)) => ResourceType::Parameter(*parameter),
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => ResourceType::Nominal(*declaration, arguments),
            Some(UnitTypeKind::Intrinsic {
                constructor: super::IntrinsicTypeConstructor::View,
                ..
            }) => ResourceType::Pure,
            Some(UnitTypeKind::Intrinsic { arguments, .. }) => ResourceType::Aggregate(arguments),
            Some(UnitTypeKind::Nullable(inner) | UnitTypeKind::EnumCase { root: inner, .. }) => {
                ResourceType::Inner(*inner)
            }
            Some(
                UnitTypeKind::Builtin(BuiltinType::Any)
                | UnitTypeKind::StaticSelf(_)
                | UnitTypeKind::Capability(_)
                | UnitTypeKind::Error
                | UnitTypeKind::Deferred(_),
            )
            | None => ResourceType::Unknown,
            Some(UnitTypeKind::Function {
                move_only: true, ..
            }) => ResourceType::Unknown,
            Some(
                UnitTypeKind::Builtin(_)
                | UnitTypeKind::IntegerLiteral(_)
                | UnitTypeKind::Function {
                    move_only: false, ..
                },
            ) => ResourceType::Pure,
        }
    }
}

/// A declaration's storage depends only on these finite parameter identities and two facts.
/// Substitution composes dependencies, rather than creating potentially unbounded types such
/// as `Node<List<List<...>>>` for `Node<T>(next: Node<List<T>>?, item: T)`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ResourceSummary<P> {
    resource: bool,
    unknown: bool,
    parameters: BTreeSet<P>,
}

impl<P: Ord> ResourceSummary<P> {
    fn new(resource: bool, unknown: bool) -> Self {
        Self {
            resource,
            unknown,
            parameters: BTreeSet::new(),
        }
    }

    fn merge(&mut self, other: Self) {
        self.resource |= other.resource;
        self.unknown |= other.unknown;
        self.parameters.extend(other.parameters);
    }

    fn classification(&self) -> Option<bool> {
        if self.resource {
            Some(true)
        } else if self.unknown || !self.parameters.is_empty() {
            None
        } else {
            Some(false)
        }
    }
}

#[derive(Clone, Debug)]
struct ResourceNominal<T, P> {
    parameters: Vec<P>,
    fields: Vec<Option<T>>,
    resource: bool,
    unknown: bool,
}

enum ResourceType<'a, T, N, P> {
    Pure,
    Unknown,
    Parameter(P),
    Inner(T),
    Aggregate(&'a [T]),
    Nominal(N, &'a [T]),
}

/// Memoized derived facts must not affect the immutable product's equality or debug output.
#[derive(Clone)]
pub(super) struct ResourceCache<T>(OnceLock<T>);

impl<T> ResourceCache<T> {
    pub(super) const fn new() -> Self {
        Self(OnceLock::new())
    }

    fn get_or_init(&self, initialize: impl FnOnce() -> T) -> &T {
        self.0.get_or_init(initialize)
    }
}

impl<T> PartialEq for ResourceCache<T> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl<T> Eq for ResourceCache<T> {}

impl<T> fmt::Debug for ResourceCache<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ResourceCache")
    }
}

pub(super) type FileResourceClassifier = ResourceClassifier<TypeId, NominalId, SymbolId>;
pub(super) type UnitResourceClassifier =
    ResourceClassifier<UnitTypeId, DeclarationId, UnitSymbolId>;

#[derive(Clone, Debug)]
pub(super) struct ResourceClassifier<T, N, P> {
    nominals: BTreeMap<N, ResourceNominal<T, P>>,
    summaries: BTreeMap<N, ResourceSummary<P>>,
}

impl<T: Copy, N: Copy + Ord, P: Copy + Ord> ResourceClassifier<T, N, P> {
    fn new<'a>(
        nominals: BTreeMap<N, ResourceNominal<T, P>>,
        kind: impl Fn(T) -> ResourceType<'a, T, N, P>,
    ) -> Self
    where
        T: 'a,
    {
        let mut summaries = nominals
            .iter()
            .map(|(id, nominal)| (*id, ResourceSummary::new(nominal.resource, nominal.unknown)))
            .collect::<BTreeMap<_, _>>();
        // Every update adds a resource/unknown fact or a parameter dependency from the finite
        // source declarations. No generic environment expansion or recursion-depth budget is used.
        loop {
            let mut changed = false;
            for (id, nominal) in &nominals {
                let mut next = ResourceSummary::new(nominal.resource, nominal.unknown);
                for field in &nominal.fields {
                    next.merge(field.map_or_else(
                        || ResourceSummary::new(false, true),
                        |ty| summarize_resource(ty, &nominals, &summaries, &kind),
                    ));
                }
                if summaries.get(id) != Some(&next) {
                    summaries.insert(*id, next);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Self {
            nominals,
            summaries,
        }
    }

    fn classify<'a>(&self, ty: T, kind: impl Fn(T) -> ResourceType<'a, T, N, P>) -> Option<bool>
    where
        T: 'a,
    {
        summarize_resource(ty, &self.nominals, &self.summaries, &kind).classification()
    }
}

fn summarize_resource<'a, T: Copy + 'a, N: Copy + Ord, P: Copy + Ord>(
    ty: T,
    nominals: &BTreeMap<N, ResourceNominal<T, P>>,
    summaries: &BTreeMap<N, ResourceSummary<P>>,
    kind: &impl Fn(T) -> ResourceType<'a, T, N, P>,
) -> ResourceSummary<P> {
    match kind(ty) {
        ResourceType::Pure => ResourceSummary::new(false, false),
        ResourceType::Unknown => ResourceSummary::new(false, true),
        ResourceType::Parameter(parameter) => {
            let mut result = ResourceSummary::new(false, false);
            result.parameters.insert(parameter);
            result
        }
        ResourceType::Inner(inner) => summarize_resource(inner, nominals, summaries, kind),
        ResourceType::Aggregate(arguments) => {
            let mut result = ResourceSummary::new(false, false);
            for ty in arguments {
                result.merge(summarize_resource(*ty, nominals, summaries, kind));
            }
            result
        }
        ResourceType::Nominal(id, arguments) => {
            let (Some(nominal), Some(summary)) = (nominals.get(&id), summaries.get(&id)) else {
                return ResourceSummary::new(false, true);
            };
            let mut result = ResourceSummary::new(summary.resource, summary.unknown);
            for parameter in &summary.parameters {
                let argument = nominal
                    .parameters
                    .iter()
                    .position(|formal| formal == parameter)
                    .and_then(|index| arguments.get(index));
                result.merge(argument.map_or_else(
                    || ResourceSummary::new(false, true),
                    |ty| summarize_resource(*ty, nominals, summaries, kind),
                ));
            }
            result
        }
    }
}
