"""SPEC-0269's finite program domain, source renderer and independent oracle.

Only structured operations are shared by the renderer and evaluator. In
particular, the evaluator never reads source, compiler facts or native output.
Move/borrow events are model expectations, not observations of native execution.
"""
import copy
import hashlib
import json
import re

GENERATOR_VERSION = "generated-owners-v1"
DEFAULT_SEEDS = (11, 29, 47, 83)
_NAME = re.compile(r"[a-z][a-z0-9_]{0,31}\Z")
# Owner names are part of this small grammar, not a duplicate language keyword table.
_OWNER = re.compile(r"(?:source|spare|moved[0-9]*|extra[0-9]+|holder|local(?:_moved)?|old)\Z")
_OP_KEYS = {
    "create": {"op", "target", "resource"},
    "holder": {"op", "target", "resource"},
    "move": {"op", "source", "target"},
    "inspect": {"op", "source"},
    "consume": {"op", "source"},
    "borrow_consume": {"op", "source"},
    "replace": {"op", "holder", "target", "resource"},
    "return_if": {"op"},
    "marker": {"op", "text"},
}


def _require(condition, reason):
    if not condition:
        raise ValueError(reason)


def _choice(seed, case_index, key, modulus):
    """Hash UTF-8 version/decimal-seed/decimal-index/key, then big-endian mod N."""
    encoded = f"{GENERATOR_VERSION}/{seed}/{case_index}/{key}".encode("utf-8")
    digest = hashlib.sha256(encoded).hexdigest()
    return {"key": key, "modulus": modulus, "sha256": digest,
            "value": int(digest, 16) % modulus}


def cases(seeds=None):
    """Return the fixed 8 valid + 8 invalid cases; custom seeds keep all four paths."""
    seeds = list(DEFAULT_SEEDS if seeds is None else seeds)
    _require(len(seeds) == 4 and all(type(s) is int and 0 <= s < 2**32 for s in seeds)
             and len(set(seeds)) == 4, "exactly four distinct uint32 seeds required")
    result = []
    for shape in ("V1", "V2", "I1", "I2"):
        for index, seed in enumerate(seeds):
            choices = [_choice(seed, index, "extra_locals", 3),
                       _choice(seed, index, "move_count", 3),
                       _choice(seed, index, "name_suffix", 65536)]
            extra, moves, suffix = [c["value"] for c in choices]
            case = {"generator_version": GENERATOR_VERSION,
                    "id": f"{shape.lower()}-{index}-{seed}", "shape": shape,
                    "seed": seed, "case_index": index, "choices": choices}
            resource = lambda name: f"{name}_{suffix:04x}"
            locals_ = [{"op": "create", "target": f"extra{i}", "resource": resource(f"extra{i}")}
                       for i in range(extra)]
            if shape == "V2":
                position, stop = (("before", False), ("before", True),
                                  ("after", False), ("after", True))[index]
                case.update(return_position=position, stop=stop)
                operations = [{"op": "holder", "target": "holder", "resource": resource("old")},
                              {"op": "create", "target": "local", "resource": resource("local")}]
                if index != 0:
                    operations.extend(locals_)
                    if moves:
                        operations.append({"op": "move", "source": "local", "target": "local_moved"})
                if position == "before":
                    operations.append({"op": "return_if"})
                operations.extend([{"op": "replace", "holder": "holder", "target": "old",
                                    "resource": resource("new")},
                                   {"op": "inspect", "source": "old"}])
                if position == "after":
                    operations.append({"op": "return_if"})
                operations.append({"op": "marker", "text": "helper-done"})
                if index == 0:
                    # Hand-specified implementation witness: Leaf old=0, Holder=1,
                    # local=2, replacement=3. Field 3 must free before instance 1.
                    case["expected_free_order"] = [0, 2, 3, 1]
            else:
                operations = [{"op": "create", "target": "source", "resource": resource("leaf")}]
                operations.extend(locals_)
                if shape == "I2":
                    operations.append({"op": "borrow_consume", "source": "source"})
                else:
                    owner = "source"
                    for step in range(1 + moves):
                        target = f"moved{step}"
                        operations.append({"op": "move", "source": owner, "target": target})
                        owner = target
                    operations.extend([{"op": "inspect", "source": owner},
                                       {"op": "consume", "source": owner}])
                    if shape == "I1":
                        operations.append({"op": "inspect", "source": "source"})
                operations.append({"op": "marker", "text": "done"})
            case["operations"] = operations
            validate(case)
            result.append(case)
    return result


def validate(case):
    """Reject inputs outside the finite grammar, including a second ownership root."""
    _require(isinstance(case, dict), "case must be an object")
    required = {"generator_version", "id", "shape", "seed", "case_index", "choices", "operations"}
    _require(required <= case.keys() and case.keys() <= required | {
        "return_position", "stop", "expected_free_order"}, "unknown/missing case fields")
    _require(case["generator_version"] == GENERATOR_VERSION, "unsupported generator version")
    _require(isinstance(case["id"], str) and re.fullmatch(r"[a-z0-9-]{1,80}", case["id"]), "invalid id")
    shape = case["shape"]
    _require(shape in ("V1", "V2", "I1", "I2"), "unsupported shape")
    _require(type(case["seed"]) is int and 0 <= case["seed"] < 2**32, "seed must be uint32")
    _require(type(case["case_index"]) is int and 0 <= case["case_index"] < 4, "index outside fixed domain")
    _require(isinstance(case["choices"], list), "choices must be a list")
    keys = set()
    for choice in case["choices"]:
        _require(isinstance(choice, dict) and choice.keys() == {"key", "modulus", "sha256", "value"},
                 "malformed choice")
        key, modulus = choice["key"], choice["modulus"]
        _require(isinstance(key, str) and _NAME.fullmatch(key) and key not in keys, "invalid choice key")
        _require(type(modulus) is int and 1 <= modulus <= 65536, "invalid choice modulus")
        _require(type(choice["value"]) is int and isinstance(choice["sha256"], str), "invalid choice encoding")
        _require(choice == _choice(case["seed"], case["case_index"], key, modulus), "choice hash mismatch")
        keys.add(key)
    operations = case["operations"]
    _require(isinstance(operations, list) and 1 <= len(operations) <= 12, "operation budget exceeded")
    live, declared, resources, retired = {}, set(), set(), set()
    origins, holder_fields = {}, {}
    moved_resources, read_resources, consumed_resources = set(), set(), set()
    counts = {op: 0 for op in _OP_KEYS}
    invalid_reads, good_reads, return_index, replace_index, holder_index = 0, 0, None, None, None
    replaced_resource = None
    markers = []
    for index, operation in enumerate(operations):
        _require(isinstance(operation, dict), "operation must be an object")
        op = operation.get("op")
        _require(isinstance(op, str) and op in _OP_KEYS and operation.keys() == _OP_KEYS[op],
                 "unknown/malformed operation")
        counts[op] += 1
        for key in ("source", "target", "holder", "resource"):
            if key in operation:
                value = operation[key]
                _require(isinstance(value, str) and _NAME.fullmatch(value), "invalid identifier")
                if key != "resource":
                    _require(_OWNER.fullmatch(value), "owner name outside finite grammar")
        if "resource" in operation:
            resource = operation["resource"]
            _require(resource not in resources and resource != "holder", "resource literals must be unique")
            resources.add(resource)
        if "target" in operation:
            target = operation["target"]
            _require(target not in declared, "duplicate owner declaration")
            declared.add(target)
        if op in ("create", "holder"):
            live[operation["target"]] = "Holder" if op == "holder" else "Leaf"
            origins[operation["target"]] = operation["resource"]
            if op == "holder":
                holder_fields[operation["target"]] = operation["resource"]
                holder_index = index
        elif op == "move":
            source = operation["source"]
            _require(live.get(source) == "Leaf", "move needs a live Leaf")
            live[operation["target"]] = live.pop(source)
            origins[operation["target"]] = origins[source]
            moved_resources.add(origins[source])
            retired.add(source)
        elif op in ("inspect", "consume", "borrow_consume"):
            source = operation["source"]
            if source not in live:
                _require(shape == "I1" and op == "inspect" and source in retired,
                         "use of absent/moved owner")
                invalid_reads += 1
            else:
                _require(live[source] == "Leaf", "call needs Leaf")
                if op == "inspect":
                    good_reads += 1
                    read_resources.add(origins[source])
                elif op == "consume":
                    live.pop(source)
                    retired.add(source)
                    consumed_resources.add(origins[source])
        elif op == "replace":
            _require(live.get(operation["holder"]) == "Holder", "replace needs live Holder")
            live[operation["target"]] = "Leaf"
            replaced_resource = holder_fields[operation["holder"]]
            origins[operation["target"]] = replaced_resource
            replace_index = index
        elif op == "return_if":
            return_index = index
        elif op == "marker":
            _require(operation["text"] in ("done", "helper-done", "checkpoint"), "unknown marker")
            markers.append(operation["text"])
    _require(len(resources) <= 8, "Leaf budget exceeded")
    _require(counts["holder"] <= 1, "Holder budget exceeded")
    if shape == "V2":
        _require(case.get("return_position") in ("before", "after") and type(case.get("stop")) is bool,
                 "V2 requires position and Boolean")
        _require(counts["holder"] == counts["replace"] == counts["return_if"] == 1
                 and counts["borrow_consume"] == 0 and invalid_reads == 0 and good_reads >= 1,
                 "V2 requires one holder/replace/return and a legal read")
        _require((return_index < replace_index) == (case["return_position"] == "before"),
                 "return position disagrees with operations")
        _require(holder_index < return_index and replaced_resource in read_resources,
                 "V2 must construct Holder before its return and read replaced old state")
        _require(markers.count("helper-done") == 1 and "done" not in markers,
                 "V2 requires its helper completion marker")
        _require(operations[-1] == {"op": "marker", "text": "helper-done"}, "helper marker must finish body")
    else:
        _require("return_position" not in case and "stop" not in case, "non-V2 has branch metadata")
        _require(counts["holder"] == counts["replace"] == counts["return_if"] == 0, "operation outside V1 domain")
        _require(markers.count("done") == 1 and "helper-done" not in markers, "entry requires done")
        _require(operations[-1] == {"op": "marker", "text": "done"}, "done must finish entry body")
        if shape == "I2":
            _require(counts["borrow_consume"] == 1 and invalid_reads == 0, "I2 requires exactly one Borrow consume")
        else:
            _require(counts["move"] >= 1 and counts["consume"] >= 1 and good_reads >= 1
                     and counts["borrow_consume"] == 0, "V1/I1 must witness move, Borrow and consume")
            _require(moved_resources & read_resources & consumed_resources,
                     "move, Borrow and consume must witness the same resource")
            _require(invalid_reads == (1 if shape == "I1" else 0), "wrong number of ownership roots")
    if "expected_free_order" in case:
        _require(shape == "V2" and case["return_position"] == "before" and case["stop"] is False
                 and case["expected_free_order"] == [0, 2, 3, 1]
                 and all(type(index) is int for index in case["expected_free_order"])
                 and [o["op"] for o in operations] == ["holder", "create", "return_if", "replace", "inspect", "marker"],
                 "physical witness must retain its fixed construction order")


class _Writer:
    """Record occurrences while appending; repeated names never use text searches."""
    def __init__(self):
        self.parts = []
        self.size = 0

    def write(self, text):
        start = self.size
        self.parts.append(text)
        self.size += len(text.encode("utf-8"))
        return [start, self.size]


def render(case):
    validate(case)
    writer = _Writer()
    writer.write('// 资源程序：UTF-8 span witness\n'
                 'class Leaf(val name: String) { deinit() { println(this.name) } }\n'
                 'fun inspect(item: Leaf): Unit { println("borrow"); println(item.name) }\n'
                 'fun consume(own item: Leaf): Unit { println("consume") }\n')
    diagnostics, retired = [], {}
    if case["shape"] == "I2":
        writer.write("fun invalid(")
        declaration = writer.write("item")
        writer.write(": Leaf): Unit { consume(")
        primary = writer.write("item")
        writer.write(") }\n")
        diagnostics.append({"code": "L0133", "primary": primary, "labels": [declaration]})
    if case["shape"] == "V2":
        writer.write('class Holder(var state: Leaf) { deinit() { println("drop:holder") } }\n'
                     'fun work(own stop: Boolean): Unit {\n')
    else:
        writer.write("fun entry(): Unit {\n")
    for operation in case["operations"]:
        op = operation["op"]
        writer.write("    ")
        if op in ("create", "holder"):
            writer.write(f'val {operation["target"]} = ')
            if op == "holder":
                writer.write("Holder(")
            writer.write(f'Leaf("drop:{operation["resource"]}")')
            if op == "holder":
                writer.write(")")
        elif op == "move":
            writer.write(f'val {operation["target"]} = ')
            retired[operation["source"]] = writer.write(operation["source"])
        elif op in ("inspect", "consume", "borrow_consume"):
            writer.write(("invalid" if op == "borrow_consume" else op) + "(")
            span = writer.write(operation["source"])
            if op == "inspect" and operation["source"] in retired:
                diagnostics.append({"code": "L0131", "primary": span,
                                    "labels": [retired[operation["source"]]]})
            elif op == "consume":
                retired[operation["source"]] = span
            writer.write(")")
        elif op == "replace":
            writer.write(f'val {operation["target"]} = replace(&{operation["holder"]}.state, '
                         f'Leaf("drop:{operation["resource"]}"))')
        elif op == "return_if":
            writer.write("if (stop) { return }")
        elif op == "marker":
            writer.write(f'println("{operation["text"]}")')
        writer.write("\n")
    writer.write("}\n")
    if case["shape"] == "V2":
        writer.write(f'fun entry(): Unit {{ work({str(case["stop"]).lower()}); println("done") }}\n')
    source = "".join(writer.parts)
    _require(writer.size <= 8192, "source budget exceeded")
    return {"source": source, "expected_diagnostics": diagnostics}


class _Owners:
    """Logical owner table and lexical scopes, independent of source generation."""
    def __init__(self):
        self.owners = {}
        self.resources = {}
        self.scopes = []
        self.events = []
        self.output = []
        self.allocations = 0

    def event(self, kind, **values):
        self.events.append({"kind": kind, **values})

    def enter(self, name):
        self.scopes.append((name, []))
        self.event("scope_enter", scope=name)

    def name(self, local):
        return f"{self.scopes[-1][0]}.{local}"

    def bind(self, owner, resource):
        self.owners[owner] = resource
        self.scopes[-1][1].append(owner)

    def allocate(self, resource, owner, kind="Leaf", field=None):
        self.resources[resource] = {"type": kind, "field": field}
        self.owners[owner] = resource
        self.allocations += 1
        self.event("allocate", resource=resource, type=kind, owner=owner)

    def move(self, source, target):
        resource = self.owners.pop(source)
        self.bind(target, resource)
        self.event("move", resource=resource, source=source, target=target)

    def marker(self, text):
        self.output.append(text + "\n")
        self.event("marker", text=text)

    def drop(self, owner):
        resource = self.owners.pop(owner)
        self.output.append(f"drop:{resource}\n")
        self.event("drop", resource=resource, owner=owner)
        if self.resources[resource]["type"] == "Holder":
            self.drop(self.resources[resource]["field"])

    def leave(self):
        name, owners = self.scopes.pop()
        for owner in reversed(owners):
            if owner in self.owners:
                self.drop(owner)
        self.event("scope_exit", scope=name)

    def run(self, operations, stop=False):
        for operation in operations:
            op = operation["op"]
            if op == "create":
                owner = self.name(operation["target"])
                self.allocate(operation["resource"], owner)
                self.scopes[-1][1].append(owner)
            elif op == "holder":
                owner = self.name(operation["target"])
                field = owner + ".state"
                self.allocate(operation["resource"], field)
                self.allocate("holder", owner, "Holder", field)
                self.scopes[-1][1].append(owner)
            elif op == "move":
                self.move(self.name(operation["source"]), self.name(operation["target"]))
            elif op == "inspect":
                owner = self.name(operation["source"])
                resource = self.owners[owner]
                self.event("borrow_begin", resource=resource, owner=owner)
                self.event("borrow_read", resource=resource, owner=owner, field="name")
                self.output.extend(["borrow\n", f"drop:{resource}\n"])
                self.event("borrow_end", resource=resource, owner=owner)
            elif op == "consume":
                source = self.name(operation["source"])
                self.enter("consume")
                self.move(source, self.name("item"))
                self.marker("consume")
                self.leave()
            elif op == "replace":
                field = self.name(operation["holder"]) + ".state"
                old = self.owners.pop(field)
                new = operation["resource"]
                self.allocate(new, field)
                target = self.name(operation["target"])
                self.bind(target, old)
                self.event("replace", owner=field, old=old, new=new, target=target)
            elif op == "return_if":
                self.event("branch", stop=stop)
                if stop:
                    self.event("return", scope=self.scopes[-1][0])
                    break
            elif op == "marker":
                self.marker(operation["text"])


def evaluate(case):
    validate(case)
    _require(case["shape"] in ("V1", "V2"), "invalid programs have no normal execution oracle")
    model = _Owners()
    model.enter("entry")
    if case["shape"] == "V2":
        model.enter("work")
    model.run(case["operations"], case.get("stop", False))
    if case["shape"] == "V2":
        model.leave()
        model.marker("done")
    model.leave()
    _require(not model.owners, "model leaked logical owners")
    return {"stdout": "".join(model.output), "allocations": model.allocations, "events": model.events}


def complexity(case):
    """A monotone structural size; metadata/provenance is deliberately excluded."""
    return (len(case["operations"]), len(json.dumps(case["operations"], sort_keys=True)))


def shrink_candidates(case):
    """Yield deterministic same-domain reductions, never arbitrary token edits.

    A caller must still execute each candidate and retain the same failure
    fingerprint. This function establishes only grammar/ownership validity.
    """
    validate(case)
    candidates = []
    operations = case["operations"]
    for index, operation in enumerate(operations):
        if operation["op"] in ("create", "inspect", "marker"):
            candidate = copy.deepcopy(case)
            del candidate["operations"][index]
            candidates.append(candidate)
        if operation["op"] == "move":
            candidate = copy.deepcopy(case)
            source, target = operation["source"], operation["target"]
            del candidate["operations"][index]
            for later in candidate["operations"][index:]:
                if later.get("source") == target:
                    later["source"] = source
            candidates.append(candidate)
    used = {op["resource"] for op in operations if "resource" in op}
    for index, operation in enumerate(operations):
        if "resource" in operation:
            short = next((name for name in "abcdefghijklmnopqrstuvwxyz" if name not in used), None)
            if short is not None and len(short) < len(operation["resource"]):
                candidate = copy.deepcopy(case)
                candidate["operations"][index]["resource"] = short
                candidates.append(candidate)
    seen = set()
    for candidate in candidates:
        try:
            validate(candidate)
        except ValueError:
            continue
        encoded = json.dumps(candidate, sort_keys=True, separators=(",", ":"))
        if complexity(candidate) < complexity(case) and encoded not in seen:
            seen.add(encoded)
            yield candidate
