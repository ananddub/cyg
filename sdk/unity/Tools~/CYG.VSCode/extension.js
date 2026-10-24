'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const vscode = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;
let serverChannel;

function expandHome(value) {
  if (!value) {
    return value;
  }

  if (value === '~') {
    return os.homedir();
  }

  if (value.startsWith('~/') || value.startsWith('~\\')) {
    return path.join(os.homedir(), value.slice(2));
  }

  return value;
}

function resolveServerPath(context) {
  const configured = expandHome(vscode.workspace.getConfiguration('cyg.lsp').get('serverPath', '').trim());
  if (configured.length > 0) {
    return { path: configured, source: 'cyg.lsp.serverPath' };
  }

  const candidates = [
    path.join(context.extensionPath, 'server', 'CYG.Lsp.dll'),
    path.join(context.extensionPath, '..', 'CYG.Lsp', 'bin', 'Release', 'net10.0', 'CYG.Lsp.dll'),
    path.join(context.extensionPath, '..', 'CYG.Lsp', 'bin', 'Debug', 'net10.0', 'CYG.Lsp.dll'),
  ];

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return { path: candidate, source: candidate };
    }
  }

  return { path: null, source: null };
}

function createClient(context, serverPath) {
  const dotnetPath = expandHome(vscode.workspace.getConfiguration('cyg.lsp').get('dotnetPath', 'dotnet').trim()) || 'dotnet';
  const options = { cwd: path.dirname(serverPath) };
  const transportOptions = {
    command: dotnetPath,
    args: [serverPath],
    transport: TransportKind.stdio,
    options,
  };

  return new LanguageClient(
    'cyg',
    'CYG Language Server',
    { run: transportOptions, debug: transportOptions },
    {
      documentSelector: [
        { scheme: 'file', language: 'cyg' },
        { scheme: 'untitled', language: 'cyg' },
      ],
      synchronize: {
        fileEvents: vscode.workspace.createFileSystemWatcher('**/*.cyg'),
      },
      outputChannel: serverChannel,
      traceOutputChannel: serverChannel,
    }
  );
}

async function startClient(context) {
  const resolved = resolveServerPath(context);
  if (!resolved.path) {
    const choice = await vscode.window.showErrorMessage(
      'CYG language server was not found. Build it with "npm run build:server" in the extension folder, or set cyg.lsp.serverPath.',
      'Open Settings'
    );
    if (choice === 'Open Settings') {
      await vscode.commands.executeCommand('workbench.action.openSettings', 'cyg.lsp.serverPath');
    }
    return;
  }

  if (!fs.existsSync(resolved.path)) {
    vscode.window.showErrorMessage(`CYG language server not found at ${resolved.path}.`);
    return;
  }

  serverChannel.appendLine(`Starting CYG language server: ${resolved.path}`);
  client = createClient(context, resolved.path);
  try {
    await client.start();
  } catch (error) {
    const failed = client;
    client = undefined;
    try {
      await failed.stop();
    } catch {
      // The client never started; there is nothing to stop.
    }
    throw error;
  }
}

async function restartClient(context) {
  try {
    if (client) {
      await client.restart();
    } else {
      await startClient(context);
    }
    vscode.window.showInformationMessage('CYG language server restarted.');
  } catch (error) {
    vscode.window.showErrorMessage(`Could not restart the CYG language server: ${error && error.message ? error.message : error}`);
  }
}

async function activate(context) {
  serverChannel = vscode.window.createOutputChannel('CYG Language Server', { log: true });
  context.subscriptions.push(serverChannel);
  context.subscriptions.push(
    vscode.commands.registerCommand('cyg.restartServer', () => restartClient(context))
  );
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration('cyg.lsp') && client) {
        restartClient(context);
      }
    })
  );

  try {
    await startClient(context);
  } catch (error) {
    serverChannel.appendLine(error && error.stack ? error.stack : String(error));
    vscode.window.showErrorMessage(
      `Could not start the CYG language server: ${error && error.message ? error.message : error}`
    );
  }
}

async function deactivate() {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

module.exports = { activate, deactivate };
