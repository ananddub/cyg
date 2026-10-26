using CookYourGame.Dsl;
using CookYourGame.Dsl.Core;

namespace CookYourGame.Lsp;

internal sealed class CygDocument
{
    private CygDocument(string uri, string path, string text, int version)
    {
        Uri = uri;
        Path = path;
        Text = text;
        Version = version;
        Lines = LspText.SplitLines(text);
    }

    public string Uri { get; }
    public string Path { get; }
    public string Text { get; }
    public int Version { get; }
    public string[] Lines { get; }
    public CygModule Module { get; private set; } = new();
    public CygDocumentIndex Index { get; private set; } = null!;
    public string? ParseFailure { get; private set; }

    public static CygDocument Parse(string uri, string text, int version)
    {
        var document = new CygDocument(uri, Uris.ToPath(uri), text, version);
        document.Index = CygDocumentIndex.Build(text, document.Path);
        try
        {
            document.Module = new CygFrontend().Parse(text, document.Path);
        }
        catch (Exception ex)
        {
            document.Module = new CygModule { SourcePath = document.Path };
            document.ParseFailure = $"CYG parser failed: {ex.GetType().Name}: {ex.Message}";
        }

        return document;
    }

    public List<object> Diagnostics()
    {
        var diagnostics = new List<object>();
        foreach (CygDiagnostic diagnostic in Module.Diagnostics)
        {
            CygRange range = LspText.Clamp(LspText.FromSpan(diagnostic.Span), Lines);
            diagnostics.Add(new
            {
                range = range.ToLsp(),
                severity = Severity(diagnostic.Severity),
                code = diagnostic.Id,
                source = "cyg",
                message = diagnostic.Message,
            });
        }

        if (ParseFailure != null)
        {
            diagnostics.Add(new
            {
                range = new CygRange(0, 0, 0, Math.Min(1, Lines.Length > 0 ? Lines[0].Length : 0)).ToLsp(),
                severity = 1,
                code = "CYG999",
                source = "cyg",
                message = ParseFailure,
            });
        }

        return diagnostics;
    }

    private static int Severity(CygSeverity severity) => severity switch
    {
        CygSeverity.Error => 1,
        CygSeverity.Warning => 2,
        _ => 3,
    };
}

internal static class Uris
{
    public static string ToPath(string uri)
    {
        if (Uri.TryCreate(uri, UriKind.Absolute, out Uri? parsed) && parsed.IsFile)
        {
            return parsed.LocalPath;
        }

        return uri;
    }

    public static string FromPath(string path)
        => new Uri(Path.GetFullPath(path)).AbsoluteUri;
}
