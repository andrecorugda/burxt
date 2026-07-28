// Burxt language support for VS Code: live diagnostics and hover, from the
// compiler itself.
//
// Deliberately dependency-free. This is a hand-written LSP client — about a
// hundred lines of framing and request bookkeeping — instead of
// `vscode-languageclient`, because that package would bring npm, a lock file and
// a bundling step, and the property worth protecting here is that the extension
// is a directory you can copy into place and use.
//
// It talks to `burxt lsp` over stdio, which is the same server every other editor
// uses. That matters more than the line count: there is one place where "what
// does the compiler know about this buffer" is answered, and every editor asks it
// the same way.
//
// (`burxt check <file> --json` still exists for tasks and CI, and the `$burxt`
// problem matcher still works — but the editor path is the server.)

const vscode = require("vscode");
const os = require("os");
const { spawn } = require("child_process");
const fs = require("fs");
const path = require("path");

/** Language id contributed in package.json. */
const LANGUAGE = "burxt";

let client;
let diagnostics;

function compilerPath() {
  const configured = vscode.workspace.getConfiguration("burxt").get("path");
  if (configured && configured.trim() !== "") {
    return configured;
  }
  // A contributor to the language itself will have a workspace build.
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (folder) {
    const local = path.join(folder.uri.fsPath, "target", "debug", "burxt");
    try {
      if (fs.existsSync(local)) return local;
    } catch {
      // Fall through to PATH.
    }
  }
  return "burxt";
}

/**
 * A minimal JSON-RPC-over-stdio client: frame messages out, unframe them in,
 * match responses to requests by id, and hand notifications to a callback.
 */
class Client {
  constructor(command, onNotification, onExit) {
    this.command = command;
    this.onNotification = onNotification;
    this.onExit = onExit;
    this.nextId = 1;
    this.pending = new Map();
    // Bytes, not a string: Content-Length counts bytes, and slicing a string on
    // a byte count corrupts every message containing a non-ASCII character.
    this.buffer = Buffer.alloc(0);
    this.child = null;
  }

  start() {
    this.child = spawn(this.command, ["lsp"]);
    this.child.stdout.on("data", (chunk) => this.receive(chunk));
    this.child.on("error", (e) => this.onExit(e.message));
    this.child.on("close", () => this.onExit(null));
    this.child.stdin.on("error", () => {}); // The server may exit mid-write.
  }

  stop() {
    if (!this.child) return;
    try {
      // Ask politely, then let the pipe close finish the job.
      this.request("shutdown", null).catch(() => {});
      this.notify("exit", null);
    } catch {
      // Already gone.
    }
    this.child.stdin.end();
    this.child = null;
  }

  send(message) {
    if (!this.child) return;
    const body = Buffer.from(JSON.stringify(message), "utf8");
    this.child.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
    this.child.stdin.write(body);
  }

  notify(method, params) {
    this.send({ jsonrpc: "2.0", method, params });
  }

  request(method, params) {
    const id = this.nextId++;
    const promise = new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
    this.send({ jsonrpc: "2.0", id, method, params });
    return promise;
  }

  receive(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    // A single chunk may hold several messages, or half of one.
    for (;;) {
      const headerEnd = this.buffer.indexOf("\r\n\r\n");
      if (headerEnd === -1) return;
      const headers = this.buffer.subarray(0, headerEnd).toString("ascii");
      const match = /Content-Length:\s*(\d+)/i.exec(headers);
      if (!match) {
        // Unframed output means something other than a server is on the pipe.
        this.buffer = Buffer.alloc(0);
        return;
      }
      const length = Number(match[1]);
      const bodyStart = headerEnd + 4;
      if (this.buffer.length < bodyStart + length) return; // Wait for the rest.
      const body = this.buffer.subarray(bodyStart, bodyStart + length).toString("utf8");
      this.buffer = this.buffer.subarray(bodyStart + length);
      let message;
      try {
        message = JSON.parse(body);
      } catch {
        continue; // Not something we can act on; keep going rather than stall.
      }
      if (message.id !== undefined && this.pending.has(message.id)) {
        const { resolve, reject } = this.pending.get(message.id);
        this.pending.delete(message.id);
        message.error ? reject(message.error) : resolve(message.result);
      } else if (message.method) {
        this.onNotification(message);
      }
    }
  }
}

function isBurxt(document) {
  return document && document.languageId === LANGUAGE;
}

function applyDiagnostics(params) {
  const uri = vscode.Uri.parse(params.uri);
  const entries = (params.diagnostics || []).map((d) => {
    const range = new vscode.Range(
      new vscode.Position(d.range.start.line, d.range.start.character),
      new vscode.Position(d.range.end.line, d.range.end.character)
    );
    // The server sends severity 1 (Error) only — Burxt has no warnings, because
    // every diagnostic it can produce is a refusal to compile.
    const entry = new vscode.Diagnostic(range, d.message, vscode.DiagnosticSeverity.Error);
    entry.source = d.source || "burxt";
    return entry;
  });
  // Setting an EMPTY list is what clears the squiggle when the code becomes
  // valid, so this path must run for the empty case too.
  diagnostics.set(uri, entries);
}

function openDocument(document) {
  if (!isBurxt(document) || !client) return;
  client.notify("textDocument/didOpen", {
    textDocument: {
      uri: document.uri.toString(),
      languageId: LANGUAGE,
      version: document.version,
      text: document.getText(),
    },
  });
}

// One terminal, reused, so running twice does not leave two behind.
let terminal;

function runFile(command) {
  const editor = vscode.window.activeTextEditor;
  if (!editor || !isBurxt(editor.document)) {
    vscode.window.showWarningMessage("Burxt: open a `.bx` file first.");
    return;
  }
  // Unsaved changes would compile the file on disk instead of the one on screen, which
  // is the most confusing possible outcome. Save first, then run.
  editor.document.save().then(() => {
    if (!terminal || terminal.exitStatus !== undefined) {
      terminal = vscode.window.createTerminal("Burxt");
    }
    terminal.show(true);
    const file = editor.document.uri.fsPath;
    // `-o` into the temp directory, so running a file does not leave an executable next
    // to it. Without this the compiler writes into the terminal's working directory,
    // which is how this repository's own root collected twenty-six of them.
    const stem = path.basename(file).replace(/\.bx$/, "");
    const out = path.join(os.tmpdir(), `burxt-${stem}`);
    const quote = (s) => (/[\s"']/.test(s) ? `'${s.replace(/'/g, "'\\''")}'` : s);
    terminal.sendText(
      `${quote(compilerPath())} ${command} ${quote(file)} -o ${quote(out)}`
    );
  });
}

function activate(context) {
  diagnostics = vscode.languages.createDiagnosticCollection(LANGUAGE);
  context.subscriptions.push(diagnostics);

  let warned = false;
  const onExit = (message) => {
    if (warned) return;
    warned = true;
    vscode.window.showWarningMessage(
      `Burxt: the language server stopped${message ? ` (${message})` : ""}. ` +
        `Set "burxt.path" in settings, or build the compiler with \`cargo build\`.`
    );
  };

  client = new Client(
    compilerPath(),
    (message) => {
      if (message.method === "textDocument/publishDiagnostics") {
        applyDiagnostics(message.params);
      }
    },
    onExit
  );
  client.start();
  client.request("initialize", { processId: process.pid, capabilities: {} }).then(
    () => {
      client.notify("initialized", {});
      vscode.workspace.textDocuments.forEach(openDocument);
    },
    () => onExit("initialize failed")
  );

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(openDocument),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (!isBurxt(e.document) || !client) return;
      // The server asks for full sync, so the whole buffer goes every time —
      // which is also why it can never disagree with what is on screen.
      client.notify("textDocument/didChange", {
        textDocument: { uri: e.document.uri.toString(), version: e.document.version },
        contentChanges: [{ text: e.document.getText() }],
      });
    }),
    vscode.workspace.onDidSaveTextDocument((document) => {
      if (!isBurxt(document) || !client) return;
      client.notify("textDocument/didSave", {
        textDocument: { uri: document.uri.toString() },
      });
    }),
    vscode.workspace.onDidCloseTextDocument((document) => {
      if (!isBurxt(document) || !client) return;
      client.notify("textDocument/didClose", {
        textDocument: { uri: document.uri.toString() },
      });
      diagnostics.delete(document.uri);
    }),
    vscode.languages.registerHoverProvider(LANGUAGE, {
      async provideHover(document, position) {
        if (!client) return null;
        let result;
        try {
          result = await client.request("textDocument/hover", {
            textDocument: { uri: document.uri.toString() },
            position: { line: position.line, character: position.character },
          });
        } catch {
          return null;
        }
        if (!result || !result.contents) return null;
        const markdown = new vscode.MarkdownString(result.contents.value);
        const range = result.range
          ? new vscode.Range(
              new vscode.Position(result.range.start.line, result.range.start.character),
              new vscode.Position(result.range.end.line, result.range.end.character)
            )
          : undefined;
        return new vscode.Hover(markdown, range);
      },
    }),
    // Running the file you are looking at, which is the thing a newcomer tries first.
    // A terminal rather than an output channel: a Burxt program writes to stdout and
    // may read arguments, so it belongs somewhere a person can see it and type into it.
    vscode.commands.registerCommand("burxt.run", () => runFile("run")),
    vscode.commands.registerCommand("burxt.build", () => runFile("build")),

    vscode.commands.registerCommand("burxt.restartServer", () => {
      client?.stop();
      warned = false;
      client = new Client(
        compilerPath(),
        (m) =>
          m.method === "textDocument/publishDiagnostics" && applyDiagnostics(m.params),
        onExit
      );
      client.start();
      client.request("initialize", { processId: process.pid, capabilities: {} }).then(() => {
        client.notify("initialized", {});
        vscode.workspace.textDocuments.forEach(openDocument);
      });
    })
  );
}

function deactivate() {
  client?.stop();
  client = null;
  diagnostics?.dispose();
}

module.exports = { activate, deactivate };
