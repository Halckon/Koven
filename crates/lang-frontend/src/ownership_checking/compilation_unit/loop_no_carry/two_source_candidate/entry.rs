//! Private-construction source entry evidence; not normal ownership or backend authority.
mod body;
mod different_pair;
mod different_shared_mixed_pair;
mod different_shared_pair;
mod late_pair;
mod shared_pair;
mod staggered_pair;
use super::*;
use crate::{
    name_resolution::{DeclarationId, SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, UnitCallArgumentOwnershipContract, UnitCallArgumentOwnershipKind,
        UnitDirectCallLoanEndFact, UnitDropFact, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
    },
    parser::{
        Expression, FunctionBody, FunctionForm, Item, LiteralKind, NameMarker, Statement,
        StringPart, VariableKind,
    },
    source::Span,
    type_checking::{BuiltinType, UnitTypeKind, ValidatedCompilationUnitTypes},
};
/// A source declaration initializer role; descriptors remain privately constructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryInitialization {
    /// Original named String owner, identified by its resolved symbol.
    Named(UnitSymbolId),
    /// Original Int literal initializer.
    Rounds,
    /// Original Boolean literal initializer.
    Choose,
}
/// Opaque entry evidence for one call or an exact proven two-call lifecycle, including late formation.
#[derive(Clone, Debug)]
pub struct EntryDescriptor {
    identity: Rc<()>,
    entry: DeclarationId,
    body: Span,
    calls: EntryCalls,
}
#[derive(Clone, Debug)]
enum EntryCalls {
    Single(Box<EntryCallDescriptor>),
    Pair(Box<[EntryCallDescriptor; 2]>),
    LateInitializedPair(Box<[EntryCallDescriptor; 2]>),
    DifferentTemporaryPair(Box<[EntryCallDescriptor; 2]>),
    StaggeredNamedPair {
        owners: Box<[EntryOwnerDescriptor; 2]>,
        calls: Box<[EntryCallDescriptor; 2]>,
    },
    DifferentSharedMixedPair {
        owner: Box<EntryOwnerDescriptor>,
        calls: Box<[EntryCallDescriptor; 2]>,
    },
    DifferentSharedNamedPair {
        owner: Box<EntryOwnerDescriptor>,
        calls: Box<[EntryCallDescriptor; 2]>,
    },
    SharedNamedPair {
        owner: Box<EntryOwnerDescriptor>,
        calls: Box<[EntryCallDescriptor; 2]>,
    },
}
#[derive(Clone, Debug)]
/// Read-only root-owned initialization and its original final-use Drop.
pub struct EntryOwnerDescriptor {
    root: UnitSymbolId,
    initializer: UnitExpressionId,
    span: Span,
    bytes: Vec<u8>,
    drop: UnitDropFact,
}
impl EntryOwnerDescriptor {
    /// Original resolved named root.
    pub fn root(&self) -> UnitSymbolId {
        self.root
    }
    /// Original initializer expression.
    pub fn initializer(&self) -> UnitExpressionId {
        self.initializer
    }
    /// Original initializer span.
    pub fn span(&self) -> Span {
        self.span
    }
    /// Original decoded String bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Original final-use Drop, not a synthetic event.
    pub fn drop_fact(&self) -> &UnitDropFact {
        &self.drop
    }
}
/// Read-only call occurrence evidence; not an independently owned entry capability.
#[derive(Clone, Debug)]
pub struct EntryCallDescriptor {
    instance: UnitCallableInstanceKey,
    parameter_roles: [usize; 4],
    initializers: [UnitExpressionId; 2],
    temporary_scalars: [Option<UnitExpressionId>; 2],
    initialization_order: Vec<EntryInitialization>,
    contracts: [UnitCallArgumentOwnershipContract; 4],
    ends: [UnitDirectCallLoanEndFact; 4],
    storage: StringStorage,
    rounds: i32,
    choose: bool,
    initializer_spans: [Span; 2],
}
#[derive(Clone, Debug)]
enum StringStorage {
    StaggeredRoots {
        roots: [UnitSymbolId; 2],
    },
    SharedMixed {
        root: UnitSymbolId,
        temporary_index: usize,
        temporary: Vec<u8>,
        drop: UnitDropFact,
    },
    SharedRoot {
        root: UnitSymbolId,
    },
    Temporary {
        strings: [Vec<u8>; 2],
        drops: [UnitDropFact; 2],
    },
    Mixed {
        owner: (UnitSymbolId, UnitExpressionId, Span, Vec<u8>),
        temporary_index: usize,
        temporary: Vec<u8>,
        drops: [UnitDropFact; 2],
    },
    DistinctNamed {
        owners: [(UnitSymbolId, UnitExpressionId, Span, Vec<u8>); 2],
        drops: [UnitDropFact; 2],
    },
    Named {
        root: UnitSymbolId,
        initializer: UnitExpressionId,
        span: Span,
        bytes: Vec<u8>,
        drop: UnitDropFact,
    },
}
impl EntryCallDescriptor {
    /// Semantic role at this source argument position, through the original parameter mapping.
    pub fn argument_role(&self, index: usize) -> Option<usize> {
        let parameter = self.contracts.get(index)?.parameter_index();
        self.parameter_roles
            .iter()
            .position(|position| *position == parameter)
    }
    /// Original direct scalar argument identity, already matched to its real End.
    pub fn temporary_scalar_argument(&self, index: usize) -> Option<UnitExpressionId> {
        let role = self.argument_role(index)?.checked_sub(2)?;
        let argument = self.contracts.get(index)?.argument();
        self.temporary_scalars
            .get(role)
            .copied()
            .flatten()
            .filter(|id| *id == argument)
    }
    /// Original source declaration order, independent of scalar argument roles.
    pub fn initialization_order(&self) -> &[EntryInitialization] {
        &self.initialization_order
    }
    /// Temporary-only String payloads in left/right role order, not signature order.
    pub fn strings(&self) -> Option<&[Vec<u8>; 2]> {
        match &self.storage {
            StringStorage::Temporary { strings, .. } => Some(strings),
            _ => None,
        }
    }
    /// Original bytes for the temporary at this argument position, including mixed entries.
    pub fn temporary_string(&self, index: usize) -> Option<&[u8]> {
        match &self.storage {
            StringStorage::Temporary { strings, .. } => {
                strings.get(self.argument_role(index)?).map(Vec::as_slice)
            }
            StringStorage::Mixed {
                temporary_index,
                temporary,
                ..
            }
            | StringStorage::SharedMixed {
                temporary_index,
                temporary,
                ..
            } if *temporary_index == index => Some(temporary),
            _ => None,
        }
    }
    /// A single real named owner shared by the two distinct String argument loans.
    pub fn named_string(&self) -> Option<(UnitSymbolId, UnitExpressionId, Span, &[u8])> {
        match &self.storage {
            StringStorage::Named {
                root,
                initializer,
                span,
                bytes,
                ..
            } => Some((*root, *initializer, *span, bytes)),
            _ => None,
        }
    }
    /// Named owners in source declaration order; never coalesced by equal bytes.
    pub fn named_strings(&self) -> Vec<(UnitSymbolId, UnitExpressionId, Span, &[u8])> {
        match &self.storage {
            StringStorage::Mixed {
                owner: (r, i, s, b),
                ..
            } => vec![(*r, *i, *s, b.as_slice())],
            StringStorage::DistinctNamed { owners, .. } => owners
                .iter()
                .map(|(r, i, s, b)| (*r, *i, *s, b.as_slice()))
                .collect(),
            _ => self.named_string().into_iter().collect(),
        }
    }
    /// Root of the original argument's real ended loan, not its positional name.
    pub fn named_argument_root(&self, index: usize) -> Option<UnitSymbolId> {
        if self.argument_role(index)? >= 2
            || (self.named_strings().is_empty()
                && !matches!(
                    self.storage,
                    StringStorage::SharedRoot { .. }
                        | StringStorage::SharedMixed { .. }
                        | StringStorage::StaggeredRoots { .. }
                ))
        {
            return None;
        }
        let argument = self.contracts.get(index)?.argument();
        let end = self.ends.iter().find(|e| e.loan().argument() == argument)?;
        match end.loan().target() {
            UnitLoanTarget::Place(place) if place.is_root() => {
                if let StringStorage::SharedRoot { root } | StringStorage::SharedMixed { root, .. } =
                    self.storage
                    && place.root() != root
                {
                    return None;
                }
                if let StringStorage::StaggeredRoots { roots } = self.storage
                    && !roots.contains(&place.root())
                {
                    return None;
                }
                Some(place.root())
            }
            _ => None,
        }
    }
    /// Original analyzed nonnegative Int literal.
    pub fn rounds(&self) -> i32 {
        self.rounds
    }
    /// Original analyzed Boolean.
    pub fn choose(&self) -> bool {
        self.choose
    }
    /// Original initializer spans.
    pub fn initializer_spans(&self) -> [Span; 2] {
        self.initializer_spans
    }

    /// Exact called instance.
    pub fn instance(&self) -> &UnitCallableInstanceKey {
        &self.instance
    }
    /// Original Int and Boolean value expressions: local initializers or direct arguments.
    pub fn initializers(&self) -> &[UnitExpressionId; 2] {
        &self.initializers
    }
    /// Actual argument contracts in source and parameter order.
    pub fn contracts(&self) -> &[UnitCallArgumentOwnershipContract; 4] {
        &self.contracts
    }
    /// Actual End events in traversal order, not inferred from descriptors.
    pub fn ends(&self) -> &[UnitDirectCallLoanEndFact; 4] {
        &self.ends
    }
    /// Actual owner drops in traversal order (two temporaries or one/two named owners).
    pub fn drops(&self) -> &[UnitDropFact] {
        match &self.storage {
            StringStorage::SharedRoot { .. } | StringStorage::StaggeredRoots { .. } => &[],
            StringStorage::Temporary { drops, .. } => drops,
            StringStorage::Named { drop, .. } | StringStorage::SharedMixed { drop, .. } => {
                std::slice::from_ref(drop)
            }
            StringStorage::DistinctNamed { drops, .. } | StringStorage::Mixed { drops, .. } => {
                drops
            }
        }
    }
}
impl EntryDescriptor {
    pub(super) fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
    /// Original zero-parameter Unit declaration.
    pub fn entry(&self) -> DeclarationId {
        self.entry
    }
    /// Complete source body.
    pub fn body(&self) -> Span {
        self.body
    }
    /// Every source call occurrence, in the complete original body order.
    pub fn calls(&self) -> &[EntryCallDescriptor] {
        match &self.calls {
            EntryCalls::Single(call) => std::slice::from_ref(call.as_ref()),
            EntryCalls::Pair(calls)
            | EntryCalls::SharedNamedPair { calls, .. }
            | EntryCalls::DifferentSharedNamedPair { calls, .. }
            | EntryCalls::DifferentSharedMixedPair { calls, .. }
            | EntryCalls::StaggeredNamedPair { calls, .. }
            | EntryCalls::LateInitializedPair(calls)
            | EntryCalls::DifferentTemporaryPair(calls) => calls.as_ref(),
        }
    }
    /// Two distinct root-owned initializers in source order and their separate final-use Drops.
    pub fn staggered_owners(&self) -> Option<&[EntryOwnerDescriptor; 2]> {
        let EntryCalls::StaggeredNamedPair { owners, .. } = &self.calls else {
            return None;
        };
        Some(owners.as_ref())
    }
    /// Root-owned initialization and the unique original final-use Drop of a shared pair.
    pub fn shared_owner(
        &self,
    ) -> Option<(UnitSymbolId, UnitExpressionId, Span, &[u8], &UnitDropFact)> {
        let (EntryCalls::SharedNamedPair { owner, .. }
        | EntryCalls::DifferentSharedNamedPair { owner, .. }
        | EntryCalls::DifferentSharedMixedPair { owner, .. }) = &self.calls
        else {
            return None;
        };
        Some((
            owner.root,
            owner.initializer,
            owner.span,
            &owner.bytes,
            &owner.drop,
        ))
    }
    /// Privately proven exact pair of fully temporary calls to two closed distinct instances.
    pub fn is_different_temporary_pair(&self) -> bool {
        matches!(self.calls, EntryCalls::DifferentTemporaryPair(_))
    }
    /// Privately proven single shared named owner across two closed distinct instances.
    pub fn is_different_shared_named_pair(&self) -> bool {
        matches!(self.calls, EntryCalls::DifferentSharedNamedPair { .. })
    }
    /// Privately proven shared root plus one temporary per distinct closed call.
    pub fn is_different_shared_mixed_pair(&self) -> bool {
        matches!(self.calls, EntryCalls::DifferentSharedMixedPair { .. })
    }
    /// Returns a key only when every original occurrence shares the complete instance.
    pub fn common_instance(&self) -> Option<&UnitCallableInstanceKey> {
        let first = self.calls().first()?.instance();
        self.calls()
            .iter()
            .all(|call| call.instance() == first)
            .then_some(first)
    }
}
pub(super) fn collect(
    sources: &crate::source::SourceMap,
    shells: &[Option<FunctionShell>],
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &ValidatedCompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Vec<EntryDescriptor> {
    names
        .names()
        .index()
        .declarations()
        .iter()
        .filter_map(|declaration| {
            let signature = typed
                .types()
                .signatures()
                .declaration(declaration.id())?
                .callable()?;
            if signature.receiver().is_some()
                || !signature.parameters().is_empty()
                || !signature.type_parameters().is_empty()
                || signature.borrow_return().is_some()
                || typed.types().types().get(signature.return_type())
                    != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            {
                return None;
            }
            let unit = declaration.source_unit();
            let source = names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|s| s.id() == unit)?;
            let parsed = inputs
                .iter()
                .find(|i| i.source_id() == source.source_id())?
                .parsed();
            let Item::Function {
                form,
                extension_receiver: None,
                ..
            } = parsed.ast().items().get(declaration.root()).ok()?.payload()
            else {
                return None;
            };
            let body::EntryBody {
                span: body_span,
                declarations,
                expressions,
                late,
            } = body::extract(parsed, form)?;
            let shared_pair = !late && expressions[1].is_some() && declarations.len() == 1;
            let staggered_pair = expressions[1].is_some() && declarations.len() == 2;
            let collect_call = |expression, declarations: &[crate::ast::StatementId]| {
                if !matches!(
                    parsed.ast().expressions().get(expression).ok()?.payload(),
                    Expression::Call { .. }
                ) {
                    return None;
                }
                if declarations.len() > 4 {
                    return None;
                }
                let formed_local = |span: Span| {
                    let mut bindings = owned.formed_owned_locals().iter().filter(|symbol| {
                        symbol.source_unit() == unit
                            && names
                                .names()
                                .source_units()
                                .get(unit.index())
                                .and_then(|s| s.resolution().symbols().get(symbol.symbol().index()))
                                .is_some_and(|resolved| resolved.span() == span)
                    });
                    let binding = *bindings.next()?;
                    bindings.next().is_none().then_some(binding)
                };
                let mut named = Vec::new();
                let mut locals = [None, None];
                let mut initialization_order = Vec::new();
                for statement in declarations {
                    let Statement::LocalVariable { declaration } =
                        parsed.ast().statements().get(*statement).ok()?.payload()
                    else {
                        return None;
                    };
                    let Item::Variable {
                        kind: VariableKind::Val,
                        name: NameMarker::Present(span),
                        initializer,
                        ..
                    } = parsed.ast().items().get(*declaration).ok()?.payload()
                    else {
                        return None;
                    };
                    let expression = parsed.ast().expressions().get(*initializer).ok()?;
                    let binding = formed_local(*span)?;
                    let initializer = UnitExpressionId::new(unit, *initializer);
                    let (expected, role) = match expression.payload() {
                        Expression::String { .. } => (BuiltinType::String, None),
                        Expression::Literal(LiteralKind::Integer(_)) => (BuiltinType::Int, Some(0)),
                        Expression::Literal(LiteralKind::Boolean(_)) => {
                            (BuiltinType::Boolean, Some(1))
                        }
                        _ => return None,
                    };
                    let expected = UnitTypeKind::Builtin(expected);
                    if typed
                        .types()
                        .expression_type(initializer)
                        .and_then(|ty| typed.types().types().get(ty))
                        != Some(&expected)
                        || typed
                            .types()
                            .symbol_type(binding)
                            .and_then(|ty| typed.types().types().get(ty))
                            != Some(&expected)
                    {
                        return None;
                    }
                    if let Some(role) = role {
                        if locals[role].replace((binding, initializer)).is_some() {
                            return None;
                        }
                        initialization_order.push(if role == 0 {
                            EntryInitialization::Rounds
                        } else {
                            EntryInitialization::Choose
                        });
                    } else {
                        if named.iter().any(|(root, _, _, _)| *root == binding) {
                            return None;
                        }
                        let bytes = plain_string(sources, expression.payload())?;
                        named.push((binding, initializer, expression.span(), bytes));
                        initialization_order.push(EntryInitialization::Named(binding));
                    }
                }

                let call_id = UnitExpressionId::new(unit, expression);
                let call = typed.types().call(call_id)?;
                if call.receiver().is_some() || !call.instance().type_arguments().is_empty() {
                    return None;
                }
                let UnitCallTarget::Declaration(target) = call.target() else {
                    return None;
                };
                let mut matching_shells = shells
                    .iter()
                    .flatten()
                    .filter(|shell| shell.callable() == target);
                let parameter_roles = matching_shells.next()?.roles();
                if matching_shells.next().is_some() {
                    return None;
                }
                let contracts: [UnitCallArgumentOwnershipContract; 4] = owned
                    .call_argument_contracts_for(call_id)
                    .copied()
                    .collect::<Vec<_>>()
                    .try_into()
                    .ok()?;
                if call.arguments().len() != 4 {
                    return None;
                }
                let mut argument_positions = [0; 4];
                for (role, parameter) in parameter_roles.iter().enumerate() {
                    let mut matching = contracts
                        .iter()
                        .enumerate()
                        .filter(|(_, contract)| contract.parameter_index() == *parameter);
                    argument_positions[role] = matching.next()?.0;
                    if matching.next().is_some() {
                        return None;
                    }
                }
                let role_at = |index| {
                    argument_positions
                        .iter()
                        .position(|position| *position == index)
                };
                let mut temporary_arguments = Vec::new();
                let mut scalar_initializers = [None, None];
                let mut temporary_scalars = [None, None];
                for (i, c) in contracts.iter().enumerate() {
                    if c.parameter_index() != call.arguments()[i].parameter_index()
                        || contracts
                            .iter()
                            .filter(|other| other.argument() == c.argument())
                            .count()
                            != 1
                        || c.kind() != UnitCallArgumentOwnershipKind::SharedLoan
                        || c.crosses_thread()
                        || call.arguments()[i].argument_index() != i
                        || call.arguments()[i].parameter_type() != c.parameter_type()
                    {
                        return None;
                    }
                    let node = parsed
                        .ast()
                        .expressions()
                        .get(c.argument().expression())
                        .ok()?;
                    if role_at(i)? < 2 {
                        match node.payload() {
                            Expression::Name if !named.is_empty() => {}
                            Expression::String { .. } if named.len() <= 1 => {
                                temporary_arguments
                                    .push((i, plain_string(sources, node.payload())?));
                            }
                            _ => return None,
                        }
                    } else {
                        let scalar = role_at(i)? - 2;
                        let expected = UnitTypeKind::Builtin(if scalar == 0 {
                            BuiltinType::Int
                        } else {
                            BuiltinType::Boolean
                        });
                        let ty = typed.types().expression_type(c.argument())?;
                        if typed.types().types().get(ty) != Some(&expected)
                            || c.parameter_type() != ty
                        {
                            return None;
                        }
                        scalar_initializers[scalar] =
                            Some(match (node.payload(), locals[scalar]) {
                                (Expression::Name, Some((_, initializer))) => initializer,
                                (Expression::Literal(LiteralKind::Integer(_)), None)
                                    if scalar == 0 =>
                                {
                                    temporary_scalars[scalar] = Some(c.argument());
                                    c.argument()
                                }
                                (Expression::Literal(LiteralKind::Boolean(_)), None)
                                    if scalar == 1 =>
                                {
                                    temporary_scalars[scalar] = Some(c.argument());
                                    c.argument()
                                }
                                _ => return None,
                            });
                    }
                }
                let ends: [UnitDirectCallLoanEndFact; 4] = owned
                    .direct_call_loan_ends()
                    .iter()
                    .filter(|e| e.loan().call() == call_id)
                    .cloned()
                    .collect::<Vec<_>>()
                    .try_into()
                    .ok()?;
                for (i, c) in contracts.iter().enumerate() {
                    let mut matching = ends.iter().filter(|e| e.loan().argument() == c.argument());
                    let end = matching.next()?;
                    if matching.next().is_some()
                        || end.point() != UnitDropPoint::CallReturn(call_id)
                        || end.preceding_drops() != 0
                        || end.loan().kind() != crate::ownership_checking::LoanKind::Shared
                    {
                        return None;
                    }
                    let role = role_at(i)?;
                    match (role, end.loan().target()) {
                        (0 | 1, UnitLoanTarget::Temporary(owner))
                            if temporary_arguments.iter().any(|(index, _)| *index == i)
                                && *owner == c.argument() => {}
                        (0 | 1, UnitLoanTarget::Place(place))
                            if !temporary_arguments.iter().any(|(index, _)| *index == i)
                                && named.iter().any(|n| place.is_root() && place.root() == n.0) => {
                        }
                        (2 | 3, UnitLoanTarget::Place(place))
                            if locals[role - 2].is_some_and(|(root, _)| {
                                place.is_root() && place.root() == root
                            }) => {}
                        (2 | 3, UnitLoanTarget::Temporary(owner))
                            if temporary_scalars[role - 2] == Some(c.argument())
                                && *owner == c.argument() => {}
                        _ => return None,
                    }
                }
                for (position, end) in ends.iter().enumerate() {
                    if end.preceding_call_loan_ends() != position {
                        return None;
                    }
                }
                let drops: Vec<_> = owned
                    .drops()
                    .iter()
                    .filter(|d| d.point() == UnitDropPoint::CallReturn(call_id))
                    .copied()
                    .collect();
                let shared_owner = if staggered_pair {
                    if named.len() != 2
                        || locals.iter().any(Option::is_some)
                        || !temporary_arguments.is_empty()
                    {
                        return None;
                    }
                    Some(named.clone())
                } else if shared_pair {
                    if named.len() != 1
                        || locals.iter().any(Option::is_some)
                        || temporary_arguments.len() > 1
                    {
                        return None;
                    }
                    Some(named.clone())
                } else {
                    None
                };
                let storage = if staggered_pair {
                    initialization_order.clear();
                    StringStorage::StaggeredRoots {
                        roots: [named[0].0, named[1].0],
                    }
                } else if let Some(owners) = &shared_owner {
                    let (root, ..) = &owners[0];
                    initialization_order.clear();
                    if let Some((temporary_index, temporary)) = temporary_arguments.pop() {
                        let target =
                            UnitDropTarget::Temporary(contracts[temporary_index].argument());
                        let mut matching = drops.iter().filter(|d| d.target() == target);
                        let drop = *matching.next()?;
                        if matching.next().is_some()
                            || owned
                                .drops()
                                .iter()
                                .filter(|d| d.target() == target)
                                .count()
                                != 1
                        {
                            return None;
                        }
                        StringStorage::SharedMixed {
                            root: *root,
                            temporary_index,
                            temporary,
                            drop,
                        }
                    } else {
                        StringStorage::SharedRoot { root: *root }
                    }
                } else if named.len() == 1 && temporary_arguments.len() == 1 {
                    let (temporary_index, temporary) = temporary_arguments.pop()?;
                    let temporary_target =
                        UnitDropTarget::Temporary(contracts[temporary_index].argument());
                    let named_target = UnitDropTarget::Named(named[0].0);
                    let drops: [UnitDropFact; 2] = drops.try_into().ok()?;
                    for target in [named_target, temporary_target] {
                        if drops.iter().filter(|drop| drop.target() == target).count() != 1
                            || owned
                                .drops()
                                .iter()
                                .filter(|drop| drop.target() == target)
                                .count()
                                != 1
                        {
                            return None;
                        }
                    }
                    let other_role = 1usize.checked_sub(role_at(temporary_index)?)?;
                    let named_argument = &contracts[argument_positions[other_role]];
                    if !ends.iter().any(|end| {
                        end.loan().argument() == named_argument.argument()
                            && matches!(end.loan().target(), UnitLoanTarget::Place(place)
                        if place.is_root() && place.root() == named[0].0)
                    }) {
                        return None;
                    }
                    StringStorage::Mixed {
                        owner: named.into_iter().next()?,
                        temporary_index,
                        temporary,
                        drops,
                    }
                } else if named.len() == 2 {
                    let drops: [UnitDropFact; 2] = drops.try_into().ok()?;
                    for (root, _, _, _) in &named {
                        let argument_count = argument_positions[..2]
                            .iter()
                            .map(|index| &contracts[*index])
                            .filter(|contract| {
                                ends.iter().any(|end| {
                                end.loan().argument() == contract.argument()
                                    && matches!(end.loan().target(), UnitLoanTarget::Place(place)
                                    if place.is_root() && place.root() == *root)
                            })
                            })
                            .count();
                        if argument_count != 1
                            || drops
                                .iter()
                                .filter(|drop| drop.target() == UnitDropTarget::Named(*root))
                                .count()
                                != 1
                            || owned
                                .drops()
                                .iter()
                                .filter(|drop| drop.target() == UnitDropTarget::Named(*root))
                                .count()
                                != 1
                        {
                            return None;
                        }
                    }
                    StringStorage::DistinctNamed {
                        owners: named.try_into().ok()?,
                        drops,
                    }
                } else if let Some((root, initializer, span, bytes)) = named.into_iter().next() {
                    if !temporary_arguments.is_empty() {
                        return None;
                    }
                    let [drop] = drops.as_slice() else {
                        return None;
                    };
                    if drop.target() != UnitDropTarget::Named(root)
                        || owned
                            .drops()
                            .iter()
                            .filter(|d| d.target() == UnitDropTarget::Named(root))
                            .count()
                            != 1
                    {
                        return None;
                    }
                    StringStorage::Named {
                        root,
                        initializer,
                        span,
                        bytes,
                        drop: *drop,
                    }
                } else {
                    let drops: [UnitDropFact; 2] = drops.try_into().ok()?;
                    for c in argument_positions[..2]
                        .iter()
                        .map(|index| &contracts[*index])
                    {
                        if drops
                            .iter()
                            .filter(|d| d.target() == UnitDropTarget::Temporary(c.argument()))
                            .count()
                            != 1
                        {
                            return None;
                        }
                    }
                    let strings: [Vec<u8>; 2] = argument_positions[..2]
                        .iter()
                        .map(|index| &contracts[*index])
                        .map(|c| {
                            plain_string(
                                sources,
                                parsed
                                    .ast()
                                    .expressions()
                                    .get(c.argument().expression())
                                    .ok()?
                                    .payload(),
                            )
                        })
                        .collect::<Option<Vec<_>>>()?
                        .try_into()
                        .ok()?;
                    StringStorage::Temporary { strings, drops }
                };
                let initializers = [scalar_initializers[0]?, scalar_initializers[1]?];
                let initializer_spans = [
                    parsed
                        .ast()
                        .expressions()
                        .get(initializers[0].expression())
                        .ok()?
                        .span(),
                    parsed
                        .ast()
                        .expressions()
                        .get(initializers[1].expression())
                        .ok()?
                        .span(),
                ];
                let Expression::Literal(LiteralKind::Integer(kind)) = parsed
                    .ast()
                    .expressions()
                    .get(initializers[0].expression())
                    .ok()?
                    .payload()
                else {
                    return None;
                };
                let magnitude = crate::type_checking::integer_literal_magnitude(
                    sources.slice(initializer_spans[0]).ok()?,
                    *kind,
                )?;
                let rounds = i32::try_from(magnitude).ok()?;
                let Expression::Literal(LiteralKind::Boolean(choose)) = parsed
                    .ast()
                    .expressions()
                    .get(initializers[1].expression())
                    .ok()?
                    .payload()
                else {
                    return None;
                };
                Some((
                    EntryCallDescriptor {
                        storage,
                        rounds,
                        choose: *choose,
                        initializer_spans,
                        instance: call.instance().clone(),
                        parameter_roles,
                        initializers,
                        temporary_scalars,
                        initialization_order,
                        contracts,
                        ends,
                    },
                    shared_owner,
                ))
            };
            let (first, first_owner) =
                collect_call(expressions[0]?, if late { &[] } else { declarations })?;
            let calls = if let Some(second_id) = expressions[1] {
                let (second, second_owner) = collect_call(second_id, declarations)?;
                if first.contracts()[0].call() == second.contracts()[0].call()
                    || [first.contracts()[0].call(), second.contracts()[0].call()]
                        .iter()
                        .any(|call| call.source_unit() != unit)
                {
                    return None;
                }
                if first.instance() != second.instance() {
                    if late {
                        return None;
                    }
                    if shared_pair {
                        let owners = first_owner?;
                        if second_owner? != owners {
                            return None;
                        }
                        let [owner]: [_; 1] = owners.try_into().ok()?;
                        if matches!(
                            (&first.storage, &second.storage),
                            (
                                StringStorage::SharedMixed { .. },
                                StringStorage::SharedMixed { .. }
                            )
                        ) {
                            different_shared_mixed_pair::finish(
                                owner, first, second, owned, parsed,
                            )?
                        } else {
                            different_shared_pair::finish(owner, first, second, owned, parsed)?
                        }
                    } else if declarations.is_empty() {
                        different_pair::finish(first, second, owned, parsed)?
                    } else {
                        return None;
                    }
                } else if late {
                    late_pair::finish(first, second, owned, parsed)?
                } else {
                    if !first.initialization_order().is_empty()
                        || !second.initialization_order().is_empty()
                        || !first.named_strings().is_empty()
                        || !second.named_strings().is_empty()
                    {
                        return None;
                    }
                    if shared_pair || staggered_pair {
                        let owners = first_owner?;
                        if second_owner? != owners {
                            return None;
                        }
                        if staggered_pair {
                            staggered_pair::finish(owners.try_into().ok()?, first, second, owned)?
                        } else {
                            let [owner]: [_; 1] = owners.try_into().ok()?;
                            shared_pair::finish(owner, first, second, owned)?
                        }
                    } else {
                        EntryCalls::Pair(Box::new([first, second]))
                    }
                }
            } else {
                EntryCalls::Single(Box::new(first))
            };
            Some(EntryDescriptor {
                identity: Rc::new(()),
                entry: declaration.id(),
                body: body_span,
                calls,
            })
        })
        .collect()
}

fn plain_string(sources: &crate::source::SourceMap, expression: &Expression) -> Option<Vec<u8>> {
    let Expression::String { parts } = expression else {
        return None;
    };
    let mut bytes = Vec::new();
    for part in parts {
        let StringPart::Text(span) = part else {
            return None;
        };
        let text = sources.slice(*span).ok()?;
        let decoded = crate::type_checking::decode_string_literal_text(text)?;
        bytes.extend_from_slice(decoded.as_bytes());
    }
    Some(bytes)
}
