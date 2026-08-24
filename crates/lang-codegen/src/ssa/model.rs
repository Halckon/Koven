use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::source::Span;

static NEXT_PROGRAM_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ProgramOwner(u64);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ModuleId {
    owner: ProgramOwner,
    index: usize,
}

impl ModuleId {
    pub(crate) const fn index(self) -> usize {
        self.index
    }
}

impl fmt::Debug for ModuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Program owner identity only prevents cross-program ID mixing and is not stable output.
        formatter
            .debug_tuple("ModuleId")
            .field(&self.index)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FunctionId {
    pub(crate) module: ModuleId,
    pub(crate) index: usize,
}

impl FunctionId {
    pub(crate) const fn module(self) -> ModuleId {
        self.module
    }

    pub(crate) const fn index(self) -> usize {
        self.index
    }
}

macro_rules! function_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub(crate) struct $name {
            pub(crate) function: FunctionId,
            pub(crate) index: usize,
        }

        impl $name {
            pub(crate) const fn function(self) -> FunctionId {
                self.function
            }

            pub(crate) const fn index(self) -> usize {
                self.index
            }
        }
    };
}

function_id!(BlockId);
function_id!(InstructionId);
function_id!(ValueId);
function_id!(PlaceId);
function_id!(LoanId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SsaTypeId {
    pub(crate) module: ModuleId,
    pub(crate) index: usize,
}

impl SsaTypeId {
    pub(crate) const fn module(self) -> ModuleId {
        self.module
    }

    pub(crate) const fn index(self) -> usize {
        self.index
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Source(Span),
    Synthetic { anchor: Span, reason: String },
}

impl Origin {
    pub(crate) const fn span(&self) -> Span {
        match self {
            Self::Source(span) | Self::Synthetic { anchor: span, .. } => *span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Ownership {
    Copyable,
    MoveOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SsaTypeKind {
    Unit,
    Boolean,
    Integer {
        bits: u16,
        signed: bool,
    },
    Opaque {
        name: String,
        ownership: Ownership,
    },
    Aggregate {
        name: String,
        fields: Vec<SsaTypeId>,
        ownership: Ownership,
    },
    HeapOwner {
        name: String,
        fields: Option<Vec<SsaTypeId>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LoanKind {
    Shared,
    Exclusive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EntityType {
    Value(SsaTypeId),
    Place(SsaTypeId),
    Loan { kind: LoanKind, target: SsaTypeId },
}

impl EntityType {
    pub(crate) const fn semantic_type(self) -> SsaTypeId {
        match self {
            Self::Value(ty) | Self::Place(ty) | Self::Loan { target: ty, .. } => ty,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum EntityId {
    Value(ValueId),
    Place(PlaceId),
    Loan(LoanId),
}

impl EntityId {
    pub(crate) const fn function(self) -> FunctionId {
        match self {
            Self::Value(id) => id.function(),
            Self::Place(id) => id.function(),
            Self::Loan(id) => id.function(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Definition {
    BlockParameter {
        block: BlockId,
        index: usize,
    },
    InstructionResult {
        instruction: InstructionId,
        index: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EntityData {
    pub(crate) ty: EntityType,
    pub(crate) definition: Definition,
    pub(crate) origin: Origin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScalarConstant {
    Unit,
    Boolean(bool),
    Integer(i128),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Equal,
    LessThan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckedArithmeticOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComparisonOperator {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlaceAccess {
    Place(PlaceId),
    Loan(LoanId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Constant(ScalarConstant),
    /// SPEC-0033 的低层 primitive；算术变体只允许在范围证明后使用。
    /// 源语言 `+`/`-`/`*` 必须先 lower 为 `CheckedArithmetic`。
    Binary {
        operator: BinaryOperator,
        left: ValueId,
        right: ValueId,
    },
    /// 返回算术结果与失败标志；lowering 必须把失败标志导向 `Abort`。
    CheckedArithmetic {
        operator: CheckedArithmeticOperator,
        left: ValueId,
        right: ValueId,
    },
    Compare {
        operator: ComparisonOperator,
        left: ValueId,
        right: ValueId,
    },
    BooleanNot {
        operand: ValueId,
    },
    DirectCall {
        callee: FunctionId,
        arguments: Vec<ValueId>,
    },
    Copy {
        source: ValueId,
    },
    Consume {
        owner: ValueId,
    },
    RootPlace {
        owner: ValueId,
    },
    BorrowBegin {
        place: PlaceId,
        kind: LoanKind,
    },
    BorrowEnd {
        loan: LoanId,
    },
    Read {
        source: PlaceAccess,
    },
    Mutate {
        place: PlaceId,
        value: ValueId,
    },
    Drop {
        owner: ValueId,
    },
}

impl Operation {
    pub(crate) fn entities(&self) -> Vec<EntityId> {
        match self {
            Self::Constant(_) => Vec::new(),
            Self::Binary { left, right, .. }
            | Self::CheckedArithmetic { left, right, .. }
            | Self::Compare { left, right, .. } => {
                vec![EntityId::Value(*left), EntityId::Value(*right)]
            }
            Self::DirectCall { arguments, .. } => {
                arguments.iter().copied().map(EntityId::Value).collect()
            }
            Self::BooleanNot { operand } => vec![EntityId::Value(*operand)],
            Self::Copy { source } => vec![EntityId::Value(*source)],
            Self::Consume { owner } | Self::RootPlace { owner } | Self::Drop { owner } => {
                vec![EntityId::Value(*owner)]
            }
            Self::BorrowBegin { place, .. } => vec![EntityId::Place(*place)],
            Self::BorrowEnd { loan } => vec![EntityId::Loan(*loan)],
            Self::Read { source } => vec![match source {
                PlaceAccess::Place(place) => EntityId::Place(*place),
                PlaceAccess::Loan(loan) => EntityId::Loan(*loan),
            }],
            Self::Mutate { place, value } => {
                vec![EntityId::Place(*place), EntityId::Value(*value)]
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Instruction {
    pub(crate) id: InstructionId,
    pub(crate) block: BlockId,
    pub(crate) operation: Operation,
    pub(crate) results: Vec<EntityId>,
    pub(crate) origin: Origin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edge {
    pub(crate) target: BlockId,
    pub(crate) arguments: Vec<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TerminatorKind {
    Branch(Edge),
    Conditional {
        condition: ValueId,
        when_true: Edge,
        when_false: Edge,
    },
    Return {
        values: Vec<ValueId>,
    },
    Abort,
}

impl TerminatorKind {
    pub(crate) fn entities(&self) -> Vec<EntityId> {
        match self {
            Self::Branch(edge) => edge.arguments.clone(),
            Self::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                let mut entities = vec![EntityId::Value(*condition)];
                entities.extend(when_true.arguments.iter().copied());
                entities.extend(when_false.arguments.iter().copied());
                entities
            }
            Self::Return { values } => values.iter().copied().map(EntityId::Value).collect(),
            Self::Abort => Vec::new(),
        }
    }

    pub(crate) fn targets(&self) -> Vec<BlockId> {
        match self {
            Self::Branch(edge) => vec![edge.target],
            Self::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true.target, when_false.target],
            Self::Return { .. } | Self::Abort => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Terminator {
    pub(crate) kind: TerminatorKind,
    pub(crate) origin: Origin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Block {
    pub(crate) id: BlockId,
    pub(crate) parameters: Vec<EntityId>,
    pub(crate) instructions: Vec<InstructionId>,
    pub(crate) terminator: Option<Terminator>,
    pub(crate) origin: Origin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Function {
    pub(crate) id: FunctionId,
    pub(crate) name: String,
    pub(crate) return_types: Vec<SsaTypeId>,
    pub(crate) blocks: Vec<Block>,
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) values: Vec<EntityData>,
    pub(crate) places: Vec<EntityData>,
    pub(crate) loans: Vec<EntityData>,
    pub(crate) origin: Origin,
}

impl Function {
    pub(crate) const fn id(&self) -> FunctionId {
        self.id
    }

    pub(crate) fn entry_block(&self) -> Option<BlockId> {
        self.blocks.first().map(|block| block.id)
    }

    pub(crate) fn block(&self, id: BlockId) -> Option<&Block> {
        (id.function() == self.id)
            .then(|| self.blocks.get(id.index()))
            .flatten()
    }

    pub(crate) fn instruction(&self, id: InstructionId) -> Option<&Instruction> {
        (id.function() == self.id)
            .then(|| self.instructions.get(id.index()))
            .flatten()
    }

    pub(crate) fn entity(&self, id: EntityId) -> Option<&EntityData> {
        if id.function() != self.id {
            return None;
        }
        match id {
            EntityId::Value(id) => self.values.get(id.index()),
            EntityId::Place(id) => self.places.get(id.index()),
            EntityId::Loan(id) => self.loans.get(id.index()),
        }
    }

    pub(crate) fn add_block(
        &mut self,
        parameter_types: Vec<EntityType>,
        origin: Origin,
    ) -> Result<BlockId, ModelError> {
        for ty in &parameter_types {
            self.check_type_owner(*ty)?;
        }
        let id = BlockId {
            function: self.id,
            index: self.blocks.len(),
        };
        let parameters = parameter_types
            .into_iter()
            .enumerate()
            .map(|(index, ty)| {
                self.allocate_entity(
                    ty,
                    Definition::BlockParameter { block: id, index },
                    origin.clone(),
                )
            })
            .collect();
        self.blocks.push(Block {
            id,
            parameters,
            instructions: Vec::new(),
            terminator: None,
            origin,
        });
        Ok(id)
    }

    pub(crate) fn append_instruction(
        &mut self,
        block: BlockId,
        operation: Operation,
        result_types: Vec<EntityType>,
        origin: Origin,
    ) -> Result<(InstructionId, Vec<EntityId>), ModelError> {
        self.check_block(block)?;
        if self.blocks[block.index()].terminator.is_some() {
            return Err(ModelError::BlockAlreadyTerminated { block });
        }
        for entity in operation.entities() {
            self.check_entity(entity)?;
        }
        for ty in &result_types {
            self.check_type_owner(*ty)?;
        }
        let id = InstructionId {
            function: self.id,
            index: self.instructions.len(),
        };
        let results = result_types
            .into_iter()
            .enumerate()
            .map(|(index, ty)| {
                self.allocate_entity(
                    ty,
                    Definition::InstructionResult {
                        instruction: id,
                        index,
                    },
                    origin.clone(),
                )
            })
            .collect::<Vec<_>>();
        self.instructions.push(Instruction {
            id,
            block,
            operation,
            results: results.clone(),
            origin,
        });
        self.blocks[block.index()].instructions.push(id);
        Ok((id, results))
    }

    pub(crate) fn set_terminator(
        &mut self,
        block: BlockId,
        kind: TerminatorKind,
        origin: Origin,
    ) -> Result<(), ModelError> {
        self.check_block(block)?;
        if self.blocks[block.index()].terminator.is_some() {
            return Err(ModelError::TerminatorAlreadySet { block });
        }
        for target in kind.targets() {
            self.check_block(target)?;
        }
        for entity in kind.entities() {
            self.check_entity(entity)?;
        }
        self.blocks[block.index()].terminator = Some(Terminator { kind, origin });
        Ok(())
    }

    fn allocate_entity(
        &mut self,
        ty: EntityType,
        definition: Definition,
        origin: Origin,
    ) -> EntityId {
        let data = EntityData {
            ty,
            definition,
            origin,
        };
        match ty {
            EntityType::Value(_) => {
                let id = ValueId {
                    function: self.id,
                    index: self.values.len(),
                };
                self.values.push(data);
                EntityId::Value(id)
            }
            EntityType::Place(_) => {
                let id = PlaceId {
                    function: self.id,
                    index: self.places.len(),
                };
                self.places.push(data);
                EntityId::Place(id)
            }
            EntityType::Loan { .. } => {
                let id = LoanId {
                    function: self.id,
                    index: self.loans.len(),
                };
                self.loans.push(data);
                EntityId::Loan(id)
            }
        }
    }

    fn check_type_owner(&self, ty: EntityType) -> Result<(), ModelError> {
        let ty = ty.semantic_type();
        if ty.module() == self.id.module() {
            Ok(())
        } else {
            Err(ModelError::WrongTypeOwner {
                expected: self.id.module(),
                actual: ty.module(),
            })
        }
    }

    fn check_block(&self, block: BlockId) -> Result<(), ModelError> {
        if block.function() != self.id {
            return Err(ModelError::WrongFunctionOwner {
                expected: self.id,
                actual: block.function(),
            });
        }
        self.blocks
            .get(block.index())
            .map(|_| ())
            .ok_or(ModelError::UnknownBlock { block })
    }

    fn check_entity(&self, entity: EntityId) -> Result<(), ModelError> {
        if entity.function() != self.id {
            return Err(ModelError::WrongFunctionOwner {
                expected: self.id,
                actual: entity.function(),
            });
        }
        self.entity(entity)
            .map(|_| ())
            .ok_or(ModelError::UnknownEntity { entity })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Module {
    pub(crate) id: ModuleId,
    pub(crate) name: String,
    pub(crate) types: Vec<SsaTypeKind>,
    type_ids: BTreeMap<SsaTypeKind, SsaTypeId>,
    named_type_ids: BTreeMap<String, SsaTypeId>,
    pub(crate) functions: Vec<Function>,
}

impl Module {
    pub(crate) const fn id(&self) -> ModuleId {
        self.id
    }

    pub(crate) fn intern_type(&mut self, kind: SsaTypeKind) -> SsaTypeId {
        if let Some(id) = self.type_ids.get(&kind).copied() {
            return id;
        }
        let id = SsaTypeId {
            module: self.id,
            index: self.types.len(),
        };
        self.types.push(kind.clone());
        self.type_ids.insert(kind, id);
        id
    }

    pub(crate) fn add_aggregate_type(
        &mut self,
        name: impl Into<String>,
        fields: Vec<SsaTypeId>,
    ) -> Result<SsaTypeId, ModelError> {
        let name = name.into();
        self.check_new_type_name(&name)?;
        let ownership = self.aggregate_ownership(&fields)?;
        let id = self.push_named_type(
            name.clone(),
            SsaTypeKind::Aggregate {
                name,
                fields,
                ownership,
            },
        );
        Ok(id)
    }

    pub(crate) fn declare_heap_owner(
        &mut self,
        name: impl Into<String>,
    ) -> Result<SsaTypeId, ModelError> {
        let name = name.into();
        self.check_new_type_name(&name)?;
        Ok(self.push_named_type(name.clone(), SsaTypeKind::HeapOwner { name, fields: None }))
    }

    pub(crate) fn define_heap_owner(
        &mut self,
        id: SsaTypeId,
        fields: Vec<SsaTypeId>,
    ) -> Result<(), ModelError> {
        if id.module() != self.id {
            return Err(ModelError::WrongTypeOwner {
                expected: self.id,
                actual: id.module(),
            });
        }
        for field in &fields {
            self.check_type_id(*field)?;
        }
        let kind = self
            .types
            .get_mut(id.index())
            .ok_or(ModelError::UnknownType { ty: id })?;
        let SsaTypeKind::HeapOwner {
            fields: definition, ..
        } = kind
        else {
            return Err(ModelError::ExpectedHeapOwner { ty: id });
        };
        if definition.is_some() {
            return Err(ModelError::TypeAlreadyDefined { ty: id });
        }
        *definition = Some(fields);
        Ok(())
    }

    pub(crate) fn type_ownership(&self, id: SsaTypeId) -> Option<Ownership> {
        match self.type_kind(id)? {
            SsaTypeKind::Unit | SsaTypeKind::Boolean | SsaTypeKind::Integer { .. } => {
                Some(Ownership::Copyable)
            }
            SsaTypeKind::Opaque { ownership, .. } | SsaTypeKind::Aggregate { ownership, .. } => {
                Some(*ownership)
            }
            SsaTypeKind::HeapOwner { .. } => Some(Ownership::MoveOnly),
        }
    }

    pub(crate) fn type_is_defined(&self, id: SsaTypeId) -> bool {
        match self.type_kind(id) {
            Some(SsaTypeKind::HeapOwner { fields, .. }) => fields.is_some(),
            Some(_) => true,
            None => false,
        }
    }

    pub(crate) fn type_kind(&self, id: SsaTypeId) -> Option<&SsaTypeKind> {
        (id.module() == self.id)
            .then(|| self.types.get(id.index()))
            .flatten()
    }

    fn aggregate_ownership(&self, fields: &[SsaTypeId]) -> Result<Ownership, ModelError> {
        let mut ownership = Ownership::Copyable;
        for field in fields {
            self.check_type_id(*field)?;
            if self.type_ownership(*field) == Some(Ownership::MoveOnly) {
                ownership = Ownership::MoveOnly;
            }
        }
        Ok(ownership)
    }

    fn check_new_type_name(&self, name: &str) -> Result<(), ModelError> {
        if name.is_empty() {
            return Err(ModelError::EmptyTypeName);
        }
        if self.named_type_ids.contains_key(name) {
            return Err(ModelError::DuplicateTypeName {
                name: name.to_owned(),
            });
        }
        Ok(())
    }

    fn push_named_type(&mut self, name: String, kind: SsaTypeKind) -> SsaTypeId {
        let id = SsaTypeId {
            module: self.id,
            index: self.types.len(),
        };
        self.types.push(kind);
        self.named_type_ids.insert(name, id);
        id
    }

    fn check_type_id(&self, ty: SsaTypeId) -> Result<(), ModelError> {
        if ty.module() != self.id {
            return Err(ModelError::WrongTypeOwner {
                expected: self.id,
                actual: ty.module(),
            });
        }
        self.type_kind(ty)
            .map(|_| ())
            .ok_or(ModelError::UnknownType { ty })
    }

    pub(crate) fn add_function(
        &mut self,
        name: impl Into<String>,
        return_types: Vec<SsaTypeId>,
        origin: Origin,
    ) -> Result<FunctionId, ModelError> {
        for ty in &return_types {
            if ty.module() != self.id {
                return Err(ModelError::WrongTypeOwner {
                    expected: self.id,
                    actual: ty.module(),
                });
            }
            if self.type_kind(*ty).is_none() {
                return Err(ModelError::UnknownType { ty: *ty });
            }
        }
        let id = FunctionId {
            module: self.id,
            index: self.functions.len(),
        };
        self.functions.push(Function {
            id,
            name: name.into(),
            return_types,
            blocks: Vec::new(),
            instructions: Vec::new(),
            values: Vec::new(),
            places: Vec::new(),
            loans: Vec::new(),
            origin,
        });
        Ok(id)
    }

    pub(crate) fn function(&self, id: FunctionId) -> Option<&Function> {
        (id.module() == self.id)
            .then(|| self.functions.get(id.index()))
            .flatten()
    }

    pub(crate) fn function_mut(&mut self, id: FunctionId) -> Option<&mut Function> {
        (id.module() == self.id)
            .then(|| self.functions.get_mut(id.index()))
            .flatten()
    }
}

pub(crate) struct Program {
    owner: ProgramOwner,
    pub(crate) modules: Vec<Module>,
}

impl Default for Program {
    fn default() -> Self {
        Self {
            owner: ProgramOwner(NEXT_PROGRAM_OWNER.fetch_add(1, Ordering::Relaxed)),
            modules: Vec::new(),
        }
    }
}

impl Program {
    pub(crate) fn add_module(&mut self, name: impl Into<String>) -> ModuleId {
        let id = ModuleId {
            owner: self.owner,
            index: self.modules.len(),
        };
        self.modules.push(Module {
            id,
            name: name.into(),
            types: Vec::new(),
            type_ids: BTreeMap::new(),
            named_type_ids: BTreeMap::new(),
            functions: Vec::new(),
        });
        id
    }

    pub(crate) fn module(&self, id: ModuleId) -> Option<&Module> {
        (id.owner == self.owner)
            .then(|| self.modules.get(id.index()))
            .flatten()
    }

    pub(crate) fn module_mut(&mut self, id: ModuleId) -> Option<&mut Module> {
        (id.owner == self.owner)
            .then(|| self.modules.get_mut(id.index()))
            .flatten()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModelError {
    WrongTypeOwner {
        expected: ModuleId,
        actual: ModuleId,
    },
    WrongFunctionOwner {
        expected: FunctionId,
        actual: FunctionId,
    },
    UnknownBlock {
        block: BlockId,
    },
    UnknownType {
        ty: SsaTypeId,
    },
    EmptyTypeName,
    DuplicateTypeName {
        name: String,
    },
    ExpectedHeapOwner {
        ty: SsaTypeId,
    },
    TypeAlreadyDefined {
        ty: SsaTypeId,
    },
    UnknownEntity {
        entity: EntityId,
    },
    BlockAlreadyTerminated {
        block: BlockId,
    },
    TerminatorAlreadySet {
        block: BlockId,
    },
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongTypeOwner { expected, actual } => {
                write!(
                    formatter,
                    "type belongs to {actual:?}, expected {expected:?}"
                )
            }
            Self::WrongFunctionOwner { expected, actual } => {
                write!(
                    formatter,
                    "entity belongs to {actual:?}, expected {expected:?}"
                )
            }
            Self::UnknownBlock { block } => write!(formatter, "unknown block {block:?}"),
            Self::UnknownType { ty } => write!(formatter, "unknown type {ty:?}"),
            Self::EmptyTypeName => write!(formatter, "named SSA type must not be empty"),
            Self::DuplicateTypeName { name } => {
                write!(formatter, "duplicate named SSA type {name:?}")
            }
            Self::ExpectedHeapOwner { ty } => write!(formatter, "type {ty:?} is not a heap owner"),
            Self::TypeAlreadyDefined { ty } => write!(formatter, "type {ty:?} is already defined"),
            Self::UnknownEntity { entity } => write!(formatter, "unknown entity {entity:?}"),
            Self::BlockAlreadyTerminated { block } => {
                write!(formatter, "cannot append to terminated block {block:?}")
            }
            Self::TerminatorAlreadySet { block } => {
                write!(formatter, "block {block:?} already has a terminator")
            }
        }
    }
}

impl Error for ModelError {}
