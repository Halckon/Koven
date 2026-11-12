//! 两个真实 factory Call.result → 全 incoming owner/source/cursor → 当前 owner Drop。
//! 只证静态身份；不推断动态执行/析构次数，不另造 LLVM 结构 oracle。
use super::{
    super::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, function, loan, result, value},
        query_inputs::one,
    },
    query_body::Body,
    query_inputs::{Inputs, SiteInput},
};
use crate::ssa::model::*;
use crate::test_support::diagnostics::diagnostic;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Roles {
    pub owner: ValueId,
    pub source: LoanId,
    pub cursor: ValueId,
}

impl Roles {
    fn entities(self) -> [EntityId; 3] {
        [
            EntityId::Value(self.owner),
            EntityId::Loan(self.source),
            EntityId::Value(self.cursor),
        ]
    }
    fn protect(self, uses: &mut Uses, protected: &mut BTreeSet<EntityId>) {
        for entity in self.entities() {
            uses.protect(entity);
            protected.insert(entity);
        }
    }
}

fn as_value(entity: EntityId) -> Result<ValueId, Failure> {
    match entity {
        EntityId::Value(id) => Ok(id),
        _ => Err(failure("temporary SSA actual Value column", entity)),
    }
}
fn as_loan(entity: EntityId) -> Result<LoanId, Failure> {
    match entity {
        EntityId::Loan(id) => Ok(id),
        _ => Err(failure("temporary SSA actual Loan column", entity)),
    }
}
fn ty(graph: &Graph<'_>, entity: EntityId) -> Result<EntityType, Failure> {
    Ok(graph
        .function
        .entity(entity)
        .ok_or_else(|| failure("temporary SSA actual entity type", entity))?
        .ty)
}
fn set<T: Copy + Ord>(values: &[T]) -> BTreeSet<T> {
    values.iter().copied().collect()
}

fn transport(
    graph: &Graph<'_>,
    edge: &Edge,
    roles: Roles,
    uses: &mut Uses,
    protected: &mut BTreeSet<EntityId>,
) -> Result<Roles, Failure> {
    let expected = roles.entities();
    need(
        set(&expected).len() == 3
            && edge.arguments.len() == 3
            && set(&edge.arguments) == set(&expected),
        "temporary SSA exact full current-role edge operands",
        (edge, roles),
    )?;
    let next = Roles {
        owner: as_value(graph.transported(edge, EntityId::Value(roles.owner))?)?,
        source: as_loan(graph.transported(edge, EntityId::Loan(roles.source))?)?,
        cursor: as_value(graph.transported(edge, EntityId::Value(roles.cursor))?)?,
    };
    next.protect(uses, protected);
    Ok(next)
}

#[derive(Debug)]
struct Shape {
    blocks: [BlockId; 4],
    edges: [(BlockId, BlockId); 4],
    call: InstructionId,
    factory: FunctionId,
    drop: InstructionId,
    protected: BTreeSet<EntityId>,
    providers: Vec<InstructionId>,
    condition: ValueId,
}

struct Context<'a, 'ir> {
    graph: &'a Graph<'ir>,
    module: &'ir Module,
    read: &'ir Function,
    map: bool,
}

impl Context<'_, '_> {
    fn site(&self, site: &SiteInput, uses: &mut Uses) -> Result<Shape, Failure> {
        let graph = self.graph;
        let factory = function(self.module, site.factory)?;
        let call = one(graph.instructions.iter().copied().filter(|i|
            matches!(i.operation, Operation::DirectCall { callee, .. } if callee == factory.id)),
            "temporary SSA unique actual factory Call")?;
        need(
            call.origin == Origin::Source(site.source)
                && matches!(&call.operation, Operation::DirectCall { receiver: None, arguments, .. } if arguments.is_empty()),
            "temporary SSA actual source Call birth with zero arguments",
            call,
        )?;
        let owner = value(call)?;
        let EntityType::Value(container) = ty(graph, EntityId::Value(owner))? else {
            return Err(failure(
                "temporary SSA factory result is real owning Value",
                call,
            ));
        };
        let expected_kind = if self.map {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        };
        need(
            matches!(self.module.type_kind(container), Some(SsaTypeKind::MapContainer { kind, key, value })
            if *kind == expected_kind && self.module.type_kind(*key) == Some(&SsaTypeKind::StringOwner)
                && self.module.type_kind(*value) == Some(&SsaTypeKind::StringOwner))
                && factory.receiver.is_none()
                && factory.borrow_return.is_none()
                && factory.carrier_return.is_none()
                && factory.return_types.as_slice() == [container]
                && factory
                    .entry_block()
                    .and_then(|id| factory.block(id))
                    .is_some_and(|b| b.parameters.is_empty()),
            "temporary SSA factory actual Owned map signature and String K/V",
            factory,
        )?;
        let root = one(
            graph
                .instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::RootPlace { owner }),
            "temporary SSA unique root of actual factory result",
        )?;
        let EntityId::Place(place) = result(root)? else {
            return Err(failure("temporary SSA actual RootPlace result", root));
        };
        let borrow = one(graph.instructions.iter().copied().filter(|i|
            matches!(i.operation, Operation::BorrowBegin { place: p, kind: LoanKind::Shared } if p == place)),
            "temporary SSA actual source BorrowBegin")?;
        let lease = loan(borrow)?;
        let first = one(
            graph
                .instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::MapIterationFirst { source: lease }),
            "temporary SSA First of actual fresh source lease",
        )?;
        let cursor = value(first)?;
        let pre = call.block;
        let ops = graph.ops(pre)?;
        let position = graph.position(call)?;
        need(
            [root, borrow, first]
                .iter()
                .all(|i| i.block == pre && i.origin == Origin::Source(site.span))
                && ty(graph, EntityId::Place(place))? == EntityType::Place(container)
                && ty(graph, EntityId::Loan(lease))?
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: container,
                    }
                && matches!(ty(graph, EntityId::Value(cursor))?, EntityType::Value(t)
                if self.module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 64, signed: false }))
                && ops[position..].iter().map(|i| i.id).collect::<Vec<_>>()
                    == [call.id, root.id, borrow.id, first.id],
            "temporary SSA actual Call/Root/Borrow/First preheader suffix",
            (&ops, call, root, borrow, first),
        )?;
        let mut protected = BTreeSet::new();
        let initial = Roles {
            owner,
            source: lease,
            cursor,
        };
        initial.protect(uses, &mut protected);
        uses.protect(EntityId::Place(place));
        protected.insert(EntityId::Place(place));
        uses.permit(EntityId::Value(owner), root);
        uses.permit(EntityId::Place(place), borrow);
        uses.permit(EntityId::Loan(lease), first);
        let entry_edge = graph.branch(pre)?;
        need(
            graph.term(pre)?.origin == Origin::Source(site.span),
            "temporary SSA preheader branch actual For",
            graph.term(pre)?,
        )?;
        let header = entry_edge.target;
        let current = transport(graph, entry_edge, initial, uses, &mut protected)?;
        let has = one(
            graph.ops(header)?.into_iter(),
            "temporary SSA header exactly actual HasNext",
        )?;
        need(
            has.operation
                == Operation::MapIterationHasNext {
                    source: current.source,
                    cursor: current.cursor,
                }
                && has.origin == Origin::Source(site.span)
                && matches!(ty(graph, result(has)?)?, EntityType::Value(t) if self.module.type_kind(t) == Some(&SsaTypeKind::Boolean)),
            "temporary SSA header actual source/cursor condition",
            has,
        )?;
        uses.permit(EntityId::Loan(current.source), has);
        uses.permit(EntityId::Value(current.cursor), has);
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &graph.term(header)?.kind
        else {
            return Err(failure(
                "temporary SSA actual HasNext Conditional",
                graph.term(header)?,
            ));
        };
        need(
            *condition == value(has)? && graph.term(header)?.origin == Origin::Source(site.span),
            "temporary SSA actual HasNext result is branch condition",
            graph.term(header)?,
        )?;
        // Uses 原 helper 只管 transport/Return；Boolean condition 单独全 uses 核验，不放宽 helper。
        protected.insert(EntityId::Value(*condition));
        for instruction in &graph.instructions {
            need(
                !instruction
                    .operation
                    .entities()
                    .contains(&EntityId::Value(*condition)),
                "temporary SSA HasNext result has no instruction use",
                instruction,
            )?;
        }
        for &block in &graph.blocks {
            let term = graph.term(block)?;
            if term.kind.entities().contains(&EntityId::Value(*condition)) {
                need(
                    block == header
                        && matches!(&term.kind, TerminatorKind::Conditional { condition: actual, when_true, when_false }
                    if actual == condition && !when_true.arguments.contains(&EntityId::Value(*condition))
                        && !when_false.arguments.contains(&EntityId::Value(*condition))),
                    "temporary SSA HasNext only used by this actual condition",
                    term,
                )?;
            }
        }
        let (body, exit) = (when_true.target, when_false.target);
        need(
            set(&[pre, header, body, exit]).len() == 4,
            "temporary SSA distinct per-site role blocks",
            (pre, header, body, exit),
        )?;
        let body_roles = transport(graph, when_true, current, uses, &mut protected)?;
        let exit_roles = transport(graph, when_false, current, uses, &mut protected)?;
        for block in [header, body, exit] {
            need(
                graph.block(block)?.origin == Origin::Source(site.span),
                "temporary SSA role block actual For origin",
                graph.block(block)?,
            )?;
            for &parameter in &graph.block(block)?.parameters {
                need(
                    graph
                        .function
                        .entity(parameter)
                        .is_some_and(|e| e.origin == Origin::Source(site.span)),
                    "temporary SSA role parameter actual For origin",
                    parameter,
                )?;
            }
        }
        graph.incoming_from(header, &[pre, body])?;
        graph.incoming_from(body, &[header])?;
        graph.incoming_from(exit, &[header])?;
        let next = Body {
            graph,
            read: self.read,
            module: self.module,
            site,
            block: body,
            roles: body_roles,
        }
        .check(uses, &mut protected)?;
        let back = graph.branch(body)?;
        need(
            back.target == header && graph.term(body)?.origin == Origin::Source(site.span),
            "temporary SSA actual Next backedge target",
            graph.term(body)?,
        )?;
        let back_roles = Roles {
            cursor: next,
            ..body_roles
        };
        let returned = transport(graph, back, back_roles, uses, &mut protected)?;
        need(
            returned == current
                && graph.argument(back, EntityId::Value(current.owner))?
                    == EntityId::Value(body_roles.owner)
                && graph.argument(back, EntityId::Loan(current.source))?
                    == EntityId::Loan(body_roles.source)
                && graph.argument(back, EntityId::Value(current.cursor))? == EntityId::Value(next),
            "temporary SSA every Header incoming keeps owner/source and actual Next result",
            (current, returned, back),
        )?;
        let exit_ops = graph.ops(exit)?;
        let end = one(
            exit_ops.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowEnd {
                        loan: exit_roles.source,
                    }
            }),
            "temporary SSA false exact current source end",
        )?;
        let drop = one(
            graph.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::Drop {
                        owner: exit_roles.owner,
                    }
            }),
            "temporary SSA unique Drop of current transported owner",
        )?;
        need(
            end.origin == Origin::Source(site.span)
                && drop.origin == Origin::Source(site.drop)
                && drop.block == exit
                && end.results.is_empty()
                && drop.results.is_empty()
                && exit_ops.first().map(|i| i.id) == Some(end.id)
                && exit_ops.get(1).map(|i| i.id) == Some(drop.id),
            "temporary SSA false EndSource then current Call-origin owner Drop prefix",
            (end, drop, &exit_ops),
        )?;
        uses.permit(EntityId::Loan(exit_roles.source), end);
        uses.permit(EntityId::Value(exit_roles.owner), drop);
        // 后续 marker String 的普通借用/析构和第二 site 初始化合法；只禁本 site 的 payload 锚。
        need(
            !exit_ops.iter().any(|i| {
                (i.origin == Origin::Source(site.span)
                    && matches!(
                        i.operation,
                        Operation::MapIterationKeyLoan { .. }
                            | Operation::MapIterationValueLoan { .. }
                            | Operation::MapIterationNext { .. }
                    ))
                    || (i.origin == Origin::Source(site.read_call)
                        && matches!(i.operation, Operation::DirectCall { .. }))
            }),
            "temporary SSA false path has no this-site payload/read/Next",
            &exit_ops,
        )?;
        let providers = graph
            .instructions
            .iter()
            .filter(|i| i.origin == Origin::Source(site.span) && provider(&i.operation))
            .map(|i| i.id)
            .collect::<Vec<_>>();
        need(
            providers.len() == 5
                && providers.contains(&first.id)
                && providers.contains(&has.id)
                && graph
                    .ops(body)?
                    .iter()
                    .filter(|i| provider(&i.operation))
                    .count()
                    == 3,
            "temporary SSA complete five provider operations for this site",
            &providers,
        )?;
        Ok(Shape {
            blocks: [pre, header, body, exit],
            edges: [
                (pre, header),
                (header, body),
                (header, exit),
                (body, header),
            ],
            call: call.id,
            factory: factory.id,
            drop: drop.id,
            protected,
            providers,
            condition: *condition,
        })
    }
}

fn provider(operation: &Operation) -> bool {
    matches!(
        operation,
        Operation::MapIterationFirst { .. }
            | Operation::MapIterationHasNext { .. }
            | Operation::MapIterationKeyLoan { .. }
            | Operation::MapIterationValueLoan { .. }
            | Operation::MapIterationNext { .. }
    )
}

pub(super) fn check(
    program: &Program,
    entry: FunctionId,
    input: &Inputs,
    map: bool,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "temporary SSA sole actual module")?;
    let function_entry = function(module, input.entry)?;
    let read = function(module, input.read)?;
    let graph = Graph::new(function_entry)?;
    let pre = function_entry
        .entry_block()
        .ok_or_else(|| failure("temporary SSA actual entry block", entry))?;
    need(
        function_entry.id == entry
            && function_entry.receiver.is_none()
            && function_entry.borrow_return.is_none()
            && function_entry.carrier_return.is_none()
            && function_entry.return_types.is_empty()
            && graph.block(pre)?.parameters.is_empty()
            && module.functions.len() == 4
            && graph.blocks.len() == function_entry.blocks.len()
            && graph.instructions.len() == function_entry.instructions.len(),
        "temporary SSA returned entry and complete actual block/instruction membership",
        function_entry,
    )?;
    let [first_input, second_input] = input.sites.as_slice() else {
        return Err(failure("temporary SSA exactly two FE-bridged sites", input));
    };
    need(
        first_input.source != second_input.source
            && first_input.factory != second_input.factory
            && first_input.read_call != second_input.read_call
            && first_input.span.end() <= second_input.span.start(),
        "temporary SSA independent bridged two source Call anchors",
        input,
    )?;
    let context = Context {
        graph: &graph,
        module,
        read,
        map,
    };
    let mut uses = Uses::default();
    let first = context.site(first_input, &mut uses)?;
    let second = context.site(second_input, &mut uses)?;
    need(
        first.blocks[0] == pre
            && first.blocks[3] == second.blocks[0]
            && first.factory != second.factory
            && first.call != second.call
            && first.drop != second.drop
            && first.condition != second.condition
            && first.protected.is_disjoint(&second.protected),
        "temporary SSA first exhaustion then second birth with disjoint complete identity sets",
        (&first, &second),
    )?;
    graph.incoming_from(pre, &[])?;
    let mut expected_blocks = first.blocks.to_vec();
    expected_blocks.extend(second.blocks);
    let mut expected_edges = first.edges.to_vec();
    expected_edges.extend(second.edges);
    let actual_edges = graph
        .edges
        .iter()
        .map(|(from, e)| (*from, e.target))
        .collect::<Vec<_>>();
    need(
        graph.blocks.len() == 7
            && set(&graph.blocks) == set(&expected_blocks)
            && actual_edges.len() == expected_edges.len()
            && set(&actual_edges) == set(&expected_edges),
        "temporary SSA every real block and all eight transport edges closed",
        (&graph.blocks, &actual_edges),
    )?;
    let exit = second.blocks[3];
    need(
        matches!(&graph.term(exit)?.kind, TerminatorKind::Return { values } if values.is_empty())
            && graph.term(exit)?.origin == Origin::Source(input.entry),
        "temporary SSA second exhaustion reaches actual normal entry Return",
        graph.term(exit)?,
    )?;
    let expected_providers = first
        .providers
        .iter()
        .chain(&second.providers)
        .copied()
        .collect::<Vec<_>>();
    let actual_providers = graph
        .instructions
        .iter()
        .filter(|i| provider(&i.operation))
        .map(|i| i.id)
        .collect::<Vec<_>>();
    need(
        set(&expected_providers) == set(&actual_providers) && actual_providers.len() == 10,
        "temporary SSA no extra provider instruction outside either proven site",
        actual_providers,
    )?;
    let calls = graph
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
        .copied()
        .collect::<Vec<_>>();
    need(
        calls.len() == 4 && calls.iter().all(|i| {
            i.id == first.call
                || i.id == second.call
                || matches!(i.operation, Operation::DirectCall { callee, .. } if callee == read.id)
                    && [first_input.read_call, second_input.read_call].contains(&i.origin.span())
        }),
        "temporary SSA exactly two real factory and two actual read calls",
        calls,
    )?;
    // 核对整个已返回 Program 中 factory 调用集合，不让 caller 外的同名调用伪装唯一。
    for shape in [&first, &second] {
        let mut calls = Vec::new();
        for function in &module.functions {
            let reachable = Graph::new(function)?;
            calls.extend(reachable.instructions.iter().filter_map(|i|
                matches!(i.operation, Operation::DirectCall { callee, .. } if callee == shape.factory).then_some(i.id)));
        }
        need(
            calls.as_slice() == [shape.call],
            "temporary SSA whole reachable Program has one actual factory call",
            calls,
        )?;
    }
    // These sets were built only from the two actual Call births and all
    // directed current-role columns. Type filtering merely selects Map roles.
    let types = [&first, &second]
        .into_iter()
        .map(|shape| {
            let call = graph
                .function
                .instruction(shape.call)
                .ok_or_else(|| failure("temporary inventory actual factory call", shape.call))?;
            Ok(ty(&graph, result(call)?)?.semantic_type())
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let related = first
        .protected
        .union(&second.protected)
        .copied()
        .filter(|&entity| {
            graph
                .function
                .entity(entity)
                .is_some_and(|data| types.contains(&data.ty.semantic_type()))
        })
        .collect::<BTreeSet<_>>();
    super::super::query_inventory::check(&graph, &types, &related)?;
    uses.check(&graph, None)?;
    // 全部受保护定义必须是图内实际 instruction result 或经过全部 incoming 核验的参数。
    for entity in first.protected.union(&second.protected) {
        let data = graph
            .function
            .entity(*entity)
            .ok_or_else(|| failure("temporary SSA protected entity definition", entity))?;
        match data.definition {
            Definition::InstructionResult { .. } => {
                let definition = graph.definition(*entity)?;
                need(
                    data.origin == definition.origin,
                    "temporary SSA protected result actual origin",
                    (entity, data, definition),
                )?;
            }
            Definition::BlockParameter { block, index } => need(
                graph.blocks.contains(&block)
                    && graph.block(block)?.parameters.get(index) == Some(entity),
                "temporary SSA protected actual block parameter",
                (entity, data),
            )?,
        }
    }
    diagnostic!(
        "SPEC0331 TEMPORARY IDENTITY SSA entry={entry:?} input={input:#?} first={first:#?} second={second:#?}"
    );
    Ok(())
}

pub(in crate::ssa) mod ast {
//! 两个同层 For 的真实 AST 身份；不复用单 For/Name 参数 fixture 假设。
use super::super::super::fixture::{Failure, clean, failure, need};
use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    lexer::lex,
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile,
        Statement, parse_file,
    },
    source::{SourceMap, Span},
};

pub(in crate::ssa) struct Input {
    pub sources: SourceMap,
    pub parsed: ParsedFile,
    pub map: bool,
}

impl Input {
    pub fn new(text: &str, map: bool) -> Result<Self, Failure> {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("provider.ko", text)
            .map_err(|e| failure("source", e))?;
        let lexed = lex(&sources, id).map_err(|e| failure("lex infrastructure", e))?;
        clean(&sources, "lex diagnostics", lexed.diagnostics())?;
        let parsed =
            parse_file(&sources, &lexed).map_err(|e| failure("parse infrastructure", e))?;
        clean(&sources, "parse diagnostics", parsed.diagnostics())?;
        need(
            parsed.package().is_none(),
            "AST default package",
            parsed.package(),
        )?;
        Ok(Self {
            sources,
            parsed,
            map,
        })
    }

    pub fn expression(&self, id: ExpressionId) -> Result<(&Expression, Span), Failure> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(|e| failure("AST expression", e))?;
        Ok((node.payload(), node.span()))
    }

    fn elements(&self, id: StatementId) -> Result<&[StatementId], Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("AST block", e))?;
        let Statement::Block { elements } = node.payload() else {
            return Err(failure("AST actual Block", node));
        };
        Ok(elements)
    }

    fn statement_expression(&self, id: StatementId) -> Result<ExpressionId, Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("AST statement", e))?;
        let Statement::Expression { expression } = node.payload() else {
            return Err(failure("AST expression statement", node));
        };
        Ok(*expression)
    }

    fn call(
        &self,
        id: ExpressionId,
        name: &str,
        arity: usize,
    ) -> Result<Vec<ExpressionId>, Failure> {
        let (expression, _) = self.expression(id)?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = expression
        else {
            return Err(failure("AST actual direct Call", expression));
        };
        let (callee, span) = self.expression(*callee)?;
        need(
            matches!(callee, Expression::Name)
                && self.sources.slice(span) == Ok(name)
                && type_arguments.is_empty()
                && arguments.len() == arity,
            "AST exact direct callee and arity",
            (id, expression, name),
        )?;
        Ok(arguments.iter().map(|a| a.value).collect())
    }

    fn marker(&self, statement: StatementId, expected: &str) -> Result<(), Failure> {
        let id = self.statement_expression(statement)?;
        let arguments = self.call(id, "println", 1)?;
        let [argument] = arguments.as_slice() else {
            return Err(failure("AST marker argument", arguments));
        };
        let (literal, span) = self.expression(*argument)?;
        need(
            matches!(literal, Expression::String { .. })
                && self.sources.slice(span) == Ok(expected),
            "AST exact entry marker",
            (id, literal, span),
        )
    }
}

#[derive(Debug)]
pub(in crate::ssa) struct Function {
    pub item: ItemId,
    pub name: &'static str,
    pub name_span: Span,
    pub span: Span,
    pub body: StatementId,
}

#[derive(Debug)]
pub(in crate::ssa) struct Site {
    pub factory: &'static str,
    pub statement: StatementId,
    pub span: Span,
    pub body: StatementId,
    pub source: ExpressionId,
    pub source_span: Span,
    pub bindings: [Span; 2],
    pub read: ExpressionId,
    pub arguments: [ExpressionId; 2],
}

#[derive(Debug)]
pub(in crate::ssa) struct Anchors {
    pub functions: Vec<Function>,
    pub sites: [Site; 2],
}

impl Anchors {
    pub fn functions_for_ssa(&self) -> Vec<(&str, Span)> {
        self.functions.iter().map(|f| (f.name, f.span)).collect()
    }

    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|f| f.name == name)
    }
}

pub(in crate::ssa) fn collect(f: &Input) -> Result<Anchors, Failure> {
    let factories = if f.map {
        ["makeMap", "makeEmptyMap"]
    } else {
        ["makeMutable", "makeEmptyMutable"]
    };
    let ast = f.parsed.ast();
    need(
        f.parsed.roots().len() == 4,
        "AST exact four function roots",
        f.parsed.roots(),
    )?;
    let mut functions = Vec::new();
    for expected in ["entry", "read", factories[0], factories[1]] {
        let mut found = Vec::new();
        for id in f.parsed.roots() {
            let node = ast
                .items()
                .get(*id)
                .map_err(|e| failure("AST function root", e))?;
            if let Item::Function {
                name: NameMarker::Present(name),
                form,
                ..
            } = node.payload()
                && f.sources.slice(*name) == Ok(expected)
            {
                let body = match form {
                    FunctionForm::ImplicitUnitBlock(body)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    } => *body,
                    _ => return Err(failure("AST actual function body", node)),
                };
                need(
                    node.span().source_id() == f.parsed.source_id()
                        && f.sources.source_name(node.span().source_id()) == Ok("provider.ko"),
                    "AST original source function identity",
                    node,
                )?;
                found.push(Function {
                    item: *id,
                    name: expected,
                    name_span: *name,
                    span: node.span(),
                    body,
                });
            }
        }
        need(
            found.len() == 1,
            "AST unique actual function",
            (&expected, &found),
        )?;
        functions.extend(found);
    }
    let entry = functions
        .first()
        .ok_or_else(|| failure("AST entry", &functions))?;
    let [nonempty, first, first_done, empty, second, second_done] = f.elements(entry.body)? else {
        return Err(failure(
            "AST six actual entry statements",
            f.elements(entry.body)?,
        ));
    };
    f.marker(*nonempty, "\"nonempty\"")?;
    f.marker(*first_done, "\"nonempty-exhausted\"")?;
    f.marker(*empty, "\"empty\"")?;
    f.marker(*second_done, "\"empty-exhausted\"")?;
    let sites = [
        site(f, *first, factories[0])?,
        site(f, *second, factories[1])?,
    ];
    let all_for = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Statement::For { .. }).then_some(id))
        .collect::<Vec<_>>();
    need(
        all_for.len() == 2
            && sites.iter().all(|site| all_for.contains(&site.statement))
            && sites[0].statement != sites[1].statement
            && sites[0].source != sites[1].source
            && sites[0].read != sites[1].read
            && sites[0].span.end() <= sites[1].span.start()
            && sites[0]
                .bindings
                .iter()
                .all(|span| !sites[1].bindings.contains(span)),
        "AST exactly two independent same-level For sites",
        (&all_for, &sites),
    )?;
    Ok(Anchors { functions, sites })
}

fn site(f: &Input, statement: StatementId, factory: &'static str) -> Result<Site, Failure> {
    let node = f
        .parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|e| failure("AST For", e))?;
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        source,
        body,
        ..
    } = node.payload()
    else {
        return Err(failure("AST actual destructuring For", node));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure("AST two real binding markers", names));
    };
    need(
        f.sources.slice(*key) == Ok("key") && f.sources.slice(*value) == Ok("value"),
        "AST ordered key/value markers",
        names,
    )?;
    f.call(*source, factory, 0)?;
    let (_, source_span) = f.expression(*source)?;
    let [read_statement] = f.elements(*body)? else {
        return Err(failure("AST sole actual body read", f.elements(*body)?));
    };
    let read = f.statement_expression(*read_statement)?;
    let arguments = f.call(read, "read", 2)?;
    let [key_argument, value_argument] = arguments.as_slice() else {
        return Err(failure("AST two actual read arguments", arguments));
    };
    for (argument, expected) in [(*key_argument, "key"), (*value_argument, "value")] {
        let (expression, span) = f.expression(argument)?;
        need(
            matches!(expression, Expression::Name) && f.sources.slice(span) == Ok(expected),
            "AST actual read binding reference",
            (argument, span),
        )?;
    }
    Ok(Site {
        factory,
        statement,
        span: node.span(),
        body: *body,
        source: *source,
        source_span,
        bindings: [*key, *value],
        read,
        arguments: [*key_argument, *value_argument],
    })
}
}
