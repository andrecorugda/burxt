#!/usr/bin/env node
// Drive extension.js against a real `burxt lsp`, with a stub `vscode` module.
//
// Why this exists: the extension's client is hand-written framing and request
// bookkeeping, and the failure modes (a message split across chunks, a byte
// length applied to a string, a promise never resolved) are exactly the ones that
// look fine on inspection. VS Code cannot be scripted here; the client can.
//
//   node editors/vscode/test/harness.js [path-to-burxt]
//
// Exits non-zero on the first failed expectation.

const Module = require("module");
const path = require("path");
const assert = require("assert");

const compiler = process.argv[2] || path.join(__dirname, "../../../target/debug/burxt");

// ---- the smallest `vscode` that extension.js actually uses ----
const listeners = {};
const on = (name) => (cb) => {
  (listeners[name] ||= []).push(cb);
  return { dispose() {} };
};
const fire = (name, arg) => (listeners[name] || []).forEach((cb) => cb(arg));

const published = new Map(); // uri -> diagnostics
let hoverProvider = null;
const warnings = [];

class Position {
  constructor(line, character) {
    this.line = line;
    this.character = character;
  }
}
class Range {
  constructor(start, end) {
    this.start = start;
    this.end = end;
  }
}

const vscode = {
  Position,
  Range,
  Uri: { parse: (s) => ({ toString: () => s, __uri: s }) },
  MarkdownString: class {
    constructor(value) {
      this.value = value;
    }
  },
  Hover: class {
    constructor(contents, range) {
      this.contents = contents;
      this.range = range;
    }
  },
  Diagnostic: class {
    constructor(range, message, severity) {
      this.range = range;
      this.message = message;
      this.severity = severity;
    }
  },
  DiagnosticSeverity: { Error: 0 },
  languages: {
    createDiagnosticCollection: () => ({
      set: (uri, list) => published.set(uri.__uri ?? uri.toString(), list),
      delete: (uri) => published.delete(uri.toString()),
      dispose() {},
    }),
    registerHoverProvider: (_lang, provider) => {
      hoverProvider = provider;
      return { dispose() {} };
    },
  },
  window: { showWarningMessage: (m) => warnings.push(m) },
  commands: { registerCommand: () => ({ dispose() {} }) },
  workspace: {
    workspaceFolders: undefined,
    textDocuments: [],
    getConfiguration: () => ({ get: () => compiler }),
    onDidOpenTextDocument: on("open"),
    onDidChangeTextDocument: on("change"),
    onDidSaveTextDocument: on("save"),
    onDidCloseTextDocument: on("close"),
  },
};

const load = Module._load;
Module._load = function (request, parent, isMain) {
  return request === "vscode" ? vscode : load(request, parent, isMain);
};

const extension = require(path.join(__dirname, "../extension.js"));

// ---- a document the editor would hand over ----
function doc(text, version = 1) {
  return {
    languageId: "burxt",
    version,
    uri: { toString: () => "file:///tmp/harness.bx", __uri: "file:///tmp/harness.bx" },
    getText: () => text,
  };
}
const URI = "file:///tmp/harness.bx";
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const VALID = "let price: Decimal<2, RoundHalfEven> = $19.99;\nlet qty: Int = 3;\nlet total: Decimal<2, RoundHalfEven> = price * qty;\n";
const BROKEN = "let price: Decimal<2> = $19.99;\nlet wrong: Bool = 2;\n";

(async () => {
  extension.activate({ subscriptions: [] });
  await wait(400);

  fire("open", doc(VALID));
  await wait(400);
  assert.deepStrictEqual(published.get(URI), [], "a valid buffer must publish an EMPTY list");
  console.log("ok  valid buffer -> no diagnostics");

  fire("change", { document: doc(BROKEN, 2) });
  await wait(400);
  const found = published.get(URI);
  assert.ok(found && found.length === 1, `expected one diagnostic, got ${JSON.stringify(found)}`);
  assert.ok(/declared Bool/.test(found[0].message), `unexpected message: ${found[0].message}`);
  assert.strictEqual(found[0].range.start.line, 1, "the error is on the second line");
  assert.strictEqual(found[0].range.start.character, 18, "the caret blames the value `2`");
  console.log("ok  broken buffer -> one diagnostic, positioned at the value");

  fire("change", { document: doc(VALID, 3) });
  await wait(400);
  assert.deepStrictEqual(published.get(URI), [], "fixing the code must CLEAR the squiggle");
  console.log("ok  fixed buffer -> squiggle cleared");

  // Hover on `price` inside `price * qty` on the third line.
  const line = VALID.split("\n")[2];
  const hover = await hoverProvider.provideHover(doc(VALID, 3), new Position(2, line.indexOf("price * qty")));
  assert.ok(hover, "expected a hover result");
  assert.ok(/Decimal<2, RoundHalfEven>/.test(hover.contents.value), `unexpected hover: ${hover.contents.value}`);
  assert.ok(/half to even/.test(hover.contents.value), "the hover must explain the contract");
  console.log("ok  hover -> exact type and its rounding contract");

  // Hover on whitespace claims nothing.
  const nothing = await hoverProvider.provideHover(doc(VALID, 3), new Position(1, 17));
  assert.strictEqual(nothing, null, "hover on whitespace must return null, not a guess");
  console.log("ok  hover on nothing -> null");

  assert.deepStrictEqual(warnings, [], `the server should not have warned: ${warnings}`);
  extension.deactivate();
  console.log("\nall extension checks passed");
  process.exit(0);
})().catch((e) => {
  console.error("FAILED:", e.message);
  process.exit(1);
});
