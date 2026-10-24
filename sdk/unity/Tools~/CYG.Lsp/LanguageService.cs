using CookYourGame.Dsl.Core;

namespace CookYourGame.Lsp;

internal sealed class LspRequestException : Exception
{
    public LspRequestException(int code, string message) : base(message) => Code = code;

    public int Code { get; }
}

internal sealed class CygLanguageService
{
    private static readonly string[] SemanticTokenTypes =
    {
        "namespace", "type", "enum", "enumMember", "event", "function", "parameter", "property",
    };

    private static readonly string[] SemanticTokenModifiers = { "declaration" };

    private readonly Dictionary<string, CygDocument> _documents = new(StringComparer.Ordinal);
    private readonly WorkspaceIndex _workspace = new();

    public static object SemanticTokensLegend => new
    {
        tokenTypes = SemanticTokenTypes,
        tokenModifiers = SemanticTokenModifiers,
    };

    public void ConfigureWorkspace(IEnumerable<string> folders) => _workspace.SetFolders(folders);

    public CygDocument Open(string uri, string text, int version)
    {
        CygDocument document = CygDocument.Parse(uri, text, version);
        _documents[uri] = document;
        return document;
    }

    public CygDocument? Change(string uri, string text, int version)
    {
        if (!_documents.ContainsKey(uri))
        {
            return null;
        }

        CygDocument document = CygDocument.Parse(uri, text, version);
        _documents[uri] = document;
        return document;
    }

    public void Close(string uri) => _documents.Remove(uri);

    public CygDocument? Get(string uri) => _documents.GetValueOrDefault(uri);

    public void InvalidateWorkspaceFile(string uri) => _workspace.Invalidate(uri);

    public object Completion(CygDocument document, int line, int character)
        => CygCompletion.Build(document, line, character);

    public object? Hover(CygDocument document, int line, int character)
    {
        CygSymbol? symbol = document.Index.SymbolAt(line, character);
        if (symbol != null)
        {
            return Hover(symbol, document.Index);
        }

        CygReference? reference = document.Index.ReferenceAt(line, character);
        if (reference != null)
        {
            CygSymbol? target = Resolve(document, reference);
            if (target != null)
            {
                return Hover(target, document.Index);
            }

            string what = reference.IsAsset ? "asset" : "type";
            return new
            {
                contents = LspText.Markup($"**Unknown {what}** `{reference.Name}`\n\nDeclare it in this file."),
                range = reference.Range.ToLsp(),
            };
        }

        CygTokenData? token = document.Index.TokenAt(line, character);
        if (token is { Kind: CygTokenKind.Identifier })
        {
            string? text = CygDocs.DescribeToken(token.Value.Text);
            if (text != null)
            {
                return new
                {
                    contents = LspText.Markup($"**`{token.Value.Text}`**\n\n{text}"),
                    range = token.Value.Range.ToLsp(),
                };
            }
        }

        return null;
    }

    public object? Definition(CygDocument document, int line, int character)
    {
        CygSymbol? symbol = document.Index.SymbolAt(line, character);
        if (symbol == null)
        {
            CygReference? reference = document.Index.ReferenceAt(line, character);
            if (reference != null)
            {
                symbol = Resolve(document, reference);
            }
        }

        if (symbol == null)
        {
            return null;
        }

        return new { uri = document.Uri, range = symbol.NameRange.ToLsp() };
    }

    public object References(
        CygDocument document,
        int line,
        int character,
        bool includeDeclaration)
    {
        CygSymbol? symbol = document.Index.SymbolAt(line, character);
        CygReference? reference = symbol == null
            ? document.Index.ReferenceAt(line, character)
            : null;
        if (symbol == null && reference == null)
        {
            return Array.Empty<object>();
        }

        string name = symbol?.Name ?? reference!.Name;
        bool isAsset = symbol?.Kind == CygSymbolKind.Asset || reference?.IsAsset == true;
        bool isTopLevel = symbol == null || symbol.Kind is CygSymbolKind.Asset
            or CygSymbolKind.Struct
            or CygSymbolKind.Component
            or CygSymbolKind.Enum
            or CygSymbolKind.Flags
            or CygSymbolKind.Union
            or CygSymbolKind.Event
            or CygSymbolKind.SyncedEvent
            or CygSymbolKind.Command
            or CygSymbolKind.Signal;

        var locations = new List<object>();
        if (includeDeclaration)
        {
            CygSymbol? declaration = symbol != null && symbol.IsDeclaration
                ? symbol
                : isAsset
                    ? document.Index.FindAsset(name)
                    : document.Index.FindType(name);
            if (declaration != null)
            {
                locations.Add(new { uri = document.Uri, range = declaration.NameRange.ToLsp() });
            }
        }

        if (isTopLevel)
        {
            foreach (CygReference item in document.Index.References)
            {
                if (item.Name == name && item.IsAsset == isAsset)
                {
                    locations.Add(new { uri = document.Uri, range = item.Range.ToLsp() });
                }
            }
        }

        return locations.ToArray();
    }

    public object? PrepareRename(CygDocument document, int line, int character)
    {
        CygSymbol? symbol = document.Index.SymbolAt(line, character);
        CygReference? reference = symbol == null
            ? document.Index.ReferenceAt(line, character)
            : null;
        if (symbol == null && reference == null)
        {
            return null;
        }

        if (symbol?.Kind == CygSymbolKind.Namespace)
        {
            return null;
        }

        if (reference != null && Resolve(document, reference) == null)
        {
            return null;
        }

        CygRange range = symbol?.NameRange ?? reference!.Range;
        string name = symbol?.Name ?? reference!.Name;
        return new { range = range.ToLsp(), placeholder = name };
    }

    public object Rename(
        CygDocument document,
        int line,
        int character,
        string newName)
    {
        if (!IsIdentifier(newName))
        {
            throw new LspRequestException(-32602,
                $"'{newName}' is not a valid CYG identifier (letters, digits and underscore, not starting with a digit).");
        }

        CygSymbol? symbol = document.Index.SymbolAt(line, character);
        CygReference? reference = symbol == null
            ? document.Index.ReferenceAt(line, character)
            : null;
        if (symbol == null && reference == null)
        {
            throw new LspRequestException(-32602, "There is no CYG declaration or reference at this position.");
        }

        if (symbol?.Kind == CygSymbolKind.Namespace)
        {
            throw new LspRequestException(-32602, "Renaming a namespace declaration is not supported.");
        }

        if (reference != null)
        {
            symbol = Resolve(document, reference);
            if (symbol == null)
            {
                throw new LspRequestException(-32602,
                    $"Cannot rename unknown {(reference.IsAsset ? "asset" : "type")} '{reference.Name}'.");
            }
        }

        bool isTopLevelType = symbol!.Kind is CygSymbolKind.Struct
            or CygSymbolKind.Component
            or CygSymbolKind.Enum
            or CygSymbolKind.Flags
            or CygSymbolKind.Union
            or CygSymbolKind.Event
            or CygSymbolKind.SyncedEvent
            or CygSymbolKind.Command
            or CygSymbolKind.Signal;
        bool isAsset = symbol.Kind == CygSymbolKind.Asset;

        if (isTopLevelType || isAsset)
        {
            if (CygDocs.IsKnownType(newName)
                || CygDocs.IsKnownKeyword(newName)
                || CygDocumentIndex.IsBlockKeyword(newName)
                || CygDocumentIndex.IsReservedTopLevel(newName)
                || newName is "signal" or "button")
            {
                throw new LspRequestException(-32602,
                    $"'{newName}' is a reserved or builtin CYG name.");
            }

            if (document.Index.Types.ContainsKey(newName) || document.Index.Assets.ContainsKey(newName))
            {
                throw new LspRequestException(-32602,
                    $"'{newName}' is already declared in this file.");
            }
        }

        if (newName == symbol.Name)
        {
            return new { changes = new Dictionary<string, object[]>() };
        }

        var edits = new List<CygRange> { symbol.NameRange };
        if (isTopLevelType || isAsset)
        {
            foreach (CygReference item in document.Index.References)
            {
                if (item.Name == symbol.Name && item.IsAsset == isAsset)
                {
                    edits.Add(item.Range);
                }
            }
        }

        var textEdits = edits
            .Distinct()
            .OrderBy(range => range.StartLine)
            .ThenBy(range => range.StartCharacter)
            .Select(range => (object)new { range = range.ToLsp(), newText = newName })
            .ToArray();

        return new
        {
            changes = new Dictionary<string, object[]> { [document.Uri] = textEdits },
        };
    }

    public object DocumentSymbols(CygDocument document)
        => document.Index.Declarations.Select(ToDocumentSymbol).ToArray();

    public object SemanticTokens(CygDocument document)
    {
        var entries = new List<(CygRange Range, int Type, int Modifiers)>();
        foreach (CygSymbol symbol in document.Index.AllSymbols)
        {
            int type = SemanticType(symbol.Kind);
            if (type >= 0 && symbol.NameRange.EndLine == symbol.NameRange.StartLine)
            {
                entries.Add((symbol.NameRange, type, 1));
            }
        }

        foreach (CygReference reference in document.Index.References)
        {
            if (reference.Range.EndLine == reference.Range.StartLine)
            {
                entries.Add((reference.Range, 1, 0));
            }
        }

        IEnumerable<(CygRange Range, int Type, int Modifiers)> ordered = entries
            .OrderBy(entry => entry.Range.StartLine)
            .ThenBy(entry => entry.Range.StartCharacter)
            .ThenByDescending(entry => entry.Range.LengthOnLine);

        var data = new List<int>();
        int lastLine = 0;
        int lastCharacter = 0;
        CygRange? previous = null;
        foreach ((CygRange range, int type, int modifiers) in ordered)
        {
            int length = range.EndCharacter - range.StartCharacter;
            if (length <= 0)
            {
                continue;
            }

            if (previous is { } last)
            {
                bool sameStart = range.StartLine == last.StartLine
                    && range.StartCharacter == last.StartCharacter;
                bool overlaps = range.StartLine == last.StartLine
                    && range.StartCharacter < last.EndCharacter;
                if (sameStart || overlaps)
                {
                    continue;
                }
            }

            int deltaLine = range.StartLine - lastLine;
            int deltaStart = deltaLine == 0
                ? range.StartCharacter - lastCharacter
                : range.StartCharacter;
            data.Add(deltaLine);
            data.Add(deltaStart);
            data.Add(length);
            data.Add(type);
            data.Add(modifiers);
            lastLine = range.StartLine;
            lastCharacter = range.StartCharacter;
            previous = range;
        }

        return new { data = data.ToArray() };
    }

    public object WorkspaceSymbols(string query)
    {
        List<(string Path, CygSymbol Symbol)> results = _workspace.Search(query ?? string.Empty);
        return results.Select(result => (object)new
        {
            name = result.Symbol.Name,
            kind = SymbolKind(result.Symbol.Kind),
            location = new
            {
                uri = Uris.FromPath(result.Path),
                range = result.Symbol.NameRange.ToLsp(),
            },
            containerName = Path.GetFileName(result.Path),
        }).ToArray();
    }

    private static CygSymbol? Resolve(CygDocument document, CygReference reference)
        => reference.IsAsset
            ? document.Index.FindAsset(reference.Name)
            : document.Index.FindType(reference.Name);

    private static object Hover(CygSymbol symbol, CygDocumentIndex index) => new
    {
        contents = LspText.Markup(CygSymbolDocs.Markdown(symbol, index)),
        range = symbol.NameRange.ToLsp(),
    };

    private static object ToDocumentSymbol(CygSymbol symbol) => new
    {
        name = symbol.Name,
        detail = symbol.Kind is CygSymbolKind.Field or CygSymbolKind.Button or CygSymbolKind.SignalParameter
            ? symbol.TypeDisplay
            : CygSymbolDocs.Detail(symbol),
        kind = SymbolKind(symbol.Kind),
        range = symbol.Range.ToLsp(),
        selectionRange = symbol.NameRange.ToLsp(),
        children = symbol.Children.Select(ToDocumentSymbol).ToArray(),
    };

    private static int SymbolKind(CygSymbolKind kind) => kind switch
    {
        CygSymbolKind.Namespace => 3,
        CygSymbolKind.Asset => 14,
        CygSymbolKind.Struct or CygSymbolKind.UnionVariant => 23,
        CygSymbolKind.Component => 5,
        CygSymbolKind.Enum or CygSymbolKind.Flags => 10,
        CygSymbolKind.Union => 11,
        CygSymbolKind.Input or CygSymbolKind.Global => 23,
        CygSymbolKind.Event or CygSymbolKind.SyncedEvent => 24,
        CygSymbolKind.Command => 6,
        CygSymbolKind.Signal => 12,
        CygSymbolKind.Field or CygSymbolKind.Button => 8,
        CygSymbolKind.EnumMember => 22,
        CygSymbolKind.SignalParameter => 13,
        _ => 13,
    };

    private static int SemanticType(CygSymbolKind kind) => kind switch
    {
        CygSymbolKind.Namespace => 0,
        CygSymbolKind.Asset => 1,
        CygSymbolKind.Struct => 1,
        CygSymbolKind.Component => 1,
        CygSymbolKind.Union => 1,
        CygSymbolKind.UnionVariant => 1,
        CygSymbolKind.Enum or CygSymbolKind.Flags => 2,
        CygSymbolKind.EnumMember => 3,
        CygSymbolKind.Event or CygSymbolKind.SyncedEvent or CygSymbolKind.Command => 4,
        CygSymbolKind.Signal => 5,
        CygSymbolKind.SignalParameter => 6,
        CygSymbolKind.Field or CygSymbolKind.Button => 7,
        _ => -1,
    };

    private static bool IsIdentifier(string name)
    {
        if (string.IsNullOrEmpty(name)
            || !(char.IsAsciiLetter(name[0]) || name[0] == '_'))
        {
            return false;
        }

        for (int i = 1; i < name.Length; i++)
        {
            char c = name[i];
            if (!(char.IsAsciiLetterOrDigit(c) || c == '_'))
            {
                return false;
            }
        }

        return true;
    }
}
