using System.Text.Json;

namespace CookYourGame.Lsp;

internal static class Program
{
    private static int Main()
    {
        var input = new BufferedStream(Console.OpenStandardInput(), 64 * 1024);
        var output = new BufferedStream(Console.OpenStandardOutput(), 64 * 1024);
        var connection = new JsonRpcConnection(input, output);
        var server = new CygLspServer(connection);
        return server.Run();
    }
}

internal sealed class CygLspServer
{
    private readonly JsonRpcConnection _connection;
    private readonly CygLanguageService _service = new();
    private bool _shutdownReceived;
    private bool _exitReceived;

    public CygLspServer(JsonRpcConnection connection) => _connection = connection;

    public int Run()
    {
        while (!_exitReceived)
        {
            JsonDocument? message;
            try
            {
                message = _connection.Read();
            }
            catch (Exception ex)
            {
                Log("transport error: " + ex.Message);
                return 1;
            }

            if (message == null)
            {
                break;
            }

            using (message)
            {
                try
                {
                    Handle(message.RootElement);
                }
                catch (Exception ex)
                {
                    Log("unhandled message error: " + ex);
                }
            }
        }

        return _exitReceived ? (_shutdownReceived ? 0 : 1) : 0;
    }

    private void Handle(JsonElement root)
    {
        if (root.ValueKind != JsonValueKind.Object
            || !root.TryGetProperty("method", out JsonElement methodElement)
            || methodElement.ValueKind != JsonValueKind.String)
        {
            return;
        }

        string method = methodElement.GetString() ?? string.Empty;
        bool hasId = root.TryGetProperty("id", out JsonElement id)
            && id.ValueKind != JsonValueKind.Null;
        JsonElement parameters = root.TryGetProperty("params", out JsonElement value)
            ? value
            : default;

        if (method == "exit")
        {
            _exitReceived = true;
            return;
        }

        try
        {
            if (_shutdownReceived && method != "shutdown")
            {
                return;
            }

            object? result = Dispatch(method, parameters);
            if (hasId)
            {
                _connection.SendResult(id, result);
            }
        }
        catch (LspRequestException ex)
        {
            if (hasId)
            {
                _connection.SendError(id, ex.Code, ex.Message);
            }
        }
        catch (Exception ex)
        {
            Log($"'{method}' failed: {ex.GetType().Name}: {ex.Message}");
            if (hasId)
            {
                _connection.SendError(id, -32603, ex.Message);
            }
        }
    }

    private object? Dispatch(string method, JsonElement parameters) => method switch
    {
        "initialize" => Initialize(parameters),
        "initialized" => null,
        "shutdown" => Shutdown(),
        "textDocument/didOpen" => DidOpen(parameters),
        "textDocument/didChange" => DidChange(parameters),
        "textDocument/didClose" => DidClose(parameters),
        "textDocument/didSave" => null,
        "textDocument/completion" => Completion(parameters),
        "textDocument/hover" => Hover(parameters),
        "textDocument/definition" => Definition(parameters),
        "textDocument/references" => References(parameters),
        "textDocument/prepareRename" => PrepareRename(parameters),
        "textDocument/rename" => Rename(parameters),
        "textDocument/documentSymbol" => DocumentSymbols(parameters),
        "textDocument/semanticTokens/full" => SemanticTokens(parameters),
        "workspace/symbol" => WorkspaceSymbols(parameters),
        "workspace/didChangeWatchedFiles" => WatchedFilesChanged(parameters),
        "$/cancelRequest" => null,
        "$/setTrace" => null,
        "window/workDoneProgress/cancel" => null,
        _ => throw new LspRequestException(-32601, "Method not found: " + method),
    };

    private object Initialize(JsonElement parameters)
    {
        var folders = new List<string>();
        if (parameters.ValueKind == JsonValueKind.Object)
        {
            if (parameters.TryGetProperty("workspaceFolders", out JsonElement workspaceFolders)
                && workspaceFolders.ValueKind == JsonValueKind.Array)
            {
                foreach (JsonElement folder in workspaceFolders.EnumerateArray())
                {
                    if (folder.TryGetProperty("uri", out JsonElement uri)
                        && uri.GetString() is { } value)
                    {
                        folders.Add(Uris.ToPath(value));
                    }
                }
            }
            else if (parameters.TryGetProperty("rootUri", out JsonElement rootUri)
                && rootUri.ValueKind == JsonValueKind.String
                && rootUri.GetString() is { } root)
            {
                folders.Add(Uris.ToPath(root));
            }
            else if (parameters.TryGetProperty("rootPath", out JsonElement rootPath)
                && rootPath.ValueKind == JsonValueKind.String
                && rootPath.GetString() is { } path)
            {
                folders.Add(path);
            }
        }

        _service.ConfigureWorkspace(folders);

        return new
        {
            capabilities = new
            {
                textDocumentSync = new { openClose = true, change = 1 },
                completionProvider = new
                {
                    triggerCharacters = new[] { "<", ":", ",", "." },
                    resolveProvider = false,
                },
                hoverProvider = true,
                definitionProvider = true,
                referencesProvider = true,
                documentSymbolProvider = true,
                renameProvider = new { prepareProvider = true },
                workspaceSymbolProvider = true,
                semanticTokensProvider = new
                {
                    legend = CygLanguageService.SemanticTokensLegend,
                    full = true,
                },
            },
            serverInfo = new { name = "CYG Language Server", version = "0.1.0" },
        };
    }

    private object? Shutdown()
    {
        _shutdownReceived = true;
        return null;
    }

    private object? DidOpen(JsonElement parameters)
    {
        JsonElement document = parameters.GetProperty("textDocument");
        string uri = document.GetProperty("uri").GetString() ?? string.Empty;
        string text = document.GetProperty("text").GetString() ?? string.Empty;
        int version = document.TryGetProperty("version", out JsonElement versionElement)
            ? versionElement.GetInt32()
            : 0;
        CygDocument opened = _service.Open(uri, text, version);
        Publish(opened);
        return null;
    }

    private object? DidChange(JsonElement parameters)
    {
        JsonElement document = parameters.GetProperty("textDocument");
        string uri = document.GetProperty("uri").GetString() ?? string.Empty;
        int version = document.TryGetProperty("version", out JsonElement versionElement)
            ? versionElement.GetInt32()
            : 0;
        JsonElement changes = parameters.GetProperty("contentChanges");
        if (changes.GetArrayLength() == 0)
        {
            return null;
        }

        JsonElement change = changes[changes.GetArrayLength() - 1];
        if (!change.TryGetProperty("text", out JsonElement textElement))
        {
            Log("incremental change received but full sync was negotiated; ignoring");
            return null;
        }

        string text = textElement.GetString() ?? string.Empty;
        CygDocument? updated = _service.Change(uri, text, version);
        if (updated != null)
        {
            Publish(updated);
        }

        return null;
    }

    private object? DidClose(JsonElement parameters)
    {
        string uri = parameters.GetProperty("textDocument").GetProperty("uri").GetString()
            ?? string.Empty;
        _service.Close(uri);
        _connection.SendNotification("textDocument/publishDiagnostics", new
        {
            uri,
            diagnostics = Array.Empty<object>(),
        });
        return null;
    }

    private object? Completion(JsonElement parameters)
    {
        CygDocument? document = FindDocument(parameters, out int line, out int character);
        return document == null ? null : _service.Completion(document, line, character);
    }

    private object? Hover(JsonElement parameters)
    {
        CygDocument? document = FindDocument(parameters, out int line, out int character);
        return document == null ? null : _service.Hover(document, line, character);
    }

    private object? Definition(JsonElement parameters)
    {
        CygDocument? document = FindDocument(parameters, out int line, out int character);
        return document == null ? null : _service.Definition(document, line, character);
    }

    private object References(JsonElement parameters)
    {
        CygDocument? document = FindDocument(parameters, out int line, out int character);
        if (document == null)
        {
            return Array.Empty<object>();
        }

        bool includeDeclaration = parameters.TryGetProperty("context", out JsonElement context)
            && context.TryGetProperty("includeDeclaration", out JsonElement include)
            && include.ValueKind == JsonValueKind.True;
        return _service.References(document, line, character, includeDeclaration);
    }

    private object? PrepareRename(JsonElement parameters)
    {
        CygDocument? document = FindDocument(parameters, out int line, out int character);
        return document == null ? null : _service.PrepareRename(document, line, character);
    }

    private object Rename(JsonElement parameters)
    {
        CygDocument? document = FindDocument(parameters, out int line, out int character);
        if (document == null)
        {
            throw new LspRequestException(-32602, "The document is not open.");
        }

        string newName = parameters.TryGetProperty("newName", out JsonElement value)
            ? value.GetString() ?? string.Empty
            : string.Empty;
        return _service.Rename(document, line, character, newName);
    }

    private object DocumentSymbols(JsonElement parameters)
    {
        string uri = parameters.GetProperty("textDocument").GetProperty("uri").GetString()
            ?? string.Empty;
        CygDocument? document = _service.Get(uri);
        return document == null ? Array.Empty<object>() : _service.DocumentSymbols(document);
    }

    private object SemanticTokens(JsonElement parameters)
    {
        string uri = parameters.GetProperty("textDocument").GetProperty("uri").GetString()
            ?? string.Empty;
        CygDocument? document = _service.Get(uri);
        return document == null
            ? new { data = Array.Empty<int>() }
            : _service.SemanticTokens(document);
    }

    private object WorkspaceSymbols(JsonElement parameters)
    {
        string query = parameters.TryGetProperty("query", out JsonElement value)
            ? value.GetString() ?? string.Empty
            : string.Empty;
        return _service.WorkspaceSymbols(query);
    }

    private object? WatchedFilesChanged(JsonElement parameters)
    {
        if (parameters.TryGetProperty("changes", out JsonElement changes)
            && changes.ValueKind == JsonValueKind.Array)
        {
            foreach (JsonElement change in changes.EnumerateArray())
            {
                if (change.TryGetProperty("uri", out JsonElement uri)
                    && uri.GetString() is { } value)
                {
                    _service.InvalidateWorkspaceFile(value);
                }
            }
        }

        return null;
    }

    private CygDocument? FindDocument(JsonElement parameters, out int line, out int character)
    {
        string uri = parameters.GetProperty("textDocument").GetProperty("uri").GetString()
            ?? string.Empty;
        JsonElement position = parameters.GetProperty("position");
        line = position.GetProperty("line").GetInt32();
        character = position.GetProperty("character").GetInt32();
        return _service.Get(uri);
    }

    private void Publish(CygDocument document)
    {
        _connection.SendNotification("textDocument/publishDiagnostics", new
        {
            uri = document.Uri,
            version = document.Version,
            diagnostics = document.Diagnostics(),
        });
    }

    private static void Log(string message) => Console.Error.WriteLine("[cyg-lsp] " + message);
}
