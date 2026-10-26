using CookYourGame.Dsl;

namespace CookYourGame.Lsp;

/// <summary>0-based, end-exclusive source range. LSP's native coordinate space.</summary>
internal readonly record struct CygRange(int StartLine, int StartCharacter, int EndLine, int EndCharacter)
{
    public bool Contains(int line, int character)
        => (line > StartLine || (line == StartLine && character >= StartCharacter))
           && (line < EndLine || (line == EndLine && character < EndCharacter));

    public bool ContainsInclusiveEnd(int line, int character)
        => (line > StartLine || (line == StartLine && character >= StartCharacter))
           && (line < EndLine || (line == EndLine && character <= EndCharacter));

    public int LengthOnLine => EndLine == StartLine ? EndCharacter - StartCharacter : EndCharacter;

    public override string ToString() => $"{StartLine}:{StartCharacter}-{EndLine}:{EndCharacter}";
}

internal static class LspText
{
    public static string[] SplitLines(string text)
    {
        string[] lines = text.Split('\n');
        for (int i = 0; i < lines.Length; i++)
        {
            if (lines[i].EndsWith('\r'))
            {
                lines[i] = lines[i][..^1];
            }
        }

        return lines;
    }

    public static CygRange FromSpan(CygSpan span)
    {
        int line = Math.Max(0, span.Line - 1);
        int character = Math.Max(0, span.Column - 1);
        int length = Math.Max(1, span.Length);
        return new CygRange(line, character, line, character + length);
    }

    public static CygRange Clamp(CygRange range, string[] lines)
    {
        if (lines.Length == 0)
        {
            return new CygRange(0, 0, 0, 0);
        }

        int startLine = Math.Clamp(range.StartLine, 0, lines.Length - 1);
        int startCharacter = Math.Clamp(range.StartCharacter, 0, lines[startLine].Length);
        int endLine = Math.Clamp(range.EndLine, startLine, lines.Length - 1);
        int endCharacter = Math.Clamp(range.EndCharacter, 0, lines[endLine].Length);
        if (endLine == startLine && endCharacter <= startCharacter)
        {
            endCharacter = Math.Min(lines[startLine].Length, startCharacter + 1);
        }

        return new CygRange(startLine, startCharacter, endLine, endCharacter);
    }

    public static object ToLsp(this CygRange range) => new
    {
        start = new { line = range.StartLine, character = range.StartCharacter },
        end = new { line = range.EndLine, character = range.EndCharacter },
    };

    public static object Markup(string markdown) => new { kind = "markdown", value = markdown };

    public static int Compare(CygRange left, CygRange right)
    {
        int line = left.StartLine.CompareTo(right.StartLine);
        return line != 0 ? line : left.StartCharacter.CompareTo(right.StartCharacter);
    }
}
