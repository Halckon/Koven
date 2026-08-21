import { readFileSync } from "node:fs";

const grammar = JSON.parse(
  readFileSync(new URL("../syntaxes/koven.tmLanguage.json", import.meta.url), "utf8"),
);
const lines = readFileSync(new URL("./lexical-contract.tsv", import.meta.url), "utf8")
  .trimEnd()
  .split("\n")
  .map((line, index) => {
    const separator = line.indexOf("\t");
    if (separator <= 0 || separator + 1 >= line.length) {
      throw new Error(`contract line ${index + 1} must contain family and source spelling`);
    }
    return [line.slice(0, separator), line.slice(separator + 1)];
  });

function findPattern(patterns, name) {
  for (const candidate of patterns ?? []) {
    if (candidate.name === name) {
      return candidate;
    }
    const nested = findPattern(candidate.patterns, name);
    if (nested) {
      return nested;
    }
  }
  return undefined;
}

function patternNamed(repository, name) {
  const pattern = findPattern(grammar.repository[repository].patterns, name);
  if (!pattern?.match) {
    throw new Error(`missing ${name} match pattern in ${repository}`);
  }
  return pattern;
}

function exact(pattern) {
  return new RegExp(`^(?:${pattern.match})$`);
}

const operatorPattern = patternNamed("operators", "keyword.operator.symbol.koven");
const punctuationPattern = patternNamed("punctuation", "punctuation.separator.koven");
const floatPattern = patternNamed("numbers", "constant.numeric.float.koven");
const integerPattern = patternNamed("numbers", "constant.numeric.integer.koven");
const escapePattern = patternNamed("strings", "constant.character.escape.koven");
const characterPattern = patternNamed("characters", "constant.character.koven");

const regexes = {
  "symbol.operator": exact(operatorPattern),
  "symbol.punctuation": exact(punctuationPattern),
  "number.float": exact(floatPattern),
  "number.integer": exact(integerPattern),
  "string.escape": exact(escapePattern),
  character: exact(characterPattern),
};
const rejectionRegexes = {
  "reject.number": [regexes["number.float"], regexes["number.integer"]],
  "reject.string_escape": [regexes["string.escape"]],
  "reject.character": [regexes.character],
};
const expectedCounts = new Map([
  ["symbol.operator", 33],
  ["symbol.punctuation", 10],
  ["number.float", 4],
  ["number.integer", 7],
  ["string.escape", 8],
  ["character", 4],
  ["reject.number", 8],
  ["reject.string_escape", 3],
  ["reject.character", 3],
]);

const counts = new Map();
for (const [family, spelling] of lines) {
  counts.set(family, (counts.get(family) ?? 0) + 1);
  if (family.startsWith("reject.")) {
    const candidates = rejectionRegexes[family];
    if (!candidates) {
      throw new Error(`unknown rejection family ${family}`);
    }
    if (candidates.some((candidate) => candidate.test(spelling))) {
      throw new Error(`${family} spelling ${JSON.stringify(spelling)} was accepted`);
    }
    continue;
  }

  const expected = regexes[family];
  if (!expected?.test(spelling)) {
    throw new Error(`${family} spelling ${JSON.stringify(spelling)} was not accepted`);
  }
  if (family === "symbol.operator" && regexes["symbol.punctuation"].test(spelling)) {
    throw new Error(`operator ${JSON.stringify(spelling)} also matched punctuation`);
  }
  if (family === "symbol.punctuation" && regexes["symbol.operator"].test(spelling)) {
    throw new Error(`punctuation ${JSON.stringify(spelling)} also matched operator`);
  }
  if (family === "number.float" && regexes["number.integer"].test(spelling)) {
    throw new Error(`float ${JSON.stringify(spelling)} also matched integer`);
  }
}

if (counts.size !== expectedCounts.size) {
  throw new Error(`contract has ${counts.size} families; expected ${expectedCounts.size}`);
}
for (const [family, expected] of expectedCounts) {
  if (counts.get(family) !== expected) {
    throw new Error(`${family} has ${counts.get(family) ?? 0} cases; expected ${expected}`);
  }
}
const numberPatterns = grammar.repository.numbers.patterns;
if (numberPatterns.indexOf(floatPattern) >= numberPatterns.indexOf(integerPattern)) {
  throw new Error("float pattern must precede integer pattern");
}

process.stdout.write(`validated ${lines.length} TextMate lexical contract cases\n`);
