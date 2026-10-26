using System.Text;

namespace CookYourGame.Lsp;

internal static class CygSymbolDocs
{
    public static string Signature(CygSymbol symbol)
    {
        if (symbol.Kind == CygSymbolKind.Signal)
        {
            string parameters = string.Join(", ", symbol.Children.Select(
                child => child.TypeDisplay + " " + child.Name));
            return $"signal {symbol.Name}({parameters})";
        }

        return symbol.Signature;
    }

    public static string Detail(CygSymbol symbol) => symbol.Kind switch
    {
        CygSymbolKind.Namespace => "namespace " + symbol.Name,
        CygSymbolKind.Asset => "asset schema",
        CygSymbolKind.Struct => $"struct · {FieldCount(symbol)} fields",
        CygSymbolKind.Component => symbol.Id.HasValue
            ? $"component · id {symbol.Id.Value} · {FieldCount(symbol)} fields"
            : $"component · {FieldCount(symbol)} fields",
        CygSymbolKind.Enum => $"enum · {symbol.Underlying ?? "Int32"} · {symbol.Children.Count} members",
        CygSymbolKind.Flags => $"flags · {symbol.Underlying ?? "Int32"} · {symbol.Children.Count} members",
        CygSymbolKind.Union => $"union · {symbol.Children.Count} variants",
        CygSymbolKind.UnionVariant => $"union variant · {FieldCount(symbol)} fields",
        CygSymbolKind.Input => $"input schema · {symbol.Children.Count} fields",
        CygSymbolKind.Global => $"global schema · {FieldCount(symbol)} fields",
        CygSymbolKind.Event => symbol.Id.HasValue
            ? $"event · id {symbol.Id.Value} · {FieldCount(symbol)} fields"
            : $"event · {FieldCount(symbol)} fields",
        CygSymbolKind.SyncedEvent => symbol.Id.HasValue
            ? $"synced event · id {symbol.Id.Value} · {FieldCount(symbol)} fields"
            : $"synced event · {FieldCount(symbol)} fields",
        CygSymbolKind.Command => symbol.Id.HasValue
            ? $"command · id {symbol.Id.Value} · {FieldCount(symbol)} fields"
            : $"command · {FieldCount(symbol)} fields",
        CygSymbolKind.Signal => $"signal · {symbol.Children.Count} parameters",
        CygSymbolKind.Field => "field",
        CygSymbolKind.Button => "input button",
        CygSymbolKind.EnumMember => symbol.Value.HasValue
            ? $"member · value {symbol.Value.Value}"
            : "member",
        CygSymbolKind.SignalParameter => "signal parameter",
        _ => string.Empty,
    };

    public static string Markdown(CygSymbol symbol, CygDocumentIndex? index = null)
    {
        var builder = new StringBuilder();
        builder.Append("```cyg\n").Append(Signature(symbol)).Append("\n```\n\n");

        switch (symbol.Kind)
        {
            case CygSymbolKind.Namespace:
                builder.Append("Generated code is emitted into `").Append(symbol.Name)
                    .Append("`. The namespace must be the first declaration in the file.");
                break;

            case CygSymbolKind.Asset:
                int uses = index?.References.Count(
                    reference => reference.IsAsset && reference.Name == symbol.Name) ?? 0;
                builder.Append("Asset schema referenced with `asset_ref<").Append(symbol.Name).Append(">`. ");
                builder.Append(uses == 1 ? "Used by 1 field." : $"Used by {uses} fields.");
                break;

            case CygSymbolKind.Struct:
                builder.Append("Value struct. Every field is part of the deterministic wire layout.");
                AppendFields(builder, symbol);
                break;

            case CygSymbolKind.Component:
                builder.Append("Deterministic ECS component.");
                if (symbol.Id.HasValue)
                {
                    builder.Append(" Component id **").Append(symbol.Id.Value)
                        .Append("** must stay stable across every build and client.");
                }
                else
                {
                    builder.Append(" No explicit id was written; the validator derives one and warns before shipping.");
                }

                AppendFields(builder, symbol);
                break;

            case CygSymbolKind.Enum:
            case CygSymbolKind.Flags:
                builder.Append(symbol.Kind == CygSymbolKind.Flags
                    ? "Bit-flags enum. `None` is 0; other members default to successive powers of two."
                    : "Integer enum. Members without a value auto-increment.");
                builder.Append(" Underlying type: `").Append(symbol.Underlying ?? "Int32").Append("`.");
                AppendTable(builder, "Member", "Value",
                    symbol.Children.Select(child => (child.Name, child.Value?.ToString() ?? "auto")));
                break;

            case CygSymbolKind.Union:
                builder.Append("Tagged union. Exactly one variant payload is active at a time.");
                AppendTable(builder, "Variant", "Fields", symbol.Children.Select(variant =>
                {
                    string fields = string.Join(", ", variant.Children.Select(
                        field => field.TypeDisplay + " " + field.Name));
                    return (variant.Name, fields.Length == 0 ? "—" : fields);
                }));
                break;

            case CygSymbolKind.UnionVariant:
                builder.Append("Variant of union `").Append(symbol.Parent?.Name ?? "?")
                    .Append("`.");
                AppendFields(builder, symbol);
                break;

            case CygSymbolKind.Input:
                builder.Append("Per-player input schema. `button` entries are one bit each; other fields are analog values.");
                AppendFields(builder, symbol);
                break;

            case CygSymbolKind.Global:
                builder.Append("Simulation-global state schema.");
                AppendFields(builder, symbol);
                break;

            case CygSymbolKind.Event:
            case CygSymbolKind.SyncedEvent:
                builder.Append(symbol.Kind == CygSymbolKind.SyncedEvent
                    ? "Synced networked event."
                    : "One-shot networked event.");
                if (symbol.Id.HasValue)
                {
                    builder.Append(" Event id **").Append(symbol.Id.Value)
                        .Append("** must be in 1000000..1999999.");
                }

                AppendFields(builder, symbol);
                break;

            case CygSymbolKind.Command:
                builder.Append("Player command. Commands are external input: they are recorded per tick, ");
                builder.Append("never hashed into the simulation state and never rewound by rollback.");
                if (symbol.Id.HasValue)
                {
                    builder.Append(" Command id **").Append(symbol.Id.Value)
                        .Append("** must be in 2000000..2999999.");
                }

                AppendFields(builder, symbol);
                break;

            case CygSymbolKind.Signal:
                builder.Append("Server-to-client signal. Signals have no id and are not simulation state.");
                AppendTable(builder, "Parameter", "Type",
                    symbol.Children.Select(child => (child.Name, child.TypeDisplay)));
                break;

            case CygSymbolKind.Field:
                builder.Append("Field of `").Append(symbol.Parent?.Name ?? "?")
                    .Append("` with type `").Append(symbol.TypeDisplay).Append("`.");
                break;

            case CygSymbolKind.Button:
                builder.Append("Digital input button (one bit) of `")
                    .Append(symbol.Parent?.Name ?? "input").Append("`.");
                break;

            case CygSymbolKind.EnumMember:
                builder.Append("Member of `").Append(symbol.Parent?.Name ?? "?")
                    .Append('`');
                if (symbol.Value.HasValue)
                {
                    builder.Append(" with value **").Append(symbol.Value.Value).Append("**");
                }

                builder.Append('.');
                break;

            case CygSymbolKind.SignalParameter:
                builder.Append("Parameter of `signal ").Append(symbol.Parent?.Name ?? "?")
                    .Append("` with type `").Append(symbol.TypeDisplay).Append("`.");
                break;
        }

        return builder.ToString();
    }

    private static int FieldCount(CygSymbol symbol)
        => symbol.Children.Count(child => child.Kind is CygSymbolKind.Field or CygSymbolKind.Button);

    private static void AppendFields(StringBuilder builder, CygSymbol symbol)
    {
        AppendTable(builder, "Field", "Type",
            symbol.Children
                .Where(child => child.Kind is CygSymbolKind.Field or CygSymbolKind.Button)
                .Select(child => (child.Name, child.TypeDisplay)));
    }

    private static void AppendTable(
        StringBuilder builder,
        string leftHeader,
        string rightHeader,
        IEnumerable<(string Left, string Right)> rows)
    {
        var list = rows.ToList();
        if (list.Count == 0)
        {
            builder.Append("\n_No entries._");
            return;
        }

        builder.Append("\n\n| ").Append(leftHeader).Append(" | ").Append(rightHeader).Append(" |\n")
            .Append("| --- | --- |\n");
        int shown = Math.Min(list.Count, 24);
        for (int i = 0; i < shown; i++)
        {
            builder.Append("| `").Append(list[i].Left).Append("` | `")
                .Append(list[i].Right).Append("` |\n");
        }

        if (list.Count > shown)
        {
            builder.Append("\n_… ").Append(list.Count - shown).Append(" more._");
        }
    }
}
