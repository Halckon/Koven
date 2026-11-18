//! Original two-root borrow wrapper compiled in one strict MemoryDocument Unit transaction.
//! Test-only root selection does not widen the production singleton entry.
use crate::ssa::{model::*, unit_lower::orchestrate::lower_roots};
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SourceUnitKey, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::owned_compilation_unit_view,
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

pub(super) fn lower_original(text: &str) -> (Program, FunctionId, FunctionId) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("result.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let raw_names =
        resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(raw_names.diagnostics().is_empty());
    let names = raw_names.validate().unwrap();
    let [document] = names.names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), source);
    let raw_types = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(raw_types.diagnostics().is_empty());
    let complete =
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw_types).unwrap();
    assert!(complete.is_same_analysis(&raw_types));
    let (typed, raw_owned) =
        analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw_types)
            .unwrap()
            .into_result()
            .unwrap();
    assert!(raw_owned.diagnostics().is_empty());
    assert!(raw_owned.deferred().is_empty());
    let owned = raw_owned.validate().unwrap();
    let declarations = names.names().index().declarations();
    let [wrapper, entry] = declarations else {
        panic!("exactly the two original declarations, in source order");
    };
    let wrapper_span = parsed.ast().items().get(wrapper.root()).unwrap().span();
    let entry_span = parsed.ast().items().get(entry.root()).unwrap().span();
    assert_eq!(
        sources.slice(wrapper_span),
        Ok(text.split_once('\n').unwrap().0)
    );
    assert_eq!(sources.slice(entry_span), Ok("fun entry() {}"));
    assert!(wrapper_span.end() < entry_span.start());
    let view =
        owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned).unwrap();
    let (program, roots) = lower_roots(&view, &[wrapper.id(), entry.id()]).unwrap();
    assert_eq!(roots.len(), 2);
    let wrapper_id = roots[&wrapper.id()];
    let entry_id = roots[&entry.id()];
    assert_ne!(wrapper_id, entry_id);
    let [module] = program.modules.as_slice() else {
        panic!("one shared source module");
    };
    assert_eq!(module.functions.len(), 2);
    assert_eq!(module.functions[0].id(), wrapper_id);
    assert_eq!(module.functions[1].id(), entry_id);
    assert_eq!(
        module.function(wrapper_id).unwrap().origin,
        Origin::Source(wrapper_span)
    );
    assert_eq!(
        module.function(entry_id).unwrap().origin,
        Origin::Source(entry_span)
    );
    (program, wrapper_id, entry_id)
}
