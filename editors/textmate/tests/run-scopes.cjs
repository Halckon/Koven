const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
require('./verify-scopes.cjs').verifyScopes({
  grammarPath: resolve(__dirname, '../syntaxes/koven.tmLanguage.json'),
  fixturePath: resolve(__dirname, 'engine-scopes.json'),
  textmate: require('vscode-textmate'),
  oniguruma: require('vscode-oniguruma'),
  wasm: readFileSync(require.resolve('vscode-oniguruma/release/onig.wasm')),
}).catch(error => { console.error(error); process.exitCode = 1; });
