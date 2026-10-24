//! The original shared argument-CFG proof, fed by actual local-source AST anchors.
use super::*;
use crate::ssa::map_provider_exits_pipeline_tests::temporary_argument_cfg_query::argument;
pub(super) fn check<'ir>(
    sources: &SourceMap,
    parsed: &ParsedFile,
    input: &super::ast::Input,
    module: &Module,
    g: &Graph<'ir>,
    factory: &Function,
    uses: &mut Uses,
) -> Result<arguments::Delivery<'ir>, Failure> {
    let expression = input.arguments[0].expression;
    let node = parsed.ast().expressions().get(expression).unwrap();
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(no),
        ..
    } = node.payload()
    else {
        return Err(failure("local argument actual If argument", node));
    };
    let arm = |statement| -> Result<Span, Failure> {
        let lang_frontend::parser::Statement::ControlBody { elements } =
            parsed.ast().statements().get(statement).unwrap().payload()
        else {
            return Err(failure("local argument actual arm body", statement));
        };
        let s = one(
            elements.iter().copied(),
            "local argument sole actual arm expression",
        )?;
        let lang_frontend::parser::Statement::Expression { expression } =
            parsed.ast().statements().get(s).unwrap().payload()
        else {
            return Err(failure("local argument literal arm", s));
        };
        Ok(super::ast::span(parsed, *expression))
    };
    let original = argument::Input {
        call: super::ast::span(parsed, input.call),
        argument: node.span(),
        flag: super::ast::span(parsed, *condition),
        arms: [arm(*then_branch)?, arm(*no)?],
    };
    let (call, end) = argument::check_unit(sources, parsed, &original, module, g, factory, uses)?;
    Ok(arguments::Delivery {
        call,
        cleanup: vec![end],
        argument_owners: BTreeSet::new(),
    })
}
