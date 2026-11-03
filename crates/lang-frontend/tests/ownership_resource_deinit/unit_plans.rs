//! Named-drop contracts intentionally retire Single observations, not owner-graph witnesses.
use super::{NamedDrop, point_text};
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        DropPoint, UnitDropPoint, UnitDropTarget, check_compilation_unit_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

pub(super) fn plans(text: &str) -> [Vec<NamedDrop>; 2] {
    let mut sources = SourceMap::new();
    let source = sources.add_source("resource-drop.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(sources.source_text(source), Ok(text));
    let (name_env, type_env) = standard_environments();
    [
        SourceUnitInput::memory_document(source, &parsed),
        SourceUnitInput::new("root", "resource-drop.ko", source, &parsed),
    ]
    .map(|input| {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [entry] = index.source_units() else {
            panic!("one original resource source")
        };
        assert_eq!(entry.source_id(), source);
        let source_unit = entry.id();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_env).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_env).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_env, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        owned
            .drops()
            .iter()
            .filter_map(|drop| {
                let UnitDropTarget::Named(symbol) = drop.target() else {
                    return None;
                };
                assert_eq!(symbol.source_unit(), source_unit);
                let name = names.names().source_units()[symbol.source_unit().index()]
                    .resolution()
                    .symbols()[symbol.symbol().index()]
                .name()
                .to_string();
                // Resolve the original AST boundary, never infer it from the expected text.
                let (point, point_source) = match drop.point() {
                    UnitDropPoint::AfterStatement(id) => {
                        (DropPoint::AfterStatement(id.statement()), id.source_unit())
                    }
                    UnitDropPoint::AfterExpression(id) => (
                        DropPoint::AfterExpression(id.expression()),
                        id.source_unit(),
                    ),
                    UnitDropPoint::ControlTransfer(id) => (
                        DropPoint::ControlTransfer(id.expression()),
                        id.source_unit(),
                    ),
                    UnitDropPoint::FunctionEntry(id) => {
                        (DropPoint::FunctionEntry(id.item()), id.source_unit())
                    }
                    UnitDropPoint::CallReturn(id) => {
                        (DropPoint::CallReturn(id.expression()), id.source_unit())
                    }
                    UnitDropPoint::BranchExit { control, branch } => (
                        DropPoint::BranchExit {
                            control: control.expression(),
                            branch,
                        },
                        control.source_unit(),
                    ),
                    UnitDropPoint::LoopExit(id) => {
                        (DropPoint::LoopExit(id.statement()), id.source_unit())
                    }
                    other => panic!("unexpected drop boundary: {other:?}"),
                };
                assert_eq!(point_source, source_unit);
                let (boundary, text) = point_text(&sources, &parsed, point);
                Some(NamedDrop {
                    name,
                    boundary,
                    text,
                })
            })
            .collect()
    })
}
