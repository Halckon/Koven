//! Real local formation and Move sources; red observations precede capability changes.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
mod boundaries;
pub(super) use boundaries::formation;
pub(crate) use boundaries::{alias_rebound_source, sources as alternate_sources};
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod facts;
mod graph;
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod owner_flow {
    //! Same-source Map births, full definition inventory and current-owner lifetime on every edge.
    //! Seeds are supplied only after the caller proves the actual declaration/call/ABI identity.
    //! Bounded callers prove each Map RootPlace as an instruction result used by its
    //! actual BorrowBegin. They have no Map Place transport role; this is not a
    //! general alias verifier. Their independent Uses checks remain required.
    use super::super::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, result},
        query_inventory,
    };
    use crate::ssa::model::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct State {
        current: BTreeMap<EntityId, EntityId>,
        places: BTreeMap<EntityId, (EntityId, EntityId)>,
        loans: BTreeMap<EntityId, EntityId>,
        retired: BTreeSet<EntityId>,
    }

    fn map_entity(module: &Module, graph: &Graph<'_>, entity: EntityId) -> bool {
        graph
            .function
            .entity(entity)
            .is_some_and(|data| module.map_container(data.ty.semantic_type()).is_some())
    }

    fn non_reentrant_birth(graph: &Graph<'_>, birth: BlockId) -> Result<(), Failure> {
        let mut pending = graph
            .edges
            .iter()
            .filter_map(|(from, edge)| (*from == birth).then_some(edge.target))
            .collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        while let Some(block) = pending.pop() {
            need(
                block != birth,
                "Unit owner flow selected birth cannot execute on a backedge",
                birth,
            )?;
            if seen.insert(block) {
                pending.extend(
                    graph
                        .edges
                        .iter()
                        .filter_map(|(from, edge)| (*from == block).then_some(edge.target)),
                );
            }
        }
        Ok(())
    }

    fn consume(state: &mut State, owner: EntityId) -> Result<(), Failure> {
        let seeds = state
            .current
            .iter()
            .filter_map(|(&seed, &current)| (current == owner).then_some(seed))
            .collect::<Vec<_>>();
        let [seed] = seeds.as_slice() else {
            return Err(failure(
                "Unit owner flow consumes exactly one live epoch",
                (owner, state),
            ));
        };
        need(
            !state.loans.values().any(|active| active == seed) && state.retired.insert(*seed),
            "Unit owner flow retires current epoch once after every Map loan ends",
            (owner, *seed),
        )?;
        state.current.remove(seed);
        state.places.retain(|_, (epoch, _)| epoch != seed);
        Ok(())
    }

    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn check(
        graph: &Graph<'_>,
        module: &Module,
        seeds: &BTreeSet<EntityId>,
    ) -> Result<(), Failure> {
        need(
            !seeds.is_empty(),
            "Unit owner flow has independently proven births",
            seeds,
        )?;
        let entry = graph
            .function
            .entry_block()
            .ok_or_else(|| failure("Unit owner flow real entry", graph.function.id))?;
        need(
            !graph
                .function
                .return_types
                .iter()
                .any(|&ty| module.map_container(ty).is_some()),
            "Unit owner flow bounded callers do not return owning Map values",
            &graph.function.return_types,
        )?;
        for &block in &graph.blocks {
            need(
                !graph.block(block)?.parameters.iter().any(|&parameter| {
                    matches!(parameter, EntityId::Place(_)) && map_entity(module, graph, parameter)
                }),
                "Unit owner flow bounded roles have no Map Place block parameter",
                block,
            )?;
        }
        let mut types = BTreeSet::new();
        let mut initial = State {
            current: BTreeMap::new(),
            places: BTreeMap::new(),
            loans: BTreeMap::new(),
            retired: BTreeSet::new(),
        };
        let mut proven = BTreeSet::new();
        for &seed in seeds {
            let data = graph
                .function
                .entity(seed)
                .ok_or_else(|| failure("Unit owner flow actual birth definition", seed))?;
            need(
                matches!(seed, EntityId::Value(_)) && map_entity(module, graph, seed),
                "Unit owner flow birth is a real owning Map value",
                (seed, data),
            )?;
            types.insert(data.ty.semantic_type());
            match data.definition {
                Definition::BlockParameter { block, index } => {
                    need(
                        block == entry && graph.block(entry)?.parameters.get(index) == Some(&seed),
                        "Unit owner flow original owning entry slot",
                        (seed, data),
                    )?;
                    non_reentrant_birth(graph, entry)?;
                    initial.current.insert(seed, seed);
                    proven.insert(seed);
                }
                Definition::InstructionResult { .. } => {
                    let birth = graph.definition(seed)?;
                    non_reentrant_birth(graph, birth.block)?;
                }
            }
        }
        let mut pending = vec![(entry, initial)];
        let mut seen = BTreeSet::new();
        let mut returns = 0;
        while let Some((block, mut state)) = pending.pop() {
            if !seen.insert((block, state.clone())) {
                continue;
            }
            need(
                seen.len() < 1000,
                "Unit owner flow finite full path states",
                seen.len(),
            )?;
            for instruction in graph.ops(block)? {
                let mut produced = BTreeSet::new();
                for operand in instruction.operation.entities() {
                    if !map_entity(module, graph, operand) {
                        continue;
                    }
                    let live = match operand {
                        EntityId::Value(_) => state.current.values().any(|&current| current == operand),
                        EntityId::Loan(_) => state.loans.contains_key(&operand),
                        EntityId::Place(_) => state
                            .places
                            .get(&operand)
                            .is_some_and(|(seed, owner)| state.current.get(seed) == Some(owner)),
                    };
                    need(
                        live,
                        "Unit owner flow every Map operand is its live current definition",
                        (instruction, operand, &state),
                    )?;
                }
                match &instruction.operation {
                    Operation::RootPlace { owner }
                        if map_entity(module, graph, EntityId::Value(*owner)) =>
                    {
                        let owner = EntityId::Value(*owner);
                        let seed = *state
                            .current
                            .iter()
                            .find(|(_, current)| **current == owner)
                            .ok_or_else(|| failure("Unit owner flow Root current birth", owner))?
                            .0;
                        let place = result(instruction)?;
                        need(
                            matches!(place, EntityId::Place(_)),
                            "Unit owner flow real Root place",
                            place,
                        )?;
                        state.places.insert(place, (seed, owner));
                        proven.insert(place);
                        produced.insert(place);
                    }
                    Operation::BorrowBegin { place, .. }
                        if map_entity(module, graph, EntityId::Place(*place)) =>
                    {
                        let (seed, _) = *state
                            .places
                            .get(&EntityId::Place(*place))
                            .ok_or_else(|| failure("Unit owner flow real Map borrow root", place))?;
                        let loan = result(instruction)?;
                        need(
                            matches!(loan, EntityId::Loan(_))
                                && state.loans.insert(loan, seed).is_none(),
                            "Unit owner flow each actual Map lease begins once",
                            loan,
                        )?;
                        proven.insert(loan);
                        produced.insert(loan);
                    }
                    Operation::SharedReborrow { source }
                        if map_entity(module, graph, EntityId::Loan(*source)) =>
                    {
                        let seed = *state
                            .loans
                            .get(&EntityId::Loan(*source))
                            .ok_or_else(|| failure("Unit owner flow reborrow live parent", source))?;
                        let loan = result(instruction)?;
                        need(
                            matches!(loan, EntityId::Loan(_))
                                && state.loans.insert(loan, seed).is_none(),
                            "Unit owner flow independent actual Map reborrow",
                            loan,
                        )?;
                        proven.insert(loan);
                        produced.insert(loan);
                    }
                    Operation::BorrowEnd { loan }
                        if map_entity(module, graph, EntityId::Loan(*loan)) =>
                    {
                        need(
                            state.loans.remove(&EntityId::Loan(*loan)).is_some(),
                            "Unit owner flow exact active Map lease ends once",
                            instruction,
                        )?;
                    }
                    Operation::Drop { owner }
                    | Operation::MapPut { owner, .. }
                    | Operation::MapConsume { owner, .. }
                        if map_entity(module, graph, EntityId::Value(*owner)) =>
                    {
                        consume(&mut state, EntityId::Value(*owner))?;
                    }
                    Operation::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    } => {
                        let target = module
                            .function(*callee)
                            .ok_or_else(|| failure("Unit owner flow actual selected callee", callee))?;
                        let parameters = &target
                            .block(
                                target
                                    .entry_block()
                                    .ok_or_else(|| failure("Unit owner flow callee entry", callee))?,
                            )
                            .ok_or_else(|| failure("Unit owner flow real callee entry block", callee))?
                            .parameters;
                        let actual = receiver
                            .iter()
                            .copied()
                            .chain(arguments.iter().copied())
                            .collect::<Vec<_>>();
                        need(
                            actual.len() == parameters.len(),
                            "Unit owner flow complete selected call ABI",
                            instruction,
                        )?;
                        for (&argument, &parameter) in actual.iter().zip(parameters) {
                            need(
                                graph.function.entity(argument).map(|data| data.ty)
                                    == target.entity(parameter).map(|data| data.ty),
                                "Unit owner flow argument matches actual callee parameter mode and type",
                                instruction,
                            )?;
                            if matches!(argument, EntityId::Value(_))
                                && map_entity(module, graph, argument)
                            {
                                consume(&mut state, argument)?;
                            }
                        }
                    }
                    _ => {}
                }
                for &entity in &instruction.results {
                    if !map_entity(module, graph, entity) {
                        continue;
                    }
                    if seeds.contains(&entity) {
                        need(
                            !state.retired.contains(&entity)
                                && state.current.insert(entity, entity).is_none(),
                            "Unit owner flow unique independently selected birth",
                            instruction,
                        )?;
                        proven.insert(entity);
                    } else {
                        need(
                            produced.contains(&entity),
                            "Unit owner flow excludes an unproved Map producer/capability",
                            instruction,
                        )?;
                    }
                }
            }
            match &graph.term(block)?.kind {
                TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. } => {
                    for (_, edge) in graph.edges.iter().filter(|(from, _)| *from == block) {
                        let mut successor = state.clone();
                        let mut columns = BTreeSet::new();
                        for (&argument, &parameter) in edge
                            .arguments
                            .iter()
                            .zip(&graph.block(edge.target)?.parameters)
                        {
                            if !map_entity(module, graph, argument) {
                                continue;
                            }
                            need(
                                columns.insert(argument),
                                "Unit owner flow never duplicates an edge capability",
                                edge,
                            )?;
                            match argument {
                                EntityId::Value(_) => {
                                    let seed = *state.current.iter().find(|(_, current)| **current == argument)
                                        .ok_or_else(|| failure("Unit owner flow no retired/foreign owner in any incoming", (edge, argument, &state)))?.0;
                                    successor.current.insert(seed, parameter);
                                }
                                EntityId::Loan(_) => {
                                    let seed = *state.loans.get(&argument).ok_or_else(|| {
                                        failure(
                                            "Unit owner flow no ended/foreign lease in any incoming",
                                            (edge, argument, &state),
                                        )
                                    })?;
                                    successor.loans.remove(&argument);
                                    successor.loans.insert(parameter, seed);
                                }
                                EntityId::Place(_) => {
                                    return Err(failure(
                                        "Unit owner flow excludes unproved Map Place transport",
                                        (edge, argument, parameter),
                                    ));
                                }
                            }
                            proven.insert(parameter);
                        }
                        pending.push((edge.target, successor));
                    }
                }
                TerminatorKind::Return { .. } => {
                    need(
                        state.current.is_empty() && state.loans.is_empty(),
                        "Unit owner flow every normal Return retires all original epochs and leases",
                        (graph.term(block)?, &state),
                    )?;
                    returns += 1;
                }
                TerminatorKind::Abort => {}
                other => {
                    return Err(failure(
                        "Unit owner flow original bounded terminator",
                        other,
                    ));
                }
            }
        }
        need(
            returns > 0 && seeds.is_subset(&proven),
            "Unit owner flow visits every original birth and normal completion",
            (&proven, seeds),
        )?;
        query_inventory::check(graph, &types.into_iter().collect::<Vec<_>>(), &proven)
    }
}
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub mutable: bool,
    pub form: usize,
    pub exit: usize,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for form in 0..3 {
            for exit in 0..4 {
                let kind = if mutable { "MutableMap" } else { "Map" };
                let conversion = if mutable {
                    "return source"
                } else {
                    "val result=source.consume();return result"
                };
                let factory = if mutable { "mutableMapOf" } else { "mapOf" };
                let transfer = ["", "continue", "break", "return result(key,value)"][exit];
                let mut scans = String::new();
                for empty in [false, true] {
                    let scan = if empty { "scanEmpty" } else { "scan" };
                    let producer = if empty { "makeEmpty" } else { "make" };
                    let declaration = match form {
                        0 => format!("val source={producer}()"),
                        1 => format!("var source={producer}()"),
                        _ => format!("val original={producer}();val source=original"),
                    };
                    scans.push_str(&format!(
                        "fun {scan}():Int{{{declaration};println(\"formed\");for((key,value) in source){{val local=Guard();read(key,value);{transfer}}};println(\"after\");return 0}}\n"
                    ));
                }
                let source = format!(
                    r#"class Guard{{deinit(){{println("guard-drop")}}}}
fun read(key:String,value:String):Unit{{println(key);println(value)}}
fun result(key:String,value:String):Int{{println("return-value");return 7}}
fun make():{kind}<String,String>{{var source=mutableMapOf<String,String>();source.put("z".clone(),"first".clone());source.put("a".clone(),"second".clone());{conversion}}}
fun makeEmpty():{kind}<String,String>{{return {factory}<String,String>()}}
{scans}
fun entry():Unit{{println("nonempty");val first=scan();if(first==7){{println("seven")}}else{{println("zero")}};println("empty");val second=scanEmpty();if(second==7){{println("seven")}}else{{println("zero")}};println("entry-end")}}
"#
                );
                let mut golden = String::from("nonempty\nformed\nz\nfirst\n");
                if exit == 3 {
                    golden.push_str("return-value\n");
                }
                golden.push_str("guard-drop\n");
                if exit < 2 {
                    golden.push_str("a\nsecond\nguard-drop\n");
                }
                if exit == 3 {
                    golden.push_str("seven\n");
                } else {
                    golden.push_str("after\nzero\n");
                }
                golden.push_str("empty\nformed\nafter\nzero\nentry-end\n");
                cases.push(Case {
                    label: format!(
                        "local-{kind}-{}-{}",
                        ["val", "var", "moved-alias"][form],
                        ["normal", "continue", "break", "return"][exit]
                    ),
                    source,
                    golden,
                    mutable,
                    form,
                    exit,
                });
            }
        }
    }
    assert_eq!(cases.len(), 24);
    assert_eq!(
        cases
            .iter()
            .map(|c| &c.label)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        24
    );
    cases
}

fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "G2 INPUT case={} unit={unit} source={:?} golden={:?}",
        case.label,
        case.source,
        case.golden
    );
    // The locked metadata belongs to source construction; identity queries use actual AST/FE.
    diagnostic!(
        "G2 FORM mutable={} form={} exit={}",
        case.mutable,
        case.form,
        case.exit
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", &case.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let result = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                facts::unit(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "G2 exactly two original qualified Map descriptors",
                    typed.map_iterations().len() == 2,
                    typed.map_iterations(),
                );
                diagnostic!(
                    "G2 UNIT ORIGINAL formed={:#?} plans={:#?} drops={:#?} deliveries={:#?}",
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.value_deliveries()
                );
            },
        )
    } else {
        pipeline::memory_document_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                facts::unit(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "G2 exactly two original qualified Map descriptors",
                    typed.map_iterations().len() == 2,
                    typed.map_iterations(),
                );
                diagnostic!(
                    "G2 UNIT ORIGINAL formed={:#?} plans={:#?} drops={:#?} deliveries={:#?}",
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.value_deliveries()
                );
            },
        )
    };
    diagnostic!("G2 REPORT {} unit={unit} {report:#?}", case.label);
    let (program, _) = result?;
    report.query(
        "Unit complete temporary source acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    if !report.passed() {
        return Err(failure("original FE query", report));
    }
    graph::check(case, &sources, &parsed, &program)?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}

#[test]
fn owned_locals_follow_real_formation_and_move() {
    let mut outcomes = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            diagnostic!("G2 CASE {} unit={unit} result={result:?}", case.label);
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert_diagnostic!(
        outcomes.len() == 48 && outcomes.iter().all(|(_, _, r)| r.is_ok()),
        "{outcomes:#?}"
    );
}
