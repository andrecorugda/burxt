// Live diagnostics for Burxt in VS Code.
//
// Deliberately dependency-free: plain CommonJS against the `vscode` API, which
// the editor injects at runtime. No `npm install`, no `node_modules`, no
// bundler — the extension stays a directory you can copy into place, which is
// the property that makes it easy to try.
//
// It shells out to `burxt check - --json` and feeds it the BUFFER on stdin, so
// errors describe what you are looking at rather than what was last saved.
// (`burxt lsp` is the real language server and any other editor should use it;
// wiring it in here would mean vscode-languageclient, npm, and a build step for
// exactly the same squiggles.)

const vscode = require("vscode");
const { spawn } = require("child_process");
const path = require("path");

/** How long to wait after the last keystroke before checking. */
const DEBOUNCE_MS = 250;

let diagnostics;
let pending;
/** Set once, after the first failure to launch, so the warning appears once. */
let warnedAboutBinary = false;

function compilerPath() {
  const configured = vscode.workspace.getConfiguration("burxt").get("path");
  if (configured && configured.trim() !== "") {
    return configured;
  }
  // A workspace build is what a contributor to the language itself will have.
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (folder) {
    const local = path.join(folder.uri.fsPath, "target", "debug", "burxt");
    try {
      if (require("fs").existsSync(local)) {
        return local;
      }
    } catch {
      // Fall through to PATH.
    }
  }
  return "burxt";
}

/**
 * Run the compiler's front end over `text` and resolve with the diagnostics it
 * reported. Resolves with an empty array when the program is legal — clearing
 * the squiggles matters as much as showing them.
 */
function check(text) {
  return new Promise((resolve) => {
    let child;
    try {
      child = spawn(compilerPath(), ["check", "-", "--json"]);
    } catch (e) {
      resolve({ error: e.message, diagnostics: [] });
      return;
    }

    let out = "";
    child.stdout.on("data", (chunk) => (out += chunk));
    child.on("error", (e) => resolve({ error: e.message, diagnostics: [] }));
    child.on("close", () => {
      const found = [];
      for (const line of out.split("\n")) {
        if (line.trim() === "") continue;
        let d;
        try {
          d = JSON.parse(line);
        } catch {
          continue; // Not a diagnostic; ignore rather than fail loudly.
        }
        // The compiler emits LSP-ready 0-based positions precisely so this
        // conversion is not done here, where an off-by-one would live.
        const start = new vscode.Position(d.lspStart?.line ?? 0, d.lspStart?.character ?? 0);
        const end = new vscode.Position(d.lspEnd?.line ?? 0, d.lspEnd?.character ?? 0);
        const entry = new vscode.Diagnostic(
          new vscode.Range(start, end),
          d.message,
          vscode.DiagnosticSeverity.Error
        );
        entry.source = "burxt";
        found.push(entry);
      }
      resolve({ diagnostics: found });
    });

    child.stdin.on("error", () => {}); // The child may exit before we finish writing.
    child.stdin.end(text);
  });
}

async function refresh(document) {
  if (!document || document.languageId !== "burxt") {
    return;
  }
  const result = await check(document.getText());
  if (result.error) {
    if (!warnedAboutBinary) {
      warnedAboutBinary = true;
      vscode.window.showWarningMessage(
        `Burxt: could not run the compiler (${result.error}). ` +
          `Set "burxt.path" in settings, or build it with \`cargo build\`.`
      );
    }
    // No compiler means no information — leave whatever was shown alone rather
    // than clearing it, which would look like the code became valid.
    return;
  }
  diagnostics.set(document.uri, result.diagnostics);
}

function scheduleRefresh(document) {
  clearTimeout(pending);
  pending = setTimeout(() => refresh(document), DEBOUNCE_MS);
}

function activate(context) {
  diagnostics = vscode.languages.createDiagnosticCollection("burxt");
  context.subscriptions.push(diagnostics);

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(refresh),
    vscode.workspace.onDidSaveTextDocument(refresh),
    vscode.workspace.onDidChangeTextDocument((e) => scheduleRefresh(e.document)),
    // A closed document's problems must go, or the panel keeps errors for a file
    // that is no longer open.
    vscode.workspace.onDidCloseTextDocument((doc) => diagnostics.delete(doc.uri)),
    vscode.commands.registerCommand("burxt.check", () =>
      refresh(vscode.window.activeTextEditor?.document)
    )
  );

  // Anything already open when the extension activates.
  vscode.workspace.textDocuments.forEach(refresh);
}

function deactivate() {
  clearTimeout(pending);
  diagnostics?.dispose();
}

module.exports = { activate, deactivate };
