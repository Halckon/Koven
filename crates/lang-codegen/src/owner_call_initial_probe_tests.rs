//! Ordinary probe of existing lowering, not authorization for a new initial producer.
use super::{OwnedProducts, UnitSource};
#[test]
fn call_initial_root_retains_existing_lowering_without_base_claim() {
    for constant in [false, true] {
        let prefix = if constant { "const val ONE:Int=1" } else { "" };
        let one = if constant { "ONE" } else { "1" };
        let text = format!(r#"{prefix}
fun makeSeed():String {{ return "seed" }}
fun run(outer:Int,inner:Int) {{
    var a=makeSeed(); var b="b"; var n0=outer; var g0=n0>0;
    while(g0) {{ var n1=inner; var g1=n1>0;
        while(g1) {{ val x=replace(&a,"child-a"); val y=replace(&b,"child-b");
            n1=n1-1; g1=n1>0 }};
        val old=replace(&b,"parent-b"); n0=n0-1; g0=n0>0
    }}; println(a); println(b)
}}
fun entry() {{ run(0,2); run({one},0); run(2,2) }}"#);
        let source = UnitSource::analyze("owner-call-initial-probe.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(matches!(source.products, OwnedProducts::Constant(..)), constant);
        let (result, actions) = crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, entry) = result.unwrap_or_else(|e| panic!("{e:?}; {actions:?}"));
        crate::ssa::verify::verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        println!("ROUTE constant={constant}\nSOURCE\n{text}\nACTIONS {actions:?}\nSSA {program:#?}\nLLVM\n{llvm}");
        assert!(!actions.iter().any(|(name,_)| *name == "parent-initial-owner-base-ssa-complete"));
        super::assert_native_output(&source, b"seed\nb\nseed\nparent-b\nchild-a\nparent-b\n");
    }
}
