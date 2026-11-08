use std::path::{Path, PathBuf};

/// Writes a Node.js mock LSP server script to `dir` supporting configurable behavior via environment variables:
/// - `MOCK_LSP_DELAY_MS`: delays `textDocument/hover` responses to test request timeouts.
/// - `MOCK_LSP_ERROR_HOVER`: returns a JSON-RPC error on `textDocument/hover`.
pub fn write_mock_lsp_server(dir: &Path) -> PathBuf {
    let script_path = dir.join("mock-lsp.mjs");
    let script = r#"import { Buffer } from 'node:buffer';

let buffer = Buffer.alloc(0);

function send(msg) {
  const body = Buffer.from(JSON.stringify(msg), 'utf8');
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}

function computeDiagnostics(text) {
  const diagnostics = [];
  const lines = text.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const errIdx = lines[i].indexOf('BROKEN');
    if (errIdx !== -1) {
      diagnostics.push({
        range: {
          start: { line: i, character: errIdx },
          end: { line: i, character: errIdx + 6 },
        },
        severity: 1,
        source: 'mock-lsp',
        code: 'E001',
        message: 'Unresolved symbol BROKEN',
      });
    }
    const warnIdx = lines[i].indexOf('UNUSED');
    if (warnIdx !== -1) {
      diagnostics.push({
        range: {
          start: { line: i, character: warnIdx },
          end: { line: i, character: warnIdx + 6 },
        },
        severity: 2,
        source: 'mock-lsp',
        code: 'W001',
        message: 'Unused symbol UNUSED',
      });
    }
  }
  return diagnostics;
}

function handleMessage(msg) {
  if (msg.method === 'initialize') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        capabilities: {
          textDocumentSync: 1,
          hoverProvider: true,
          definitionProvider: true,
        },
      },
    });
    return;
  }
  if (msg.method === 'textDocument/didOpen' || msg.method === 'textDocument/didChange') {
    const uri = msg.params.textDocument.uri;
    const text = msg.method === 'textDocument/didOpen'
      ? msg.params.textDocument.text
      : msg.params.contentChanges[0].text;
    send({
      jsonrpc: '2.0',
      method: 'textDocument/publishDiagnostics',
      params: { uri, diagnostics: computeDiagnostics(text) },
    });
    return;
  }
  if (msg.method === 'textDocument/didClose') {
    const uri = msg.params.textDocument.uri;
    send({
      jsonrpc: '2.0',
      method: 'textDocument/publishDiagnostics',
      params: { uri, diagnostics: [] },
    });
    return;
  }
  if (msg.method === 'textDocument/hover') {
    if (process.env.MOCK_LSP_ERROR_HOVER === '1') {
      send({
        jsonrpc: '2.0',
        id: msg.id,
        error: { code: -32603, message: 'Mock hover protocol failure' },
      });
      return;
    }
    const delayMs = Number(process.env.MOCK_LSP_DELAY_MS || '0');
    const respond = () => {
      send({
        jsonrpc: '2.0',
        id: msg.id,
        result: {
          contents: {
            kind: 'markdown',
            value: '`fn helper() -> usize`',
          },
          range: {
            start: { line: msg.params.position.line, character: 0 },
            end: { line: msg.params.position.line, character: 6 },
          },
        },
      });
    };
    if (delayMs > 0) {
      setTimeout(respond, delayMs);
    } else {
      respond();
    }
    return;
  }
  if (msg.method === 'textDocument/definition') {
    const targetUri = process.env.MOCK_LSP_DEF_URI || msg.params.textDocument.uri;
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: [
        {
          uri: targetUri,
          range: {
            start: { line: 0, character: 3 },
            end: { line: 0, character: 9 },
          },
        },
      ],
    });
    return;
  }
  if (msg.method === 'textDocument/completion') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        isIncomplete: false,
        items: [
          {
            label: 'helper',
            kind: 3,
            detail: 'fn helper() -> usize',
            documentation: {
              kind: 'markdown',
              value: 'Computes the helper value.',
            },
            textEdit: {
              range: {
                start: { line: msg.params.position.line, character: 4 },
                end: { line: msg.params.position.line, character: msg.params.position.character },
              },
              newText: 'helper()',
            },
          },
        ],
      },
    });
    return;
  }
  if (msg.method === 'textDocument/signatureHelp') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        signatures: [
          {
            label: 'fn helper(count: usize, label: &str) -> usize',
            documentation: {
              kind: 'markdown',
              value: 'Runs `helper` with a count and label.',
            },
            parameters: [
              { label: 'count: usize' },
              { label: 'label: &str' },
            ],
          },
        ],
        activeSignature: 0,
        activeParameter: 1,
      },
    });
    return;
  }
  if (msg.method === 'shutdown') {
    send({ jsonrpc: '2.0', id: msg.id, result: null });
    return;
  }
  if (msg.method === 'exit') {
    process.exit(0);
  }
}

process.stdin.on('data', (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  while (true) {
    const headerEnd = buffer.indexOf('\r\n\r\n');
    if (headerEnd === -1) break;
    const header = buffer.slice(0, headerEnd).toString('utf8');
    const match = header.match(/Content-Length:\s*(\d+)/i);
    if (!match) break;
    const length = Number(match[1]);
    const total = headerEnd + 4 + length;
    if (buffer.length < total) break;
    const body = buffer.slice(headerEnd + 4, total).toString('utf8');
    buffer = buffer.slice(total);
    handleMessage(JSON.parse(body));
  }
});
"#;
    std::fs::write(&script_path, script).unwrap();
    script_path
}
