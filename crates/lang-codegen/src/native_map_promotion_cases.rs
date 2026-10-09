//! Ordinary constructor/local operands; no annotated nullable substitute or fault generation.
use crate::native_borrow_last_use_cases::Case;

pub(crate) fn cases() -> [Case; 4] {
    [
        Case {
            name: "nullable-map-resource-delivery",
            source: r#"class Resource(val n: Int) { deinit() { if (n == 1) { println("drop-one") } else { println("drop-two") } } }
fun entry() {
 var source = mutableMapOf<String, Resource?>()
 source.put("key".clone(), Resource(1))
 val replacement = Resource(2)
 source.put("key".clone(), (replacement))
 source.put("key".clone(), null)
}"#,
            stdout: b"drop-one\ndrop-two\n",
            allocations: 6,
        },
        Case {
            name: "nullable-map-move-only-payload",
            source: r#"value class Packet(val text: String)
class Envelope(val packet: Packet)
fun entry() {
 var source = mutableMapOf<String, Envelope?>()
 source.put("key".clone(), Envelope(Packet("one".clone())))
 val replacement = Envelope(Packet("two".clone()))
 source.put("key".clone(), replacement)
 source.put("key".clone(), null)
}"#,
            stdout: b"",
            allocations: 8,
        },
        Case {
            name: "nullable-map-box-move-only-value",
            source: r#"value class Packet(val text: String)
fun entry() {
 var source = mutableMapOf<String, Box<Packet>?>()
 source.put("key".clone(), Box(Packet("one".clone())))
 val replacement = Box(Packet("two".clone()))
 source.put("key".clone(), (replacement))
 source.put("key".clone(), null)
}"#,
            stdout: b"",
            allocations: 8,
        },
        Case {
            name: "nullable-map-string-field-value",
            source: r#"class Label(val text: String)
fun entry() {
 var source = mutableMapOf<String, Label?>()
 source.put("key".clone(), Label("one".clone()))
 val replacement = Label("two".clone())
 source.put("key".clone(), replacement)
 source.put("key".clone(), null)
}"#,
            stdout: b"",
            allocations: 8,
        },
    ]
}
