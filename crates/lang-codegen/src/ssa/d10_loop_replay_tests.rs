//! Independent structural replay of existing ordinary Unit SSA. No Carry permission.
use super::{model, unit_lower, unit_lower_test_support, verify::verify_program};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
use std::collections::BTreeSet;

fn replay(
    function: &model::Function,
    control: lang_frontend::source::Span,
    condition_origin: lang_frontend::source::Span,
) -> Option<()> {
    use model::TerminatorKind;

    let headers = function
        .blocks
        .iter()
        .filter(|block| {
            matches!(
                block.terminator.as_ref().map(|t| &t.kind),
                Some(TerminatorKind::Conditional { .. })
            )
        })
        .collect::<Vec<_>>();
    let [header] = headers.as_slice() else {
        return None;
    };
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        condition,
    } = &header.terminator.as_ref()?.kind
    else {
        return None;
    };
    if header.origin != model::Origin::Source(control) {
        return None;
    }
    let body = function.block(when_true.target)?;
    let TerminatorKind::Branch(backedge) = &body.terminator.as_ref()?.kind else {
        return None;
    };
    if backedge.target != header.id || when_false.target == body.id {
        return None;
    }
    let mut predecessors = BTreeSet::new();
    for block in &function.blocks {
        let edges = match &block.terminator.as_ref()?.kind {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            TerminatorKind::Return { .. } => Vec::new(),
            _ => return None,
        };
        for edge in edges {
            let target = function.block(edge.target)?;
            if edge.arguments.len() != target.parameters.len() {
                return None;
            }
            for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                if function.entity(*argument)?.ty != function.entity(*parameter)?.ty {
                    return None;
                }
            }
            if edge.target == header.id {
                predecessors.insert(block.id);
            }
        }
    }
    let entry = function.block(function.entry_block()?)?;
    if predecessors != BTreeSet::from([entry.id, body.id]) {
        return None;
    }
    let [read] = function.instructions.as_slice() else {
        return None;
    };
    let model::Operation::Read {
        source: model::PlaceAccess::Loan(source),
    } = read.operation
    else {
        return None;
    };
    if read.origin != model::Origin::Source(condition_origin)
        || read.block != header.id
        || read.results != vec![model::EntityId::Value(*condition)]
        || header.instructions != vec![read.id]
    {
        return None;
    }
    if function
        .blocks
        .iter()
        .any(|block| block.id != header.id && !block.instructions.is_empty())
    {
        return None;
    }
    let condition = model::EntityId::Loan(source);
    let header_slot = header
        .parameters
        .iter()
        .position(|parameter| *parameter == condition)?;
    let body_slot = when_true
        .arguments
        .iter()
        .position(|argument| *argument == condition)?;
    if backedge.arguments.get(header_slot) != body.parameters.get(body_slot) {
        return None;
    }
    let TerminatorKind::Branch(initial) = &entry.terminator.as_ref()?.kind else {
        return None;
    };
    if entry.parameters.len() != 1 || initial.arguments.get(header_slot) != entry.parameters.first()
    {
        return None;
    }
    // The false edge must reach a normal return, without revisiting body/header.
    let mut visited = BTreeSet::new();
    let mut exit = when_false.target;
    loop {
        if exit == header.id || exit == body.id || !visited.insert(exit) {
            return None;
        }
        match &function.block(exit)?.terminator.as_ref()?.kind {
            TerminatorKind::Branch(edge) => exit = edge.target,
            TerminatorKind::Return { values } if values.is_empty() => {
                visited.extend([entry.id, header.id, body.id]);
                return (visited.len() == function.blocks.len()).then_some(());
            }
            _ => return None,
        }
    }
}

#[test]
fn ordinary_existing_unit_loop_replays_actual_edges_and_rejects_nonbackedge_neighbors() {
    for (body, accepted) in [
        ("while(flag) {}", true),
        ("while(flag) { break }", false),
        ("while(false) {}", false),
    ] {
        let text = format!("fun run(flag: Boolean) {{ {body} }}");
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "loop-replay.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "loop-replay.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let (program, _) = unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
            .unwrap();
            verify_program(&program).unwrap();
            let functions = program
                .modules
                .iter()
                .flat_map(|module| &module.functions)
                .filter(|function| !function.blocks.is_empty())
                .collect::<Vec<_>>();
            assert_eq!(functions.len(), 1);
            let node = parsed
                .ast()
                .statements()
                .iter()
                .find_map(|(_, node)| match node.payload() {
                    lang_frontend::parser::Statement::While { condition, .. } => Some((
                        node.span(),
                        parsed.ast().expressions().get(*condition).unwrap().span(),
                    )),
                    _ => None,
                })
                .unwrap();
            assert_eq!(replay(functions[0], node.0, node.1).is_some(), accepted);
        }
    }
}

mod owner_cleanup;
