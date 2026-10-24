using System.Text;
using CookYourGame.Dsl;
using CookYourGame.Dsl.Core;

namespace CookYourGame.Lsp;

internal enum CygSymbolKind
{
    Namespace,
    Asset,
    Struct,
    Component,
    Enum,
    Flags,
    Union,
    UnionVariant,
    Input,
    Global,
    Event,
    SyncedEvent,
    Command,
    Signal,
    Field,
    Button,
    EnumMember,
    SignalParameter,
}

internal sealed class CygSymbol
{
    public required string Name { get; init; }
    public required CygSymbolKind Kind { get; init; }
    public required CygRange NameRange { get; init; }
    public required CygRange Range { get; init; }
    public CygSymbol? Parent { get; init; }
    public List<CygSymbol> Children { get; } = new();
    public string TypeDisplay { get; init; } = string.Empty;
    public long? Value { get; init; }
    public long? Id { get; init; }
    public string? Underlying { get; init; }
    public bool IsDeclaration => Kind != CygSymbolKind.Field
        && Kind != CygSymbolKind.Button
        && Kind != CygSymbolKind.EnumMember
        && Kind != CygSymbolKind.SignalParameter;

    public string Signature => Kind switch
    {
        CygSymbolKind.Asset => "asset " + Name,
        CygSymbolKind.Struct => "struct " + Name,
        CygSymbolKind.Component => Id.HasValue ? $"component {Name} : {Id.Value}" : "component " + Name,
        CygSymbolKind.Enum => $"enum {Name} : {Underlying ?? "Int32"}",
        CygSymbolKind.Flags => $"flags {Name} : {Underlying ?? "Int32"}",
        CygSymbolKind.Union => "union " + Name,
        CygSymbolKind.UnionVariant => "struct " + Name,
        CygSymbolKind.Input => "input",
        CygSymbolKind.Global => "global",
        CygSymbolKind.Event => Id.HasValue ? $"event {Name} : {Id.Value}" : "event " + Name,
        CygSymbolKind.SyncedEvent => Id.HasValue ? $"synced_event {Name} : {Id.Value}" : "synced_event " + Name,
        CygSymbolKind.Command => Id.HasValue ? $"command {Name} : {Id.Value}" : "command " + Name,
        CygSymbolKind.Signal => "signal " + Name,
        CygSymbolKind.Namespace => "namespace " + Name,
        CygSymbolKind.Field => TypeDisplay + " " + Name,
        CygSymbolKind.Button => "button " + Name,
        CygSymbolKind.EnumMember => Value.HasValue ? $"{Name} = {Value.Value}" : Name,
        CygSymbolKind.SignalParameter => TypeDisplay + " " + Name,
        _ => Name,
    };
}

internal sealed class CygReference
{
    public required string Name { get; init; }
    public required CygRange Range { get; init; }
    public required bool IsAsset { get; init; }
}

internal readonly record struct CygTokenData(CygTokenKind Kind, string Text, long Number, CygSpan Span, CygRange Range)
{
    public bool IsIdentifier(string text) => Kind == CygTokenKind.Identifier && Text == text;
}

/// <summary>
/// Positional index over one .cyg file. Built from lexer tokens rather than the
/// AST so it still works while the file has syntax errors.
/// </summary>
internal sealed class CygDocumentIndex
{
    private static readonly HashSet<string> BlockKeywords = new(StringComparer.Ordinal)
    {
        "struct", "component", "enum", "flags", "union", "input", "global", "event", "synced_event", "command",
    };

    private static readonly HashSet<string> ReservedTopLevel = new(StringComparer.Ordinal)
    {
        "rpc",
    };

    private readonly List<CygSymbol> _allSymbols = new();
    private readonly HashSet<int> _excludedReferenceTokens = new();

    private CygDocumentIndex(string path, List<CygTokenData> tokens)
    {
        Path = path;
        Tokens = tokens;
    }

    public string Path { get; }
    public IReadOnlyList<CygTokenData> Tokens { get; }
    public List<CygSymbol> Declarations { get; } = new();
    public List<CygReference> References { get; } = new();
    public Dictionary<string, CygSymbol> Types { get; } = new(StringComparer.Ordinal);
    public Dictionary<string, CygSymbol> Assets { get; } = new(StringComparer.Ordinal);
    public IEnumerable<CygSymbol> AllSymbols => _allSymbols;

    public static CygDocumentIndex Build(string text, string path)
    {
        var diagnostics = new CygDiagnosticBag();
        var lexer = new CygLexer(text, path);
        List<CygToken> raw = lexer.Tokenize(diagnostics);
        var tokens = new List<CygTokenData>(raw.Count);
        foreach (CygToken token in raw)
        {
            if (token.Kind == CygTokenKind.EndOfFile)
            {
                continue;
            }

            tokens.Add(new CygTokenData(
                token.Kind,
                token.Text,
                token.Number,
                token.Span,
                LspText.FromSpan(token.Span)));
        }

        var index = new CygDocumentIndex(path, tokens);
        index.BuildDeclarations();
        index.BuildReferences();
        return index;
    }

    public CygSymbol? SymbolAt(int line, int character)
    {
        foreach (CygSymbol symbol in _allSymbols)
        {
            if (symbol.NameRange.ContainsInclusiveEnd(line, character))
            {
                return symbol;
            }
        }

        return null;
    }

    public CygReference? ReferenceAt(int line, int character)
    {
        foreach (CygReference reference in References)
        {
            if (reference.Range.ContainsInclusiveEnd(line, character))
            {
                return reference;
            }
        }

        return null;
    }

    public CygTokenData? TokenAt(int line, int character)
    {
        foreach (CygTokenData token in Tokens)
        {
            if (token.Range.ContainsInclusiveEnd(line, character))
            {
                return token;
            }
        }

        return null;
    }

    public CygSymbol? FindType(string name) => Types.GetValueOrDefault(name);

    public CygSymbol? FindAsset(string name) => Assets.GetValueOrDefault(name);

    public CygReference? ReferenceOf(CygSymbol symbol)
    {
        foreach (CygReference reference in References)
        {
            if (reference.Name == symbol.Name && reference.IsAsset == (symbol.Kind == CygSymbolKind.Asset))
            {
                return reference;
            }
        }

        return null;
    }

    private void BuildDeclarations()
    {
        int i = 0;
        while (i < Tokens.Count)
        {
            CygTokenData token = Tokens[i];
            if (token.Kind != CygTokenKind.Identifier)
            {
                i++;
                continue;
            }

            switch (token.Text)
            {
                case "namespace":
                    i = ParseNamespace(i);
                    break;
                case "asset":
                    i = ParseAsset(i);
                    break;
                case "signal":
                    i = ParseSignal(i);
                    break;
                case "struct":
                case "component":
                case "enum":
                case "flags":
                case "union":
                case "input":
                case "global":
                case "event":
                case "synced_event":
                case "command":
                    i = ParseBlockDeclaration(i);
                    break;
                case "rpc":
                    i = SkipDeclaration(i);
                    break;
                default:
                    i++;
                    break;
            }
        }
    }

    private int ParseNamespace(int keywordIndex)
    {
        int end = FindSemicolon(keywordIndex, Tokens.Count);
        var parts = new List<string>();
        CygTokenData? first = null;
        for (int i = keywordIndex + 1; i < end; i++)
        {
            if (Tokens[i].Kind == CygTokenKind.Identifier)
            {
                parts.Add(Tokens[i].Text);
                first ??= Tokens[i];
                _excludedReferenceTokens.Add(i);
            }
        }

        _excludedReferenceTokens.Add(keywordIndex);
        for (int i = keywordIndex + 1; i <= Math.Min(end, Tokens.Count - 1); i++)
        {
            _excludedReferenceTokens.Add(i);
        }

        if (first.HasValue && parts.Count > 0)
        {
            string name = string.Join(".", parts);
            var keyword = Tokens[keywordIndex];
            CygTokenData last = end < Tokens.Count ? Tokens[end] : Tokens[^1];
            var symbol = new CygSymbol
            {
                Name = name,
                Kind = CygSymbolKind.Namespace,
                NameRange = first.Value.Range,
                Range = Union(keyword, last),
            };
            AddTopLevel(symbol);
        }

        return end < Tokens.Count ? end + 1 : Tokens.Count;
    }

    private int ParseAsset(int keywordIndex)
    {
        int nameIndex = FindHeaderIdentifier(keywordIndex + 1);
        if (nameIndex < 0)
        {
            return SkipDeclaration(keywordIndex);
        }

        CygTokenData keyword = Tokens[keywordIndex];
        CygTokenData name = Tokens[nameIndex];
        int end = FindSemicolon(nameIndex + 1, Tokens.Count);
        CygTokenData last = end < Tokens.Count ? Tokens[end] : name;
        var symbol = new CygSymbol
        {
            Name = name.Text,
            Kind = CygSymbolKind.Asset,
            NameRange = name.Range,
            Range = Union(keyword, last),
        };
        AddTopLevel(symbol);
        return end < Tokens.Count ? end + 1 : Tokens.Count;
    }

    private int ParseBlockDeclaration(int keywordIndex)
    {
        CygTokenData keyword = Tokens[keywordIndex];
        CygSymbolKind kind = KindOf(keyword.Text);
        bool anonymous = kind is CygSymbolKind.Input or CygSymbolKind.Global;
        int nameIndex = anonymous ? keywordIndex : FindHeaderIdentifier(keywordIndex + 1);
        if (nameIndex < 0)
        {
            return SkipDeclaration(keywordIndex);
        }

        CygTokenData name = Tokens[nameIndex];
        int open = nameIndex + 1;
        while (open < Tokens.Count
            && Tokens[open].Kind != CygTokenKind.LBrace
            && Tokens[open].Kind != CygTokenKind.Semicolon)
        {
            open++;
        }

        if (open >= Tokens.Count || Tokens[open].Kind != CygTokenKind.LBrace)
        {
            return open < Tokens.Count ? open + 1 : Tokens.Count;
        }

        int close = FindMatching(open, CygTokenKind.LBrace, CygTokenKind.RBrace);
        if (close < 0)
        {
            close = Tokens.Count - 1;
        }

        int? id = null;
        string? underlying = null;
        for (int i = nameIndex + 1; i < open; i++)
        {
            if (Tokens[i].Kind != CygTokenKind.Colon || i + 1 >= open)
            {
                continue;
            }

            CygTokenData value = Tokens[i + 1];
            if ((kind is CygSymbolKind.Component or CygSymbolKind.Event or CygSymbolKind.SyncedEvent
                    or CygSymbolKind.Command)
                && value.Kind == CygTokenKind.Number)
            {
                id = value.Number is >= int.MinValue and <= int.MaxValue ? (int)value.Number : null;
            }
            else if (kind is CygSymbolKind.Enum or CygSymbolKind.Flags && value.Kind == CygTokenKind.Identifier)
            {
                underlying = value.Text;
            }
        }

        if (kind == CygSymbolKind.Enum && underlying == null)
        {
            underlying = "Int32";
        }

        CygTokenData closing = Tokens[Math.Max(0, close)];
        var symbol = new CygSymbol
        {
            Name = name.Text,
            Kind = kind,
            NameRange = name.Range,
            Range = Union(keyword, closing),
            Id = id,
            Underlying = underlying,
        };
        AddTopLevel(symbol);

        switch (kind)
        {
            case CygSymbolKind.Enum:
            case CygSymbolKind.Flags:
                ExtractEnumMembers(open + 1, close, symbol);
                break;
            case CygSymbolKind.Union:
                ExtractUnionVariants(open + 1, close, symbol);
                break;
            case CygSymbolKind.Input:
                ExtractFields(open + 1, close, symbol, buttons: true);
                break;
            default:
                ExtractFields(open + 1, close, symbol, buttons: false);
                break;
        }

        return close + 1;
    }

    private int ParseSignal(int keywordIndex)
    {
        int nameIndex = FindHeaderIdentifier(keywordIndex + 1, stopAtParen: true);
        if (nameIndex < 0)
        {
            return SkipDeclaration(keywordIndex);
        }

        int open = nameIndex + 1;
        while (open < Tokens.Count
            && Tokens[open].Kind != CygTokenKind.LParen
            && Tokens[open].Kind != CygTokenKind.Semicolon)
        {
            open++;
        }

        if (open >= Tokens.Count || Tokens[open].Kind != CygTokenKind.LParen)
        {
            return open < Tokens.Count ? open + 1 : Tokens.Count;
        }

        int close = FindMatching(open, CygTokenKind.LParen, CygTokenKind.RParen);
        if (close < 0)
        {
            close = Tokens.Count - 1;
        }

        int end = FindSemicolon(close + 1, Tokens.Count);
        CygTokenData keyword = Tokens[keywordIndex];
        CygTokenData last = end < Tokens.Count ? Tokens[end] : Tokens[Math.Max(0, close)];
        var symbol = new CygSymbol
        {
            Name = Tokens[nameIndex].Text,
            Kind = CygSymbolKind.Signal,
            NameRange = Tokens[nameIndex].Range,
            Range = Union(keyword, last),
        };
        AddTopLevel(symbol);
        ExtractSignalParameters(open + 1, close, symbol);
        return end < Tokens.Count ? end + 1 : Tokens.Count;
    }

    private void ExtractSignalParameters(int start, int end, CygSymbol parent)
    {
        int segmentStart = start;
        int angleDepth = 0;
        for (int i = start; i <= end; i++)
        {
            bool boundary = i == end
                || (Tokens[i].Kind == CygTokenKind.Comma && angleDepth == 0);
            if (!boundary)
            {
                if (Tokens[i].Kind == CygTokenKind.LAngle)
                {
                    angleDepth++;
                }
                else if (Tokens[i].Kind == CygTokenKind.RAngle && angleDepth > 0)
                {
                    angleDepth--;
                }

                continue;
            }

            AddParameter(segmentStart, i, parent);
            segmentStart = i + 1;
        }
    }

    private void AddParameter(int start, int end, CygSymbol parent)
    {
        int nameIndex = -1;
        for (int i = end - 1; i >= start; i--)
        {
            if (Tokens[i].Kind == CygTokenKind.Identifier)
            {
                nameIndex = i;
                break;
            }
        }

        if (nameIndex <= start)
        {
            return;
        }

        string type = FormatType(start, nameIndex);
        if (type.Length == 0)
        {
            return;
        }

        var symbol = new CygSymbol
        {
            Name = Tokens[nameIndex].Text,
            Kind = CygSymbolKind.SignalParameter,
            NameRange = Tokens[nameIndex].Range,
            Range = Union(Tokens[start], Tokens[end - 1]),
            Parent = parent,
            TypeDisplay = type,
        };
        AddMember(parent, symbol);
    }

    private void ExtractUnionVariants(int start, int end, CygSymbol parent)
    {
        int i = start;
        while (i < end)
        {
            if (!Tokens[i].IsIdentifier("struct"))
            {
                i++;
                continue;
            }

            int nameIndex = FindNextIdentifier(i + 1, end);
            if (nameIndex < 0)
            {
                i++;
                continue;
            }

            int open = nameIndex + 1;
            while (open < end && Tokens[open].Kind != CygTokenKind.LBrace)
            {
                open++;
            }

            if (open >= end)
            {
                i++;
                continue;
            }

            int close = FindMatching(open, CygTokenKind.LBrace, CygTokenKind.RBrace);
            if (close < 0 || close > end)
            {
                close = end - 1;
            }

            var variant = new CygSymbol
            {
                Name = Tokens[nameIndex].Text,
                Kind = CygSymbolKind.UnionVariant,
                NameRange = Tokens[nameIndex].Range,
                Range = Union(Tokens[i], Tokens[Math.Max(i, close)]),
                Parent = parent,
            };
            AddMember(parent, variant);
            ExtractFields(open + 1, close, variant, buttons: false);
            i = close + 1;
        }
    }

    private void ExtractEnumMembers(int start, int end, CygSymbol parent)
    {
        bool flags = parent.Kind == CygSymbolKind.Flags;
        long next = 0;
        long nextBit = 1;
        int i = start;
        while (i < end)
        {
            if (Tokens[i].Kind != CygTokenKind.Identifier)
            {
                i++;
                continue;
            }

            CygTokenData name = Tokens[i];
            int j = i + 1;
            long? explicitValue = null;
            int lastIndex = i;
            if (j < end && Tokens[j].Kind == CygTokenKind.Equals)
            {
                j++;
                bool negative = false;
                if (j < end && Tokens[j].Kind == CygTokenKind.Minus)
                {
                    negative = true;
                    j++;
                }

                if (j < end && Tokens[j].Kind == CygTokenKind.Number)
                {
                    explicitValue = negative ? -Tokens[j].Number : Tokens[j].Number;
                    lastIndex = j;
                    j++;
                }
            }

            long value;
            if (flags)
            {
                if (explicitValue.HasValue)
                {
                    value = explicitValue.Value;
                }
                else if (string.Equals(name.Text, "None", StringComparison.OrdinalIgnoreCase))
                {
                    value = 0;
                }
                else
                {
                    value = nextBit;
                    nextBit = nextBit > (long.MaxValue >> 1) ? long.MaxValue : nextBit << 1;
                }
            }
            else
            {
                value = explicitValue ?? next;
                next = value == long.MaxValue ? long.MaxValue : value + 1;
            }

            var member = new CygSymbol
            {
                Name = name.Text,
                Kind = CygSymbolKind.EnumMember,
                NameRange = name.Range,
                Range = Union(name, Tokens[lastIndex]),
                Parent = parent,
                Value = value,
            };
            AddMember(parent, member);
            i = j;
        }
    }

    private void ExtractFields(int start, int end, CygSymbol parent, bool buttons)
    {
        int statementStart = start;
        int level = 0;
        for (int i = start; i < end; i++)
        {
            if (Tokens[i].Kind == CygTokenKind.LBrace)
            {
                level++;
            }
            else if (Tokens[i].Kind == CygTokenKind.RBrace)
            {
                level = Math.Max(0, level - 1);
            }
            else if (Tokens[i].Kind == CygTokenKind.Semicolon && level == 0)
            {
                AddField(statementStart, i, parent, buttons);
                statementStart = i + 1;
            }
        }
    }

    private void AddField(int start, int end, CygSymbol parent, bool buttons)
    {
        if (end <= start)
        {
            return;
        }

        int nameIndex = -1;
        for (int i = end - 1; i >= start; i--)
        {
            if (Tokens[i].Kind == CygTokenKind.Identifier)
            {
                nameIndex = i;
                break;
            }
        }

        if (nameIndex < 0)
        {
            return;
        }

        CygTokenData name = Tokens[nameIndex];
        CygTokenData last = Tokens[Math.Max(start, end - 1)];
        bool isButton = buttons && Tokens[start].IsIdentifier("button");
        string type = isButton ? "button" : FormatType(start, nameIndex);
        if (type.Length == 0)
        {
            return;
        }

        var field = new CygSymbol
        {
            Name = name.Text,
            Kind = isButton ? CygSymbolKind.Button : CygSymbolKind.Field,
            NameRange = name.Range,
            Range = Union(Tokens[start], last),
            Parent = parent,
            TypeDisplay = type,
        };
        AddMember(parent, field);
    }

    private string FormatType(int start, int end)
    {
        var builder = new StringBuilder();
        for (int i = start; i < end; i++)
        {
            CygTokenData token = Tokens[i];
            switch (token.Kind)
            {
                case CygTokenKind.Identifier:
                    builder.Append(token.Text);
                    break;
                case CygTokenKind.Number:
                    builder.Append(token.Number);
                    break;
                case CygTokenKind.Comma:
                    builder.Append(", ");
                    break;
                case CygTokenKind.LAngle:
                    builder.Append('<');
                    break;
                case CygTokenKind.RAngle:
                    builder.Append('>');
                    break;
                case CygTokenKind.LBracket:
                    builder.Append('[');
                    break;
                case CygTokenKind.RBracket:
                    builder.Append(']');
                    break;
                case CygTokenKind.Dot:
                    builder.Append('.');
                    break;
                case CygTokenKind.Minus:
                    builder.Append('-');
                    break;
                case CygTokenKind.Colon:
                    builder.Append(": ");
                    break;
                default:
                    break;
            }
        }

        return builder.ToString().Trim();
    }

    private void BuildReferences()
    {
        foreach (CygSymbol symbol in _allSymbols)
        {
            _excludedReferenceTokens.Add(IndexOf(symbol.NameRange));
        }

        var assetTargets = new HashSet<int>();
        for (int i = 0; i + 2 < Tokens.Count; i++)
        {
            if (!Tokens[i].IsIdentifier("asset_ref")
                || Tokens[i + 1].Kind != CygTokenKind.LAngle
                || Tokens[i + 2].Kind != CygTokenKind.Identifier)
            {
                continue;
            }

            CygTokenData name = Tokens[i + 2];
            References.Add(new CygReference
            {
                Name = name.Text,
                Range = name.Range,
                IsAsset = true,
            });
            assetTargets.Add(i + 2);
        }

        for (int i = 0; i < Tokens.Count; i++)
        {
            CygTokenData token = Tokens[i];
            if (token.Kind != CygTokenKind.Identifier
                || _excludedReferenceTokens.Contains(i)
                || assetTargets.Contains(i))
            {
                continue;
            }

            if (!Types.ContainsKey(token.Text))
            {
                continue;
            }

            References.Add(new CygReference
            {
                Name = token.Text,
                Range = token.Range,
                IsAsset = false,
            });
        }

        References.Sort((left, right) => LspText.Compare(left.Range, right.Range));
    }

    private int IndexOf(CygRange range)
    {
        for (int i = 0; i < Tokens.Count; i++)
        {
            if (Tokens[i].Range == range)
            {
                return i;
            }
        }

        return -1;
    }

    private void AddTopLevel(CygSymbol symbol)
    {
        Declarations.Add(symbol);
        _allSymbols.Add(symbol);
        if (symbol.Kind == CygSymbolKind.Asset)
        {
            Assets.TryAdd(symbol.Name, symbol);
        }
        else if (symbol.Kind is CygSymbolKind.Struct
            or CygSymbolKind.Component
            or CygSymbolKind.Enum
            or CygSymbolKind.Flags
            or CygSymbolKind.Union)
        {
            Types.TryAdd(symbol.Name, symbol);
        }
    }

    private void AddMember(CygSymbol parent, CygSymbol member)
    {
        parent.Children.Add(member);
        _allSymbols.Add(member);
    }

    private int FindNextIdentifier(int start, int end)
    {
        for (int i = start; i < end; i++)
        {
            if (Tokens[i].Kind == CygTokenKind.Identifier)
            {
                return i;
            }
        }

        return -1;
    }

    private int FindHeaderIdentifier(int start, bool stopAtParen = false)
    {
        for (int i = start; i < Tokens.Count; i++)
        {
            if (Tokens[i].Kind is CygTokenKind.LBrace or CygTokenKind.Semicolon)
            {
                return -1;
            }

            if (stopAtParen && Tokens[i].Kind == CygTokenKind.LParen)
            {
                return -1;
            }

            if (Tokens[i].Kind == CygTokenKind.Identifier)
            {
                return i;
            }
        }

        return -1;
    }

    private int FindSemicolon(int start, int end)
    {
        for (int i = start; i < end; i++)
        {
            if (Tokens[i].Kind == CygTokenKind.Semicolon)
            {
                return i;
            }
        }

        return end;
    }

    private int FindMatching(int openIndex, CygTokenKind openKind, CygTokenKind closeKind)
    {
        int depth = 0;
        for (int i = openIndex; i < Tokens.Count; i++)
        {
            if (Tokens[i].Kind == openKind)
            {
                depth++;
            }
            else if (Tokens[i].Kind == closeKind)
            {
                depth--;
                if (depth == 0)
                {
                    return i;
                }
            }
        }

        return -1;
    }

    private int SkipDeclaration(int keywordIndex)
    {
        int i = keywordIndex + 1;
        while (i < Tokens.Count
            && Tokens[i].Kind != CygTokenKind.Semicolon
            && Tokens[i].Kind != CygTokenKind.LBrace)
        {
            i++;
        }

        if (i < Tokens.Count && Tokens[i].Kind == CygTokenKind.LBrace)
        {
            int close = FindMatching(i, CygTokenKind.LBrace, CygTokenKind.RBrace);
            return close >= 0 ? close + 1 : Tokens.Count;
        }

        return i < Tokens.Count ? i + 1 : Tokens.Count;
    }

    private static CygRange Union(CygTokenData start, CygTokenData end)
        => new(start.Range.StartLine, start.Range.StartCharacter, end.Range.EndLine, end.Range.EndCharacter);

    public static bool IsBlockKeyword(string text) => BlockKeywords.Contains(text);

    public static bool IsReservedTopLevel(string text) => ReservedTopLevel.Contains(text);

    private static CygSymbolKind KindOf(string keyword) => keyword switch
    {
        "component" => CygSymbolKind.Component,
        "enum" => CygSymbolKind.Enum,
        "flags" => CygSymbolKind.Flags,
        "union" => CygSymbolKind.Union,
        "input" => CygSymbolKind.Input,
        "global" => CygSymbolKind.Global,
        "event" => CygSymbolKind.Event,
        "synced_event" => CygSymbolKind.SyncedEvent,
        "command" => CygSymbolKind.Command,
        _ => CygSymbolKind.Struct,
    };
}
