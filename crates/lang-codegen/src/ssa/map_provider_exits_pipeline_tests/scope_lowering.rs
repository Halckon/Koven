//! A1 frozen scope programs: explicit no-For controls and complete ordinary provider identities.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast {
    //! Actual scope, parameter, caller and body syntax from each frozen source.
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use super::Case;
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        parser::{
            Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile, Statement,
        },
        source::{SourceMap, Span},
    };
    #[derive(Debug)]
    pub(super) struct Function {
        pub span: Span,
        pub name: Span,
        pub parameters: Vec<Span>,
        pub body: StatementId,
    }
    #[derive(Debug)]
    pub(super) struct Call {
        pub id: ExpressionId,
        pub arguments: Vec<ExpressionId>,
    }
    #[derive(Debug)]
    pub(super) struct Site {
        pub statement: StatementId,
        pub span: Span,
        pub source: ExpressionId,
        pub body: StatementId,
        pub bindings: [Span; 2],
    }
    #[derive(Debug)]
    pub(super) struct Input {
        pub scan: Function,
        pub entry: Function,
        pub reader: Option<Function>,
        pub observe: Option<Function>,
        pub make: Option<Function>,
        pub count: Option<Function>,
        pub callers: Vec<Call>,
        pub site: Option<Site>,
        pub read: Option<Call>,
        pub observe_call: Option<Call>,
        pub count_call: Option<Call>,
        pub assignment: Option<ExpressionId>,
        pub clone: Option<ExpressionId>,
        pub cloned: Option<Span>,
        pub clone_print: Option<Call>,
        pub early_return: Option<ExpressionId>,
    }
    pub(super) fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
        parsed.ast().expressions().get(id).unwrap().span()
    }
    pub(super) fn inside(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() < inner.start()
            && inner.end() < outer.end()
    }
    pub(super) fn function(
        sources: &SourceMap,
        parsed: &ParsedFile,
        wanted: &str,
    ) -> Result<Function, Failure> {
        let n=one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok()).filter(|n|matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..}if sources.slice(*name)==Ok(wanted))),"scope actual original top-level function")?;
        let Item::Function {
            name: NameMarker::Present(name),
            parameters,
            form,
            ..
        } = n.payload()
        else {
            unreachable!()
        };
        let parameters = parameters
            .iter()
            .map(|p| match p.name {
                NameMarker::Present(name) => Ok(name),
                _ => Err(failure("scope actual named parameter", p)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let body = match form {
            FunctionForm::ImplicitUnitBlock(b)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(b),
                ..
            } => *b,
            _ => return Err(failure("scope original callable block", form)),
        };
        Ok(Function {
            span: n.span(),
            name: *name,
            parameters,
            body,
        })
    }
    pub(super) fn calls(
        sources: &SourceMap,
        parsed: &ParsedFile,
        scope: Span,
        wanted: &str,
    ) -> Vec<Call> {
        let mut calls = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::Call {
                    callee, arguments, ..
                } if inside(scope, n.span()) && sources.slice(span(parsed, *callee)) == Ok(wanted) => {
                    Some(Call {
                        id,
                        arguments: arguments.iter().map(|a| a.value).collect(),
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        calls.sort_by_key(|c| span(parsed, c.id).start());
        calls
    }
    fn one_call(
        sources: &SourceMap,
        parsed: &ParsedFile,
        scope: Span,
        wanted: &str,
    ) -> Result<Call, Failure> {
        one(
            calls(sources, parsed, scope, wanted).into_iter(),
            "scope exact original Call",
        )
    }
    pub(super) fn input(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
    ) -> Result<Input, Failure> {
        let scan = function(sources, parsed, "scan")?;
        let entry = function(sources, parsed, "entry")?;
        let callers = calls(sources, parsed, entry.span, "scan");
        need(
            callers.len() == if case.gate == 2 { 1 } else { 2 }
                && scan.parameters.len() == if case.gate == 2 { 0 } else { 2 },
            "scope exact original entry invocations and scanner ABI",
            &callers,
        )?;
        if case.gate != 2 {
            for (c, wanted) in callers.iter().zip(["true", "false"]) {
                let [map, flag] = c.arguments.as_slice() else {
                    return Err(failure("scope actual two scan arguments", c));
                };
                need(
                    sources.slice(span(parsed, *flag)) == Ok(wanted)
                        && sources.slice(span(parsed, *map)) == Ok("make()"),
                    "scope original entry encodes true/false with real make calls",
                    c,
                )?;
            }
        }
        let sites = parsed
            .ast()
            .statements()
            .iter()
            .filter_map(|(statement, n)| match n.payload() {
                Statement::For {
                    binding: ForBinding::Destructuring { names, .. },
                    source,
                    body,
                    ..
                } if inside(scan.span, n.span()) => Some((statement, n, names, *source, *body)),
                _ => None,
            })
            .collect::<Vec<_>>();
        need(
            sites.len() == usize::from(case.provider),
            "scope original map For/control distinction",
            sites.len(),
        )?;
        let site = if let Some((statement, n, names, source, body)) = sites.first() {
            let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
                return Err(failure("scope actual K/V markers", names));
            };
            Some(Site {
                statement: *statement,
                span: n.span(),
                source: *source,
                body: *body,
                bindings: [*key, *value],
            })
        } else {
            None
        };
        let early_return = if case.gate == 2 {
            Some(one(
                parsed.ast().expressions().iter().filter_map(|(id, n)| {
                    (inside(scan.span, n.span())
                        && matches!(n.payload(), Expression::Return { value: None, .. }))
                    .then_some(id)
                }),
                "scope dead-tail preceding Return",
            )?)
        } else {
            None
        };
        if let (Some(ret), Some(site)) = (early_return, &site) {
            need(
                span(parsed, ret).end() < site.span.start(),
                "scope legal Return precedes unacquired source",
                (ret, site),
            )?;
        }
        let (reader, observe, make) = if case.gate == 2 {
            (None, None, None)
        } else {
            (
                Some(function(sources, parsed, "read")?),
                Some(function(sources, parsed, "observe")?),
                Some(function(sources, parsed, "make")?),
            )
        };
        let read = if case.gate == 17 || case.gate == 18 || case.provider && case.gate != 2 {
            Some(one_call(sources, parsed, scan.span, "read")?)
        } else {
            None
        };
        let observe_call = if !case.provider && matches!(case.gate, 12 | 13 | 14 | 16) {
            Some(one_call(sources, parsed, scan.span, "observe")?)
        } else {
            None
        };
        let (count, count_call, assignment) = if case.gate == 17 {
            let assignment = one(
                parsed.ast().expressions().iter().filter_map(|(id, n)| {
                    (inside(scan.span, n.span()) && sources.slice(n.span()) == Ok("n=n+1"))
                        .then_some(id)
                }),
                "scope actual scalar assignment",
            )?;
            (
                Some(function(sources, parsed, "checkCount")?),
                Some(one_call(sources, parsed, scan.span, "checkCount")?),
                Some(assignment),
            )
        } else {
            (None, None, None)
        };
        let (clone, cloned, clone_print) = if case.gate == 18 {
            let (marker, value) = one(
                parsed
                    .ast()
                    .items()
                    .iter()
                    .filter_map(|(_, n)| match n.payload() {
                        Item::Variable {
                            name: NameMarker::Present(name),
                            initializer,
                            ..
                        } if inside(scan.span, n.span()) && sources.slice(*name) == Ok("cloned") => {
                            Some((*name, *initializer))
                        }
                        _ => None,
                    }),
                "scope original independent cloned local",
            )?;
            (
                Some(value),
                Some(marker),
                Some(one_call(sources, parsed, scan.span, "println")?),
            )
        } else {
            (None, None, None)
        };
        Ok(Input {
            scan,
            entry,
            reader,
            observe,
            make,
            count,
            callers,
            site,
            read,
            observe_call,
            count_call,
            assignment,
            clone,
            cloned,
            clone_print,
            early_return,
        })
    }
}
mod facts;
mod graph;
mod sources {
    //! A1 references fourteen frozen originals; it does not register later scope groups.
    pub(crate) struct Case {
        pub label: &'static str,
        pub gate: u8,
        pub provider: bool,
        pub source: &'static str,
        pub golden: &'static str,
    }
    macro_rules! pair {
        ($gate:literal,$id:literal) => {
            [
                Case {
                    label: concat!($id, "-for"),
                    gate: $gate,
                    provider: true,
                    source: include_str!(concat!(
                        "../../../../tests/fixtures/map-provider-scope-0331/",
                        $id,
                        "-for.ko"
                    )),
                    golden: include_str!(concat!(
                        "../../../../tests/fixtures/map-provider-scope-0331/",
                        $id,
                        "-for.stdout"
                    )),
                },
                Case {
                    label: concat!($id, "-control"),
                    gate: $gate,
                    provider: false,
                    source: include_str!(concat!(
                        "../../../../tests/fixtures/map-provider-scope-0331/",
                        $id,
                        "-control.ko"
                    )),
                    golden: include_str!(concat!(
                        "../../../../tests/fixtures/map-provider-scope-0331/",
                        $id,
                        "-control.stdout"
                    )),
                },
            ]
        };
    }
    pub(crate) fn sources() -> Vec<Case> {
        [
            pair!(2, "P02"),
            pair!(12, "P12"),
            pair!(13, "P13"),
            pair!(14, "P14"),
            pair!(16, "P16"),
            pair!(17, "P17"),
            pair!(18, "P18"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) use sources::{Case, sources};
fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "SCOPE A1 INPUT label={} rooted={unit} source={:?} golden={:?}",
        case.label,
        case.source,
        case.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", case.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let input = ast::input(case, &sources, &parsed)?;
    let mut report = Report::default();
    let lowered = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |n, t, o, r| {
            r.query(
                "scope A1 qualified actual owner/parameter/body/complete cleanup",
                &facts::unit((case, &sources, &parsed, &input), n, t, o),
            );
        },
    );
    diagnostic!(
        "SCOPE A1 LOWER label={} rooted={unit} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    let (program, _) = lowered?;
    report.query(
        "scope A1 independent SSA all incoming/current loans/protected uses/endpoints",
        &graph::check(case, &sources, &parsed, &input, &program),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    diagnostic!(
        "SCOPE A1 FINAL label={} rooted={unit} report={report:#?}",
        case.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("scope A1 independent identity queries", report))
    }
}
#[test]
fn a1_original_scope_sources_preserve_actual_identity() {
    let mut outcomes = Vec::new();
    let cases = sources();
    assert_eq!(cases.len(), 14);
    for c in cases {
        for unit in [false, true] {
            let r = observe(&c, unit);
            diagnostic!("SCOPE A1 CASE label={} rooted={unit} result={r:?}", c.label);
            outcomes.push((c.label, unit, r));
        }
    }
    assert_eq!(outcomes.len(), 28);
    assert_diagnostic!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}

pub(in crate::ssa) mod query_graph {
//! 小型只读图查询：真实 block 引用、全 incoming 和 operand 对应列。
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use crate::ssa::model::*;
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::ssa) struct Graph<'a> {
    pub function: &'a Function,
    pub blocks: Vec<BlockId>,
    pub edges: Vec<(BlockId, &'a Edge)>,
    pub instructions: Vec<&'a Instruction>,
}

impl<'a> Graph<'a> {
    pub fn new(function: &'a Function) -> Result<Self, Failure> {
        Self::collect(function, false)
    }

    /// Local later-use fixtures contain real `error` calls after size failure.
    /// Keep the existing graph entry strict for all other identity queries.
    pub fn with_abort(function: &'a Function) -> Result<Self, Failure> {
        Self::collect(function, true)
    }

    fn collect(function: &'a Function, allow_abort: bool) -> Result<Self, Failure> {
        let mut graph = Self {
            function,
            blocks: vec![],
            edges: vec![],
            instructions: vec![],
        };
        let mut pending = vec![
            function
                .entry_block()
                .ok_or_else(|| failure("identity SSA entry", function.id))?,
        ];
        let mut seen_instructions = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if graph.blocks.contains(&id) {
                continue;
            }
            let block = graph.block(id)?;
            graph.blocks.push(id);
            for &instruction in &block.instructions {
                let i = function
                    .instruction(instruction)
                    .ok_or_else(|| failure("identity SSA referenced instruction", instruction))?;
                need(
                    i.block == id && seen_instructions.insert(instruction),
                    "identity SSA unique instruction membership",
                    i,
                )?;
                for (index, &result) in i.results.iter().enumerate() {
                    need(
                        function.entity(result).is_some_and(|data| {
                            data.definition == Definition::InstructionResult { instruction, index }
                        }),
                        "identity SSA actual result definition",
                        (i, result),
                    )?;
                }
                graph.instructions.push(i);
            }
            let term = graph.term(id)?;
            let edges = match &term.kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => vec![when_true, when_false],
                TerminatorKind::Return { .. } => vec![],
                TerminatorKind::Abort if allow_abort => vec![],
                _ => return Err(failure("identity SSA unexpected fixture terminator", term)),
            };
            for edge in edges {
                let target = graph.block(edge.target)?;
                need(
                    edge.arguments.len() == target.parameters.len(),
                    "identity SSA every edge full arity",
                    edge,
                )?;
                for (index, (&argument, &parameter)) in
                    edge.arguments.iter().zip(&target.parameters).enumerate()
                {
                    let a = function
                        .entity(argument)
                        .ok_or_else(|| failure("identity SSA incoming argument", argument))?;
                    let p = function
                        .entity(parameter)
                        .ok_or_else(|| failure("identity SSA incoming parameter", parameter))?;
                    need(
                        a.ty == p.ty
                            && p.definition
                                == Definition::BlockParameter {
                                    block: target.id,
                                    index,
                                },
                        "identity SSA every incoming definition/type",
                        (edge, parameter),
                    )?;
                }
                graph.edges.push((id, edge));
                pending.push(edge.target);
            }
        }
        Ok(graph)
    }

    pub fn block(&self, id: BlockId) -> Result<&'a Block, Failure> {
        self.function
            .block(id)
            .ok_or_else(|| failure("identity SSA block", id))
    }
    pub fn term(&self, id: BlockId) -> Result<&'a Terminator, Failure> {
        self.block(id)?
            .terminator
            .as_ref()
            .ok_or_else(|| failure("identity SSA terminator", id))
    }
    pub fn branch(&self, id: BlockId) -> Result<&'a Edge, Failure> {
        match &self.term(id)?.kind {
            TerminatorKind::Branch(edge) => Ok(edge),
            term => Err(failure("identity SSA actual unconditional branch", term)),
        }
    }
    pub fn ops(&self, id: BlockId) -> Result<Vec<&'a Instruction>, Failure> {
        self.block(id)?
            .instructions
            .iter()
            .map(|&id| {
                self.function
                    .instruction(id)
                    .ok_or_else(|| failure("identity SSA block instruction", id))
            })
            .collect()
    }
    pub fn incoming(&self, target: BlockId) -> Vec<(BlockId, &'a Edge)> {
        self.edges
            .iter()
            .copied()
            .filter(|(_, edge)| edge.target == target)
            .collect()
    }
    pub fn incoming_from(&self, target: BlockId, expected: &[BlockId]) -> Result<(), Failure> {
        let actual = self
            .incoming(target)
            .iter()
            .map(|(from, _)| *from)
            .collect::<Vec<_>>();
        need(
            actual.len() == expected.len()
                && actual.iter().copied().collect::<BTreeSet<_>>()
                    == expected.iter().copied().collect::<BTreeSet<_>>(),
            "identity SSA complete incoming predecessor set",
            (target, actual, expected),
        )
    }
    pub fn transported(&self, edge: &Edge, input: EntityId) -> Result<EntityId, Failure> {
        let slot = one(
            edge.arguments
                .iter()
                .enumerate()
                .filter_map(|(i, &v)| (v == input).then_some(i)),
            "identity SSA unique actual transport column",
        )?;
        self.block(edge.target)?
            .parameters
            .get(slot)
            .copied()
            .ok_or_else(|| failure("identity SSA target parameter slot", (edge, slot)))
    }
    pub fn argument(&self, edge: &Edge, parameter: EntityId) -> Result<EntityId, Failure> {
        let data = self
            .function
            .entity(parameter)
            .ok_or_else(|| failure("identity SSA transport entity", parameter))?;
        let Definition::BlockParameter { block, index } = data.definition else {
            return Err(failure("identity SSA transport target is parameter", data));
        };
        need(
            block == edge.target,
            "identity SSA actual transport target",
            (edge, data),
        )?;
        edge.arguments
            .get(index)
            .copied()
            .ok_or_else(|| failure("identity SSA incoming slot", edge))
    }
    pub fn definition(&self, entity: EntityId) -> Result<&'a Instruction, Failure> {
        let data = self
            .function
            .entity(entity)
            .ok_or_else(|| failure("identity SSA entity definition", entity))?;
        let Definition::InstructionResult { instruction, index } = data.definition else {
            return Err(failure("identity SSA instruction result required", data));
        };
        let i = self
            .function
            .instruction(instruction)
            .ok_or_else(|| failure("identity SSA result producer", instruction))?;
        need(
            i.results.get(index) == Some(&entity)
                && self.instructions.iter().any(|v| v.id == instruction),
            "identity SSA real reachable producer",
            i,
        )?;
        Ok(i)
    }
    /// Every visited incoming node must reach an actual source anchor. A cycle
    /// cannot inherit proof merely because a different incoming reaches it.
    pub fn anchored(&self, current: EntityId, anchors: &[EntityId]) -> Result<(), Failure> {
        let mut pending = vec![current];
        let mut seen = BTreeSet::new();
        let mut dependencies = Vec::new();
        while let Some(entity) = pending.pop() {
            if anchors.contains(&entity) || !seen.insert(entity) {
                continue;
            }
            let data = self
                .function
                .entity(entity)
                .ok_or_else(|| failure("identity SSA transported entity", entity))?;
            let Definition::BlockParameter { block, .. } = data.definition else {
                return Err(failure(
                    "identity SSA transport reaches another producer",
                    data,
                ));
            };
            let incoming = self.incoming(block);
            need(
                !incoming.is_empty(),
                "identity SSA complete incoming exists",
                entity,
            )?;
            for (_, edge) in incoming {
                let argument = self.argument(edge, entity)?;
                dependencies.push((entity, argument));
                pending.push(argument);
            }
        }
        let mut reached = anchors.iter().copied().collect::<BTreeSet<_>>();
        loop {
            let before = reached.len();
            for &(entity, argument) in &dependencies {
                if reached.contains(&argument) {
                    reached.insert(entity);
                }
            }
            if before == reached.len() {
                break;
            }
        }
        need(
            seen.is_subset(&reached),
            "identity SSA every incoming node anchored",
            (current, seen, reached),
        )
    }

    pub fn position(&self, instruction: &Instruction) -> Result<usize, Failure> {
        one(
            self.block(instruction.block)?
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(i, id)| (*id == instruction.id).then_some(i)),
            "identity SSA instruction position",
        )
    }
}

pub(in crate::ssa) fn result(i: &Instruction) -> Result<EntityId, Failure> {
    one(
        i.results.iter().copied(),
        "identity SSA unique operation result",
    )
}
pub(in crate::ssa) fn value(i: &Instruction) -> Result<ValueId, Failure> {
    match result(i)? {
        EntityId::Value(id) => Ok(id),
        e => Err(failure("identity SSA Value result", e)),
    }
}
pub(in crate::ssa) fn loan(i: &Instruction) -> Result<LoanId, Failure> {
    match result(i)? {
        EntityId::Loan(id) => Ok(id),
        e => Err(failure("identity SSA Loan result", e)),
    }
}
pub(in crate::ssa) fn function(
    module: &Module,
    span: lang_frontend::source::Span,
) -> Result<&Function, Failure> {
    one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(span)),
        "identity SSA original source function",
    )
}

/// A role that an edge does not transport must already dominate the target block.
/// Refinement SSA keeps such loans as ordinary dominating definitions instead of
/// threading every carrier through every merge.
pub(in crate::ssa) fn inherits(
    graph: &Graph<'_>,
    entity: EntityId,
    target: BlockId,
) -> Result<bool, Failure> {
    let data = graph
        .function
        .entity(entity)
        .ok_or_else(|| failure("identity SSA inherited role entity", entity))?;
    let definition = match data.definition {
        Definition::BlockParameter { block, .. } => block,
        Definition::InstructionResult { instruction, .. } => {
            graph
                .function
                .instruction(instruction)
                .ok_or_else(|| failure("identity SSA inherited role producer", instruction))?
                .block
        }
    };
    if definition == target {
        return Ok(false);
    }
    let dominators = crate::ssa::verify::compute_dominators(graph.function);
    Ok(dominators[target.index()].contains(&definition.index()))
}

#[derive(Default)]
pub(in crate::ssa) struct Uses(BTreeMap<EntityId, BTreeSet<InstructionId>>);

impl Uses {
    pub fn protect(&mut self, entity: EntityId) {
        self.0.entry(entity).or_default();
    }
    pub fn permit(&mut self, entity: EntityId, instruction: &Instruction) {
        self.0.entry(entity).or_default().insert(instruction.id);
    }
    pub fn check(
        &self,
        graph: &Graph<'_>,
        returned: Option<(BlockId, ValueId)>,
    ) -> Result<(), Failure> {
        for instruction in &graph.instructions {
            for entity in instruction.operation.entities() {
                if let Some(allowed) = self.0.get(&entity) {
                    need(
                        allowed.contains(&instruction.id),
                        "identity SSA complete protected operand uses",
                        (entity, instruction),
                    )?;
                }
            }
        }
        for &block in &graph.blocks {
            let term = graph.term(block)?;
            for entity in term.kind.entities() {
                if self.0.contains_key(&entity) {
                    let allowed = match (&term.kind, entity) {
                        (TerminatorKind::Return { values }, EntityId::Value(v)) => {
                            returned == Some((block, v)) && values.as_slice() == [v]
                        }
                        (TerminatorKind::Branch(_), _)
                        | (TerminatorKind::Conditional { .. }, _) => graph
                            .edges
                            .iter()
                            .any(|(from, edge)| *from == block && edge.arguments.contains(&entity)),
                        _ => false,
                    };
                    need(
                        allowed,
                        "identity SSA protected terminal operands only proven edge/Return",
                        (entity, term),
                    )?;
                }
            }
        }
        Ok(())
    }
}
}
