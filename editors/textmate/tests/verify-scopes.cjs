const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { dirname, resolve } = require('node:path');

/** Both producer and consumer run this contract using the official VS Code engine. */
exports.verifyScopes = async function ({ grammarPath, fixturePath, textmate, oniguruma, wasm }) {
  await oniguruma.loadWASM(wasm);
  const registry = new textmate.Registry({
    onigLib: Promise.resolve({
      createOnigScanner: patterns => new oniguruma.OnigScanner(patterns),
      createOnigString: text => new oniguruma.OnigString(text),
    }),
    loadGrammar: async scope => scope === 'source.koven'
      ? textmate.parseRawGrammar(readFileSync(grammarPath, 'utf8'), grammarPath) : null,
  });
  const grammar = await registry.loadGrammar('source.koven');
  const cases = JSON.parse(readFileSync(fixturePath, 'utf8'));
  assert.ok(cases.length > 0, 'The scope corpus must not be empty');
  let count = 0;
  const failures = [];
  try {
    for (const fixture of cases) {
      const source = readFileSync(resolve(dirname(fixturePath), fixture.file), 'utf8');
      const scopes = [];
      let stack = textmate.INITIAL;
      for (const line of source.split('\n')) {
        const result = grammar.tokenizeLine(line, stack);
        assert.equal(result.stoppedEarly, false, `${fixture.file}: tokenization timed out`);
        for (let i = 0; i < line.length; i++) {
          const token = result.tokens.find(token => token.startIndex <= i && i < token.endIndex);
          scopes.push(token?.scopes.at(-1));
        }
        scopes.push(undefined); // Match source UTF-16 offsets across newlines.
        stack = result.ruleStack;
      }
      assert.ok(fixture.checks.length > 0, `${fixture.file}: missing assertions`);
      for (const check of fixture.checks) {
        const start = check.start;
        assert.ok(Number.isInteger(start) && start >= 0 && check.text.length > 0);
        assert.equal(source.slice(start, start + check.text.length), check.text, `${fixture.file}: stale fixture offset`);
        const actual = scopes.slice(start, start + check.text.length);
        if (actual.some(scope => scope !== check.scope)) failures.push(`${fixture.file}: ${check.text}@${start} expected ${check.scope}, got ${JSON.stringify(actual)}`);
        count++;
      }
    }
    assert.deepEqual(failures, [], failures.join('\n'));
    console.log(`TextMate scopes passed: ${cases.length} files, ${count} assertions.`);
  } finally {
    registry.dispose();
  }
};
