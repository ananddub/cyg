using System;
using System.Collections.Generic;
using CookYourGame.Dsl;

namespace CookYourGame.Dsl.Core
{
    internal enum CygTokenKind
    {
        Identifier,
        Number,
        LBrace,
        RBrace,
        LParen,
        RParen,
        LBracket,
        RBracket,
        LAngle,
        RAngle,
        Comma,
        Colon,
        Semicolon,
        Equals,
        Dot,
        Minus,
        EndOfFile
    }

    internal readonly struct CygToken
    {
        public CygToken(CygTokenKind kind, string text, long number, CygSpan span)
        {
            Kind = kind;
            Text = text;
            Number = number;
            Span = span;
        }

        public CygTokenKind Kind { get; }
        public string Text { get; }
        public long Number { get; }
        public CygSpan Span { get; }

        public bool IsIdentifier(string text)
            => Kind == CygTokenKind.Identifier && string.Equals(Text, text, StringComparison.Ordinal);

        public override string ToString() => Kind == CygTokenKind.EndOfFile ? "<eof>" : Text;
    }

    /// <summary>Hand-written lexer for the .cyg surface language.</summary>
    internal sealed class CygLexer
    {
        private readonly string _source;
        private readonly string _file;
        private readonly List<CygToken> _tokens = new List<CygToken>();
        private int _position;
        private int _line = 1;
        private int _column = 1;

        public CygLexer(string source, string file)
        {
            _source = source ?? string.Empty;
            _file = file ?? string.Empty;
        }

        public List<CygToken> Tokenize(CygDiagnosticBag diagnostics)
        {
            while (true)
            {
                SkipTrivia(diagnostics);
                if (_position >= _source.Length)
                {
                    _tokens.Add(new CygToken(CygTokenKind.EndOfFile, string.Empty, 0, CurrentSpan(0)));
                    return _tokens;
                }

                int start = _position;
                int line = _line;
                int column = _column;
                char c = _source[_position];

                if (IsIdentifierStart(c))
                {
                    _position++;
                    _column++;
                    while (_position < _source.Length && IsIdentifierPart(_source[_position]))
                    {
                        _position++;
                        _column++;
                    }

                    string text = _source.Substring(start, _position - start);
                    _tokens.Add(new CygToken(CygTokenKind.Identifier, text, 0,
                        new CygSpan(_file, line, column, text.Length)));
                    continue;
                }

                if (c >= '0' && c <= '9')
                {
                    LexNumber(diagnostics, start, line, column);
                    continue;
                }

                _position++;
                _column++;
                CygTokenKind kind;
                switch (c)
                {
                    case '{': kind = CygTokenKind.LBrace; break;
                    case '}': kind = CygTokenKind.RBrace; break;
                    case '(': kind = CygTokenKind.LParen; break;
                    case ')': kind = CygTokenKind.RParen; break;
                    case '[': kind = CygTokenKind.LBracket; break;
                    case ']': kind = CygTokenKind.RBracket; break;
                    case '<': kind = CygTokenKind.LAngle; break;
                    case '>': kind = CygTokenKind.RAngle; break;
                    case ',': kind = CygTokenKind.Comma; break;
                    case ':': kind = CygTokenKind.Colon; break;
                    case ';': kind = CygTokenKind.Semicolon; break;
                    case '=': kind = CygTokenKind.Equals; break;
                    case '.': kind = CygTokenKind.Dot; break;
                    case '-': kind = CygTokenKind.Minus; break;
                    default:
                        diagnostics.Error(CygDiagnosticIds.Lexer,
                            $"Unexpected character '{c}'.",
                            new CygSpan(_file, line, column, 1));
                        continue;
                }

                _tokens.Add(new CygToken(kind, c.ToString(), 0, new CygSpan(_file, line, column, 1)));
            }
        }

        private void LexNumber(CygDiagnosticBag diagnostics, int start, int line, int column)
        {
            bool hex = false;
            if (_source[_position] == '0'
                && _position + 1 < _source.Length
                && (_source[_position + 1] == 'x' || _source[_position + 1] == 'X'))
            {
                hex = true;
                _position += 2;
                _column += 2;
                while (_position < _source.Length && IsHexDigit(_source[_position]))
                {
                    _position++;
                    _column++;
                }
            }
            else
            {
                while (_position < _source.Length && _source[_position] >= '0' && _source[_position] <= '9')
                {
                    _position++;
                    _column++;
                }
            }

            string text = _source.Substring(start, _position - start);
            long value = 0;
            bool valid = true;
            if (hex)
            {
                string digits = text.Substring(2);
                if (digits.Length == 0)
                {
                    valid = false;
                }
                else
                {
                    for (int i = 0; i < digits.Length && valid; i++)
                    {
                        int digit = HexValue(digits[i]);
                        if (digit < 0 || value > (long.MaxValue - digit) / 16)
                        {
                            valid = false;
                        }
                        else
                        {
                            value = value * 16 + digit;
                        }
                    }
                }
            }
            else
            {
                for (int i = 0; i < text.Length && valid; i++)
                {
                    int digit = text[i] - '0';
                    if (value > (long.MaxValue - digit) / 10)
                    {
                        valid = false;
                    }
                    else
                    {
                        value = value * 10 + digit;
                    }
                }
            }

            CygSpan span = new CygSpan(_file, line, column, text.Length);
            if (!valid)
            {
                diagnostics.Error(CygDiagnosticIds.Lexer, $"Number '{text}' does not fit in a signed 64-bit integer.", span);
                value = 0;
            }

            _tokens.Add(new CygToken(CygTokenKind.Number, text, value, span));
        }

        private void SkipTrivia(CygDiagnosticBag diagnostics)
        {
            while (_position < _source.Length)
            {
                char c = _source[_position];
                if (c == ' ' || c == '\t' || c == '\r')
                {
                    _position++;
                    _column++;
                    continue;
                }

                if (c == '\n')
                {
                    _position++;
                    _line++;
                    _column = 1;
                    continue;
                }

                if (c == '/' && _position + 1 < _source.Length && _source[_position + 1] == '/')
                {
                    _position += 2;
                    _column += 2;
                    while (_position < _source.Length && _source[_position] != '\n')
                    {
                        _position++;
                        _column++;
                    }
                    continue;
                }

                if (c == '/' && _position + 1 < _source.Length && _source[_position + 1] == '*')
                {
                    int line = _line;
                    int column = _column;
                    _position += 2;
                    _column += 2;
                    bool closed = false;
                    while (_position < _source.Length)
                    {
                        if (_source[_position] == '*' && _position + 1 < _source.Length && _source[_position + 1] == '/')
                        {
                            _position += 2;
                            _column += 2;
                            closed = true;
                            break;
                        }

                        if (_source[_position] == '\n')
                        {
                            _position++;
                            _line++;
                            _column = 1;
                        }
                        else
                        {
                            _position++;
                            _column++;
                        }
                    }

                    if (!closed)
                    {
                        diagnostics.Error(CygDiagnosticIds.Lexer,
                            "Unterminated block comment.",
                            new CygSpan(_file, line, column, 2));
                        return;
                    }

                    continue;
                }

                return;
            }
        }

        private CygSpan CurrentSpan(int length) => new CygSpan(_file, _line, _column, length);

        private static bool IsIdentifierStart(char c)
            => (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';

        private static bool IsIdentifierPart(char c)
            => IsIdentifierStart(c) || (c >= '0' && c <= '9');

        private static bool IsHexDigit(char c)
            => (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');

        private static int HexValue(char c)
        {
            if (c >= '0' && c <= '9') return c - '0';
            if (c >= 'a' && c <= 'f') return c - 'a' + 10;
            if (c >= 'A' && c <= 'F') return c - 'A' + 10;
            return -1;
        }
    }
}
