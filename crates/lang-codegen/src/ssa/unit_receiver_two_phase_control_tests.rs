use super::lower_short_circuit_fixture;
use crate::ssa::model::{LoanKind, Operation};

#[test]
fn receiver_two_phase_diverging_argument_never_activates() {
    for kind in ["class", "value class"] {
        for exit in ["return", "break", "continue", "error(p.TEXT)"] {
            let source = format!(
                "{kind} Host(var item: Int) {{\n\
                 fun read(): Int = item\n\
                 inout fun update(text: String, own next: Int, own flag: Boolean): Unit {{ item = next }}\n}}\n\
                 fun entry(): Unit {{ loop {{ var host = Host(1)\n\
                 val done = host.update(p.TEXT, host.read(), {exit})\n}} }}"
            );
            let program = lower_short_circuit_fixture(&source);
            let entry = program.modules[0]
                .functions
                .iter()
                .find(|function| function.name.contains(".entry."))
                .unwrap();
            assert!(
                !entry.instructions.iter().any(|instruction| matches!(
                    instruction.operation,
                    Operation::BorrowBegin {
                        kind: LoanKind::Exclusive,
                        ..
                    }
                )),
                "unreached activation: {source}"
            );
            let updater = program.modules[0]
                .functions
                .iter()
                .find(|function| function.name.contains(".update."))
                .unwrap();
            assert!(!entry.instructions.iter().any(|instruction|
                matches!(instruction.operation, Operation::DirectCall { callee, .. } if callee == updater.id())));
            crate::llvm::render_verified_program(&program).expect("diverging reservation cleanup");
        }
    }
}

#[test]
fn receiver_two_phase_cfg_joins_preserve_the_single_receiver_place() {
    for kind in ["class", "value class"] {
        for exit in ["return", "break", "continue", "error(p.TEXT)"] {
            let source = format!(
                "{kind} Host(var item: Int) {{\n\
                 fun read(): Int = item\n\
                 inout fun update(text: String, own next: Int): Unit {{ item = next }}\n}}\n\
                 fun entry(own flag: Boolean): Unit {{ loop {{ var host = Host(1)\n\
                 val done = host.update(p.TEXT, if (flag) {{ {exit} }} else {{ host.read() + 1 }})\n\
                 if (host.read() != 2) {{ error(p.TEXT) }}\nbreak }} }}"
            );
            let program = lower_short_circuit_fixture(&source);
            crate::llvm::render_verified_program(&program)
                .expect("reservation must survive argument CFG");
        }
    }
}
