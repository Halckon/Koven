//! Real forwarding callees; no native execution or manufactured evidence.
use super::*;
use crate::ssa::{unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn original_forward_closes_branch_to_identity_across_sources_and_assemblies() {
    for reverse in [false, true] {
        let mut sources = SourceMap::new();
        let (provider_id, provider) = unit_lower_test_support::parsed(
            &mut sources,
            "forward-provider.ko",
            "fun identity(source:String):borrow String from source=source\nfun forward(source:String):borrow String from source=identity(source)",
        );
        let (caller_id, caller) = unit_lower_test_support::parsed(
            &mut sources,
            "forward-caller.ko",
            "fun wrap(source:String,flag:Boolean):borrow String from source {if(flag){return forward(source)}else{return forward(source)}}\nfun consume(item:String):Unit {}\nfun run(flag:Boolean):Unit {val owner=\"kept\";borrow val item=wrap(owner,flag);consume(item)}",
        );
        let mut inputs = [
            SourceUnitInput::new("root", "forward-provider.ko", provider_id, &provider),
            SourceUnitInput::new("root", "forward-caller.ko", caller_id, &caller),
        ];
        if reverse {
            inputs.reverse();
        }
        let (environment, types) = standard_environments();
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let declaration = |name| {
            names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == name)
                .unwrap()
                .id()
        };
        let regions: Vec<_> = owned
            .ownership()
            .loan_action_observations()
            .iter()
            .flat_map(|c| c.forward_regions().iter())
            .collect();
        assert_eq!(regions.len(), 1);
        let (programs, audits) = observe(|| {
            (0..2)
                .map(|_| {
                    unit_lower::lower_scalar_unit_with_entry(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        &owned,
                        declaration("run"),
                    )
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(audits.len(), 2);
        for (result, audit) in programs.into_iter().zip(audits) {
            let (program, _) = result.unwrap();
            crate::ssa::verify::verify_program(&program).unwrap();
            assert!(audit.unsupported.is_empty(), "{:?}", audit.unsupported);
            assert!(
                audit
                    .borrow_edges
                    .contains(&(declaration("forward"), declaration("identity")))
            );
            assert!(
                audit
                    .borrow_edges
                    .contains(&(declaration("wrap"), declaration("forward")))
            );
            assert!(
                audit
                    .borrow_edges
                    .contains(&(declaration("run"), declaration("wrap")))
            );
            let forward = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.name.contains(".forward."))
                .unwrap();
            assert_eq!(forward.instructions.len(), 1);
            assert_eq!(forward.blocks.len(), 1);
            assert_eq!(forward.loans.len(), 2);
        }
    }
}

#[test]
fn non_expression_forward_keeps_original_unsupported_marker() {
    let mut sources = SourceMap::new();
    let text = "fun identity(source:String):borrow String from source=source\nfun forward(source:String):borrow String from source {return identity(source)}\nfun consume(item:String):Unit {}\nfun run():Unit {val owner=\"kept\";borrow val item=forward(owner);consume(item)}";
    let (id, parsed) = unit_lower_test_support::parsed(&mut sources, "forward-block.ko", text);
    let inputs = [SourceUnitInput::memory_document(id, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let declaration = |name| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == name)
            .unwrap()
            .id()
    };
    assert!(
        owned
            .ownership()
            .loan_action_observations()
            .iter()
            .all(|c| c.forward_regions().is_empty())
    );
    let (result, audits) = observe(|| {
        unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            declaration("run"),
        )
    });
    let Err(error) = result else {
        panic!("original block borrowed-return route remains unsupported");
    };
    assert_eq!(
        error.kind,
        crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode
    );
    assert_eq!(sources.slice(error.span.unwrap()), Ok("borrow"));
    assert!(audits.is_empty());
}
