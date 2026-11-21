//! Exact original non-returning equality LHS, shared with the frontend Single witness.
use super::{
    Operation, SourceMap, SourceUnitInput, check_compilation_unit_constant_ownership,
    check_compilation_unit_types, index_compilation_unit, lower_constant_unit_with_entry, parsed,
    resolve_compilation_unit_names, standard_environments,
};
use crate::ssa::model::TerminatorKind;
use lang_frontend::name_resolution::SourceUnitKey;

#[test]
fn original_aborting_binary_left_has_no_runtime_successor_or_cleanup() {
    let text = include_str!(
        "../../../../lang-frontend/tests/ownership_constants/fixtures/aborting-binary-left.ko"
    );
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "constants.ko", text);
    assert_eq!(sources.source_text(source), Ok(text));
    let (ne, te) = standard_environments();
    for rooted in [false, true] {
        let inputs = [if rooted {
            SourceUnitInput::new("root", "constants.ko", source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let [indexed] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(indexed.source_id(), source);
        assert!(match (rooted, indexed.key()) {
            (false, SourceUnitKey::MemoryDocument) => true,
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                root.as_str() == "root" && logical_path.as_str() == "constants.ko"
            }
            _ => false,
        });
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te)
            .unwrap()
            .validate_constants()
            .unwrap();
        let owned =
            check_compilation_unit_constant_ownership(&sources, &inputs, &names, &te, &typed)
                .unwrap()
                .validate()
                .unwrap();
        assert!(owned.materializations().is_empty());
        let id = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "use")
            .unwrap()
            .id();
        let (program, entry) =
            lower_constant_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, id)
                .expect("original no-return equality must reach verified constant Unit SSA");
        crate::ssa::verify::verify_program(&program).unwrap();
        let module = program.module(entry.module()).unwrap();
        let caller = module.function(entry).unwrap();
        let [head] = caller.blocks.as_slice() else {
            panic!(
                "no normal successor or dead tail block: {:?}",
                caller.blocks
            )
        };
        assert_eq!(caller.entry_block(), Some(head.id));
        assert!(head.parameters.is_empty());
        let [id] = head.instructions.as_slice() else {
            panic!("only the no-return call may run; no borrow, drop, or materialization")
        };
        let call = caller.instruction(*id).unwrap();
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments,
        } = &call.operation
        else {
            panic!("the lhs is the original no-argument direct call")
        };
        assert!(arguments.is_empty());
        assert!(call.results.is_empty());
        let call_start = text.find("stop() == TEXT").unwrap();
        assert_eq!(
            call.origin.span(),
            sources
                .span(source, call_start, call_start + "stop()".len())
                .unwrap()
        );
        let target = module
            .function(*callee)
            .expect("actual local Nothing callee");
        let stop = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "stop")
            .expect("original stop declaration");
        assert_eq!(target.name, format!("koven.stop.d{}", stop.id().index()));
        assert!(target.never_returns);
        assert!(target.return_types.is_empty());
        let terminal = head.terminator.as_ref().unwrap();
        assert!(matches!(terminal.kind, TerminatorKind::Unreachable));
        assert!(terminal.kind.targets().is_empty());
        assert_eq!(terminal.origin.span(), call.origin.span());
    }
}
