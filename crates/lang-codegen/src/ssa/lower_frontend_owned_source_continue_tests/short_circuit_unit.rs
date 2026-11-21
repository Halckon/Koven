use crate::ssa::{
    model::{Edge, EntityId, Operation, TerminatorKind},
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, parsed},
};
use lang_frontend::{
    name_resolution::{SourceUnitInput, SourceUnitKey},
    source::SourceMap,
    type_checking::{complete_compilation_unit_types, standard_environments},
};

#[test]
fn original_live_maps_short_circuit_and_release_on_both_unit_edges() {
    let text = super::ORIGINAL_LIVE_MAP_SHORT_CIRCUIT;
    let path = "lowering.ko";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, path, text);
    assert_eq!(sources.source_text(source), Ok(text));
    let (ne, te) = standard_environments();
    for rooted in [false, true] {
        let inputs = [if rooted {
            SourceUnitInput::new("root", path, source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        complete_compilation_unit_types(&sources, &inputs, &names, &te, typed.types()).unwrap();
        let index = names.names().index();
        let [input] = index.source_units() else {
            panic!("one original input")
        };
        assert_eq!(input.source_id(), source);
        assert_eq!(
            matches!(input.key(), SourceUnitKey::MemoryDocument),
            !rooted
        );
        let declaration = index
            .declarations()
            .iter()
            .find(|decl| decl.name() == "main")
            .unwrap()
            .id();
        let (program, entry) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &te,
            &typed,
            &owned,
            declaration,
        )
        .expect("the preserved original source really is supported by Unit");
        crate::ssa::verify::verify_program(&program).unwrap();
        let module = program.module(entry.module()).unwrap();
        let function = module.function(entry).unwrap();
        assert_eq!(sources.slice(function.origin.span()), Ok(text));
        let queries = function
            .instructions
            .iter()
            .filter(|inst| matches!(inst.operation, Operation::MapContains { .. }))
            .collect::<Vec<_>>();
        let [lhs, rhs] = queries.as_slice() else {
            panic!("the exact source has two Map queries")
        };
        let span =
            |inst: &crate::ssa::model::Instruction| sources.slice(inst.origin.span()).unwrap();
        assert_eq!(span(lhs), "first.contains(3)");
        assert_eq!(span(rhs), "second.contains(3)");
        let Operation::MapContains {
            owner: EntityId::Value(left),
            ..
        } = lhs.operation
        else {
            panic!("left owner is live")
        };
        let [EntityId::Value(predicate)] = lhs.results.as_slice() else {
            panic!("actual left predicate")
        };
        let block = function.block(lhs.block).unwrap();
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &block.terminator.as_ref().unwrap().kind
        else {
            panic!("actual short-circuit fork")
        };
        assert_eq!(condition, predicate);
        assert_eq!(when_true.target, rhs.block);
        assert_ne!(when_false.target, rhs.block);
        assert_eq!(when_true.arguments, when_false.arguments);
        let [second @ EntityId::Value(_)] = when_true.arguments.as_slice() else {
            panic!("only the same live second Map crosses both edges")
        };
        assert_ne!(second, &EntityId::Value(left));
        let transferred = |edge: &Edge| {
            let body = function.block(edge.target).unwrap();
            let [EntityId::Value(map)] = body.parameters.as_slice() else {
                panic!("only the transported Map")
            };
            let instructions = body
                .instructions
                .iter()
                .map(|id| function.instruction(*id).unwrap())
                .collect::<Vec<_>>();
            (body, *map, instructions)
        };
        let (_, second_when_true, taken) = transferred(when_true);
        let (skip, second_when_false, skipped) = transferred(when_false);
        assert!(
            matches!(rhs.operation, Operation::MapContains { owner: EntityId::Value(owner), .. } if owner == second_when_true)
        );
        let [_, query, release] = taken.as_slice() else {
            panic!("second query before releasing its Map")
        };
        assert_eq!(query.id, rhs.id);
        assert_eq!(
            release.operation,
            Operation::Drop {
                owner: second_when_true
            }
        );
        let [release] = skipped.as_slice() else {
            panic!("left false bypasses the rhs entirely")
        };
        assert_eq!(
            release.operation,
            Operation::Drop {
                owner: second_when_false
            }
        );
        assert!(matches!(
            skip.terminator.as_ref().unwrap().kind,
            TerminatorKind::Branch(_)
        ));

        let left_release = block
            .instructions
            .iter()
            .position(|id| {
                function.instruction(*id).unwrap().operation == Operation::Drop { owner: left }
            })
            .unwrap();
        let left_query = block
            .instructions
            .iter()
            .position(|id| *id == lhs.id)
            .unwrap();
        assert!(left_query < left_release);
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        crate::ssa::unit_field_replace_tests::link_and_run(&llvm, "", b"maps\n");
    }
}
