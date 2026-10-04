"""Exercise the real CLI tree, including text/ranges that corpus S-expressions omit."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

GRAMMAR = Path(__file__).resolve().parents[1]
CLI = os.environ.get("TREE_SITTER", "tree-sitter")


class EditorContractTests(unittest.TestCase):
    def parse(self, source, valid=True):
        with tempfile.TemporaryDirectory(prefix="koven-editor-") as directory:
            path = Path(directory) / "probe.ko"
            path.write_text(source)
            result = subprocess.run([CLI, "parse", "--xml", str(path)], cwd=GRAMMAR,
                                    text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 0 if valid else 1, result.stdout + result.stderr)
        start = result.stdout.index("<?xml")
        end = result.stdout.index("</sources>") + len("</sources>")
        tree = ET.fromstring(result.stdout[start:end]).find("source/source_file")
        self.assertIsNotNone(tree)
        if valid:
            self.assertEqual(list(tree.iter("ERROR")), [])
        else:
            self.assertTrue(list(tree.iter("ERROR")), result.stdout)
        return tree

    def test_parameter_modes_and_same_spelling_names(self):
        for mode in ("own", "borrow", "inout"):
            with self.subTest(mode=mode):
                tree = self.parse(f"fun f({mode} item: Int, {mode}: Int): Int = item\n")
                self.assertEqual(len(list(tree.iter("parameter_mode"))), 1)
                tree = self.parse(f"val f: ({mode} Int, {mode}) -> Int = handler\n")
                self.assertEqual(len(list(tree.iter("parameter_mode"))), 1)
                tree = self.parse(f"class Holder {{ {mode} /* receiver */ fun f(): Int = 1 }}\n"
                                  f"val f: ({mode} /* function */ (Int) -> Int) -> Int = handler\n")
                self.assertEqual(len(list(tree.iter("parameter_mode"))), 2)

    def test_soft_words_keep_ordinary_name_and_call_positions(self):
        for word in ("value", "loop", "own", "borrow", "inout", "move", "by", "to"):
            with self.subTest(word=word):
                tree = self.parse(f"fun {word}({word}: Int): Int = {word}\n"
                                  f"val result = {word}({word}) + cell.{word}()\n")
                self.assertEqual(len(list(tree.iter("call_expression"))), 2)
                self.assertEqual(list(tree.iter("parameter_mode")), [])

    def test_contextual_constructs_have_their_actual_nodes(self):
        tree = self.parse("value class Point(val value: Int)\n"
                          "class Holder { own fun take(): Int = 1 }\n"
                          "val f: move (own Int) -> Int = move { item -> item }\n"
                          "fun run(): Unit { loop { break } }\n"
                          "val pair = 1 to 2\nclass Adapter(val inner: Shape): Shape by inner\n")
        for tag, count in (("value_class_declaration", 1), ("parameter_mode", 2),
                           ("loop_statement", 1), ("lambda_expression", 1), ("to_operator", 1), ("delegation_keyword", 1)):
            self.assertEqual(len(list(tree.iter(tag))), count, tag)
        closure = tree.find(".//lambda_expression")
        self.assertIn("move", closure.text)

    def test_named_argument_precedes_assignment_but_grouping_preserves_it(self):
        tree = self.parse("val result = call(name = input, (slot = input), name = &slot)\n")
        self.assertEqual(len(list(tree.iter("named_argument_prefix"))), 2)
        self.assertEqual(len(list(tree.iter("assignment_expression"))), 1)
        self.assertEqual(len(list(tree.iter("argument_mode"))), 1)

    def test_reserved_words_are_rejected_at_the_word_and_suffix_names_survive(self):
        for word in ("async", "await", "suspend", "actor", "spawn", "sealed",
                     "dyn", "where", "yield", "macro", "reify"):
            with self.subTest(word=word):
                tree = self.parse(f"val before = 0\nval {word} = 1\nval {word}Task = 2\n", valid=False)
                errors = list(tree.iter("ERROR"))
                self.assertTrue(any(error.get("srow") == "1" and error.get("scol") == "4"
                                    and error.get("erow") == "1" and 4 < int(error.get("ecol")) <= 4 + len(word)
                                    for error in errors), ET.tostring(tree).decode())
                names = [node.text for node in tree.findall("variable_declaration/identifier[@field='name']")]
                self.assertEqual(names[0], "before")
                self.assertEqual(names[-1], f"{word}Task")
                self.assertNotIn(word, names)
                after = tree.findall("variable_declaration")[-1]
                self.assertEqual(after.get("srow"), "2")
                self.assertEqual(list(after.iter("ERROR")), [])
                self.parse(f"val {word}Task = 1\nval prefix{word} = 2\n")

    def test_context_lookahead_preserves_comments_and_token_ranges(self):
        tree = self.parse("fun f(own /* mode */ item: Int): Int = item\n"
                          "fun g(): Unit { loop /* body */ { break } }\n"
                          "val f = move /* capture */ { 1 }\n")
        mode = tree.find(".//parameter_mode")
        self.assertEqual((mode.get("scol"), mode.get("ecol")), ("6", "9"))
        self.assertEqual(len(list(tree.iter("loop_statement"))), 1)
        self.assertEqual(len(list(tree.iter("lambda_expression"))), 1)

    def test_word_operators_do_not_split_identifier_prefixes(self):
        tree = self.parse("val input = 1\nval island = 2\nval asset = 3\n"
                          "val membership = input in items\nval check = island is Int\n"
                          "val cast = asset as Int\nval safe = asset as? Int\n"
                          "val absent = input !in items\nval different = island !is Int\n")
        self.assertEqual(len(list(tree.iter("binary_expression"))), 4)
        self.assertEqual(len(list(tree.iter("cast_expression"))), 2)
        for word in ("input", "island", "asset"):
            self.parse(f"val rejected = consume(own {word})\nval after = 1\n", valid=False)

    def test_compound_word_operators_require_adjacent_whole_words(self):
        for operand in ("!input", "!island", "!inside", "!in_item", "!is2"):
            with self.subTest(operand=operand):
                tree = self.parse(f"val result = {operand}\n")
                self.assertEqual(len(list(tree.iter("prefix_expression"))), 1)
                self.assertEqual(tree.find(".//prefix_expression/expression/identifier").text, operand[1:])
                self.parse(f"val rejected = value {operand}\n", valid=False)
        for operator in ("! in", "! is", "!/*gap*/in", "!/*gap*/is", "as ?", "as/*gap*/?"):
            with self.subTest(operator=operator):
                self.parse(f"val rejected = value {operator} Target\n", valid=False)

    def test_invalid_modes_and_destructuring_wildcard_recover(self):
        for value in ("consume(own input)", "consume(borrow input)", "consume(inout input)"):
            with self.subTest(value=value):
                tree = self.parse(f"val rejected = {value}\nval after = 1\n", valid=False)
                self.assertEqual(tree.findall("variable_declaration/identifier[@field='name']")[-1].text, "after")
        self.parse("fun f(): Unit { val (first, _) = pair }\n", valid=False)
        self.parse("fun f(): Unit { val (first, second) = pair }\n")
        self.parse("fun f(): Unit { for ((first, _) in pairs) {} }\n")


if __name__ == "__main__":
    unittest.main()
