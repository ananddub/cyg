using CookYourGame.Dsl.Core;

namespace CookYourGame.Lsp;

internal enum CygCompletionKind
{
    None,
    TopLevel,
    TypePosition,
    AssetReference,
    EnumUnderlying,
    EnumMember,
    NumericCapacity,
    ElementId,
}

internal sealed class CygCompletionContext
{
    public CygCompletionKind Kind { get; private init; }
    public string Partial { get; private init; } = string.Empty;
    public bool InInput { get; private init; }
    public string? HeaderKeyword { get; private init; }

    public static CygCompletionContext Analyze(CygDocument document, int line, int character)
    {
        if (InComment(document.Text, document.Lines, line, character))
        {
            return new CygCompletionContext { Kind = CygCompletionKind.None };
        }

        IReadOnlyList<CygTokenData> tokens = document.Index.Tokens;
        int contextEnd = 0;
        while (contextEnd < tokens.Count)
        {
            CygRange range = tokens[contextEnd].Range;
            if (range.StartLine > line || (range.StartLine == line && range.StartCharacter > character))
            {
                break;
            }

            contextEnd++;
        }

        string partial = string.Empty;
        if (contextEnd > 0)
        {
            CygTokenData last = tokens[contextEnd - 1];
            if (last.Kind == CygTokenKind.Identifier
                && last.Range.ContainsInclusiveEnd(line, character))
            {
                int length = Math.Max(0, Math.Min(last.Text.Length, character - last.Range.StartCharacter));
                partial = last.Text[..length];
                contextEnd--;
            }
        }

        var blocks = new List<string>();
        var angles = new List<AngleState>();
        int parenDepth = 0;
        for (int i = 0; i < contextEnd; i++)
        {
            CygTokenData token = tokens[i];
            switch (token.Kind)
            {
                case CygTokenKind.LBrace:
                    blocks.Add(FindBlockKeyword(tokens, i) ?? "block");
                    break;
                case CygTokenKind.RBrace:
                    if (blocks.Count > 0)
                    {
                        blocks.RemoveAt(blocks.Count - 1);
                    }

                    break;
                case CygTokenKind.LParen:
                    parenDepth++;
                    break;
                case CygTokenKind.RParen:
                    if (parenDepth > 0)
                    {
                        parenDepth--;
                    }

                    break;
                case CygTokenKind.LAngle:
                    angles.Add(new AngleState(PreviousIdentifier(tokens, i)));
                    break;
                case CygTokenKind.RAngle:
                    if (angles.Count > 0)
                    {
                        angles.RemoveAt(angles.Count - 1);
                    }

                    break;
                case CygTokenKind.Comma:
                    if (angles.Count > 0)
                    {
                        angles[^1].SawComma = true;
                    }

                    break;
            }
        }

        if (angles.Count > 0)
        {
            string? head = angles[^1].Head;
            CygCompletionKind kind = head == "asset_ref"
                ? CygCompletionKind.AssetReference
                : head == "QString"
                    ? CygCompletionKind.NumericCapacity
                    : CygCompletionKind.TypePosition;
            return new CygCompletionContext
            {
                Kind = kind,
                Partial = partial,
                InInput = blocks.Count > 0 && blocks[^1] == "input",
            };
        }

        bool signalParameters = parenDepth > 0 && NearestKeyword(tokens, contextEnd) == "signal";
        if (signalParameters)
        {
            return new CygCompletionContext
            {
                Kind = CygCompletionKind.TypePosition,
                Partial = partial,
            };
        }

        if (blocks.Count == 0)
        {
            string? headerKeyword = contextEnd > 0
                && tokens[contextEnd - 1].Kind == CygTokenKind.Colon
                ? FindTopLevelKeyword(tokens, contextEnd)
                : null;
            CygCompletionKind kind = headerKeyword switch
            {
                "enum" or "flags" => CygCompletionKind.EnumUnderlying,
                "component" or "event" or "synced_event" => CygCompletionKind.ElementId,
                _ => CygCompletionKind.TopLevel,
            };
            return new CygCompletionContext
            {
                Kind = kind,
                Partial = partial,
                HeaderKeyword = headerKeyword,
            };
        }

        string block = blocks[^1];
        if (block is "enum" or "flags")
        {
            return new CygCompletionContext
            {
                Kind = CygCompletionKind.EnumMember,
                Partial = partial,
            };
        }

        return new CygCompletionContext
        {
            Kind = CygCompletionKind.TypePosition,
            Partial = partial,
            InInput = block == "input",
        };
    }

    private static string? FindBlockKeyword(IReadOnlyList<CygTokenData> tokens, int braceIndex)
    {
        for (int i = braceIndex - 1; i >= 0; i--)
        {
            CygTokenData token = tokens[i];
            if (token.Kind is CygTokenKind.LBrace or CygTokenKind.RBrace or CygTokenKind.Semicolon)
            {
                return null;
            }

            if (token.Kind == CygTokenKind.Identifier && CygDocumentIndex.IsBlockKeyword(token.Text))
            {
                return token.Text;
            }
        }

        return null;
    }

    private static string? PreviousIdentifier(IReadOnlyList<CygTokenData> tokens, int index)
    {
        for (int i = index - 1; i >= 0; i--)
        {
            if (tokens[i].Kind == CygTokenKind.Identifier)
            {
                return tokens[i].Text;
            }

            if (tokens[i].Kind is CygTokenKind.Semicolon or CygTokenKind.LBrace or CygTokenKind.RBrace)
            {
                return null;
            }
        }

        return null;
    }

    private static string? NearestKeyword(IReadOnlyList<CygTokenData> tokens, int contextEnd)
    {
        for (int i = contextEnd - 1; i >= 0; i--)
        {
            CygTokenData token = tokens[i];
            if (token.Kind is CygTokenKind.Semicolon or CygTokenKind.LBrace or CygTokenKind.RBrace)
            {
                return null;
            }

            if (token.IsIdentifier("signal"))
            {
                return "signal";
            }
        }

        return null;
    }

    private static string? FindTopLevelKeyword(IReadOnlyList<CygTokenData> tokens, int contextEnd)
    {
        for (int i = contextEnd - 1; i >= 0; i--)
        {
            CygTokenData token = tokens[i];
            if (token.Kind is CygTokenKind.Semicolon or CygTokenKind.LBrace or CygTokenKind.RBrace)
            {
                return null;
            }

            if (token.Kind == CygTokenKind.Identifier
                && (CygDocumentIndex.IsBlockKeyword(token.Text) || token.Text == "signal"))
            {
                return token.Text;
            }
        }

        return null;
    }

    private static bool InComment(string text, string[] lines, int line, int character)
    {
        int offset = 0;
        for (int i = 0; i < line && i < lines.Length; i++)
        {
            offset += lines[i].Length + 1;
        }

        offset += Math.Min(character, line >= 0 && line < lines.Length ? lines[line].Length : 0);
        offset = Math.Clamp(offset, 0, text.Length);

        bool block = false;
        bool lineComment = false;
        for (int i = 0; i < offset; i++)
        {
            char current = text[i];
            if (lineComment)
            {
                if (current == '\n')
                {
                    lineComment = false;
                }

                continue;
            }

            if (block)
            {
                if (current == '*' && i + 1 < offset && text[i + 1] == '/')
                {
                    block = false;
                    i++;
                }

                continue;
            }

            if (current == '/' && i + 1 < offset)
            {
                if (text[i + 1] == '/')
                {
                    lineComment = true;
                    i++;
                }
                else if (text[i + 1] == '*')
                {
                    block = true;
                    i++;
                }
            }
        }

        return block || lineComment;
    }

    private sealed class AngleState
    {
        public AngleState(string? head) => Head = head;
        public string? Head { get; }
        public bool SawComma { get; set; }
    }
}

internal static class CygCompletion
{
    public static object Build(CygDocument document, int line, int character)
    {
        CygCompletionContext context = CygCompletionContext.Analyze(document, line, character);
        var items = new List<object>();
        switch (context.Kind)
        {
            case CygCompletionKind.TopLevel:
                AddTopLevel(items);
                break;
            case CygCompletionKind.TypePosition:
                AddTypes(document, items, context.InInput);
                break;
            case CygCompletionKind.AssetReference:
                AddAssets(document, items, wrapped: false);
                break;
            case CygCompletionKind.EnumUnderlying:
                AddEnumUnderlyings(items);
                break;
            case CygCompletionKind.EnumMember:
                AddEnumMember(items);
                break;
            case CygCompletionKind.NumericCapacity:
                AddCapacities(items);
                break;
            case CygCompletionKind.ElementId:
                AddElementIds(items, context.HeaderKeyword);
                break;
        }

        return new { isIncomplete = false, items = items.ToArray() };
    }

    private static void AddTopLevel(List<object> items)
    {
        items.Add(Snippet(
            "namespace",
            "namespace CookYourGame.Sim.Generated;",
            "namespace ${1:CookYourGame.Sim.Generated};",
            "Emit generated types into this C# namespace. Must be the first declaration.",
            "0_01"));
        items.Add(Snippet(
            "asset",
            "asset WeaponSpec;",
            "asset ${1:WeaponSpec};",
            CygDocs.Keyword("asset"),
            "0_02"));
        items.Add(Snippet(
            "struct",
            "struct Name { ... }",
            """
            struct ${1:Name} {
            	$0
            }
            """,
            CygDocs.Keyword("struct"),
            "0_03"));
        items.Add(Snippet(
            "component",
            "component Name : 1000 { ... }",
            """
            component ${1:Name} : ${2:1000} {
            	$0
            }
            """,
            CygDocs.Keyword("component"),
            "0_04"));
        items.Add(Snippet(
            "enum",
            "enum Name : Byte { ... }",
            """
            enum ${1:Name} : ${2:Byte} {
            	${3:None} = 0,
            	$0
            }
            """,
            CygDocs.Keyword("enum"),
            "0_05"));
        items.Add(Snippet(
            "flags",
            "flags Name : Byte { ... }",
            """
            flags ${1:Name} : ${2:Byte} {
            	${3:None},
            	$0
            }
            """,
            CygDocs.Keyword("flags"),
            "0_06"));
        items.Add(Snippet(
            "union",
            "union Name { ... }",
            """
            union ${1:Name} {
            	struct ${2:VariantA} {
            		$0
            	}

            	struct ${3:VariantB} {
            	}
            }
            """,
            CygDocs.Keyword("union"),
            "0_07"));
        items.Add(Snippet(
            "input",
            "input { ... }",
            """
            input {
            	${1:button Jump};
            	$0
            }
            """,
            CygDocs.Keyword("input"),
            "0_08"));
        items.Add(Snippet(
            "global",
            "global { ... }",
            """
            global {
            	$0
            }
            """,
            CygDocs.Keyword("global"),
            "0_09"));
        items.Add(Snippet(
            "event",
            "event Name : 1000000 { ... }",
            """
            event ${1:Name} : ${2:1000000} {
            	$0
            }
            """,
            CygDocs.Keyword("event"),
            "0_10"));
        items.Add(Snippet(
            "synced_event",
            "synced_event Name : 1000000 { ... }",
            """
            synced_event ${1:Name} : ${2:1000000} {
            	$0
            }
            """,
            CygDocs.Keyword("synced_event"),
            "0_11"));
        items.Add(Snippet(
            "signal",
            "signal OnEvent(Int32 value);",
            "signal ${1:OnEvent}(${2:Int32 value});",
            CygDocs.Keyword("signal"),
            "0_12"));
    }

    private static void AddTypes(CygDocument document, List<object> items, bool inInput)
    {
        foreach (CygSymbol type in document.Index.Types.Values
            .OrderBy(symbol => symbol.Name, StringComparer.Ordinal))
        {
            items.Add(new
            {
                label = type.Name,
                kind = TypeCompletionKind(type.Kind),
                detail = CygSymbolDocs.Detail(type),
                documentation = LspText.Markup(CygSymbolDocs.Markdown(type, document.Index)),
                insertText = type.Name,
                sortText = "0_" + type.Name,
            });
        }

        AddAssets(document, items, wrapped: true);

        foreach (string type in CygDocs.BuiltinTypes)
        {
            items.Add(new
            {
                label = type,
                kind = 22,
                detail = "builtin type",
                documentation = LspText.Markup(CygDocs.Type(type) ?? string.Empty),
                insertText = type,
                sortText = "2_" + type,
            });
        }

        items.Add(Snippet(
            "array",
            "array<T>[N]",
            "array<${1:FP}>[${2:4}]",
            CygDocs.Type("array"),
            "3_01"));
        items.Add(Snippet(
            "list",
            "list<T>[N]",
            "list<${1:entity_ref}>[${2:16}]",
            CygDocs.Type("list"),
            "3_02"));
        items.Add(Snippet(
            "dictionary",
            "dictionary<K, V>[N]",
            "dictionary<${1:Int32}, ${2:FP}>[${3:8}]",
            CygDocs.Type("dictionary"),
            "3_03"));
        items.Add(Snippet(
            "QString",
            "QString<N>",
            "QString<${1:32}>",
            CygDocs.Type("QString"),
            "3_04"));
        items.Add(Snippet(
            "asset_ref",
            "asset_ref<Asset>",
            "asset_ref<${1:AssetName}>",
            CygDocs.Type("asset_ref"),
            "3_05"));

        if (inInput)
        {
            items.Add(Snippet(
                "button",
                "button Jump;",
                "button ${1:Jump};",
                CygDocs.Keyword("button"),
                "3_00"));
        }

        items.Add(Snippet(
            "field",
            "Type Name;",
            "${1:Int32} ${2:Value};",
            "Declare a field with an explicit deterministic type.",
            "4_01"));
    }

    private static void AddAssets(CygDocument document, List<object> items, bool wrapped)
    {
        foreach (CygSymbol asset in document.Index.Assets.Values
            .OrderBy(symbol => symbol.Name, StringComparer.Ordinal))
        {
            items.Add(new
            {
                label = wrapped ? $"asset_ref<{asset.Name}>" : asset.Name,
                filterText = asset.Name,
                kind = 21,
                detail = "asset schema",
                documentation = LspText.Markup(CygSymbolDocs.Markdown(asset, document.Index)),
                insertText = wrapped ? $"asset_ref<${{1:{asset.Name}}}>" : asset.Name,
                insertTextFormat = wrapped ? 2 : 1,
                sortText = "1_" + asset.Name,
            });
        }
    }

    private static void AddEnumUnderlyings(List<object> items)
    {
        foreach (string type in CygDocs.IntegerUnderlyings)
        {
            items.Add(new
            {
                label = type,
                kind = 14,
                detail = "enum underlying type",
                documentation = LspText.Markup($"Use `{type}` as the wire representation of this enum."),
                insertText = type,
                sortText = "0_" + type,
            });
        }
    }

    private static void AddEnumMember(List<object> items)
    {
        items.Add(Snippet(
            "member",
            "Name = 0,",
            "${1:Member} = ${2:0},",
            "Declare an enum or flags member. Omit the value to auto-assign one.",
            "0_01"));
    }

    private static void AddCapacities(List<object> items)
    {
        foreach (int capacity in new[] { 8, 16, 32, 64, 128, 256 })
        {
            items.Add(new
            {
                label = capacity.ToString(),
                kind = 16,
                detail = "capacity",
                insertText = capacity.ToString(),
                sortText = capacity.ToString("D3"),
            });
        }
    }

    private static void AddElementIds(List<object> items, string? keyword)
    {
        bool isEvent = keyword is "event" or "synced_event";
        int start = isEvent ? 1_000_000 : 1000;
        for (int i = 0; i < 4; i++)
        {
            string value = (start + i).ToString();
            items.Add(new
            {
                label = value,
                kind = 16,
                detail = isEvent ? "event id (1000000..1999999)" : "component id (1000..999999)",
                insertText = value,
                sortText = value,
            });
        }
    }

    private static object Snippet(
        string label,
        string detail,
        string insertText,
        string? documentation,
        string sortPrefix)
        => new
        {
            label,
            kind = 15,
            detail,
            documentation = LspText.Markup(documentation ?? string.Empty),
            insertText,
            insertTextFormat = 2,
            sortText = sortPrefix + "_" + label,
        };

    private static int TypeCompletionKind(CygSymbolKind kind) => kind switch
    {
        CygSymbolKind.Component => 7,
        CygSymbolKind.Enum => 13,
        CygSymbolKind.Flags => 13,
        CygSymbolKind.Union => 8,
        _ => 22,
    };
}
