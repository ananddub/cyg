using CookYourGame.Dsl;

namespace CookYourGame.Dsl.Core
{
    /// <summary>Lexes, parses and validates a .cyg source file into a CygModule.</summary>
    public sealed class CygFrontend : ICygFrontend
    {
        public CygModule Parse(string source, string filePath)
        {
            var diagnostics = new CygDiagnosticBag();
            var lexer = new CygLexer(source, filePath);
            var tokens = lexer.Tokenize(diagnostics);
            var parser = new CygParser(tokens, filePath, diagnostics);
            CygModule module = parser.Parse();
            CygValidator.Validate(module, diagnostics);
            module.Diagnostics.AddRange(diagnostics.Items);
            return module;
        }
    }
}
