package dev.cookyourgame.cyg;

import com.intellij.lexer.LexerBase;
import com.intellij.psi.TokenType;
import com.intellij.psi.tree.IElementType;
import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Nullable;

import java.util.Set;

public final class CygLexer extends LexerBase {

    private static final Set<String> KEYWORDS = Set.of(
            "namespace", "asset", "struct", "component", "enum", "flags", "union",
            "input", "global", "event", "synced_event", "signal", "button");

    private static final Set<String> TYPES = Set.of(
            "Byte", "SByte", "Int16", "UInt16", "Int32", "UInt32", "Int64", "UInt64",
            "Boolean", "FP", "FPVector2", "FPVector3", "FPQuaternion", "FPMatrix4x4",
            "FPAABB", "FPBounds", "entity_ref", "player_ref", "QString", "asset_ref",
            "array", "list", "dictionary");

    private CharSequence buffer = "";
    private int bufferEnd;
    private int tokenStart;
    private int tokenEnd;
    private IElementType tokenType;

    @Override
    public void start(@NotNull CharSequence buffer, int startOffset, int endOffset, int initialState) {
        this.buffer = buffer;
        this.bufferEnd = endOffset;
        this.tokenStart = startOffset;
        this.tokenEnd = startOffset;
        this.tokenType = null;
        advance();
    }

    @Override
    public int getState() {
        return 0;
    }

    @Override
    public @Nullable IElementType getTokenType() {
        return tokenType;
    }

    @Override
    public int getTokenStart() {
        return tokenStart;
    }

    @Override
    public int getTokenEnd() {
        return tokenEnd;
    }

    @Override
    public void advance() {
        tokenStart = tokenEnd;
        tokenType = tokenStart >= bufferEnd ? null : locateToken();
    }

    @Override
    public @NotNull CharSequence getBufferSequence() {
        return buffer;
    }

    @Override
    public int getBufferEnd() {
        return bufferEnd;
    }

    private IElementType locateToken() {
        char c = buffer.charAt(tokenStart);

        if (c == ' ' || c == '\t' || c == '\r' || c == '\n') {
            tokenEnd = tokenStart + 1;
            while (tokenEnd < bufferEnd && isWhitespace(buffer.charAt(tokenEnd))) {
                tokenEnd++;
            }
            return TokenType.WHITE_SPACE;
        }

        if (c == '/' && tokenStart + 1 < bufferEnd) {
            char next = buffer.charAt(tokenStart + 1);
            if (next == '/') {
                tokenEnd = tokenStart + 2;
                while (tokenEnd < bufferEnd && buffer.charAt(tokenEnd) != '\n') {
                    tokenEnd++;
                }
                return CygTokenTypes.LINE_COMMENT;
            }
            if (next == '*') {
                tokenEnd = tokenStart + 2;
                while (tokenEnd < bufferEnd) {
                    if (buffer.charAt(tokenEnd) == '*' && tokenEnd + 1 < bufferEnd && buffer.charAt(tokenEnd + 1) == '/') {
                        tokenEnd += 2;
                        return CygTokenTypes.BLOCK_COMMENT;
                    }
                    tokenEnd++;
                }
                return CygTokenTypes.BLOCK_COMMENT;
            }
        }

        if (isIdentifierStart(c)) {
            tokenEnd = tokenStart + 1;
            while (tokenEnd < bufferEnd && isIdentifierPart(buffer.charAt(tokenEnd))) {
                tokenEnd++;
            }
            String text = buffer.subSequence(tokenStart, tokenEnd).toString();
            if (KEYWORDS.contains(text)) {
                return CygTokenTypes.KEYWORD;
            }
            if (TYPES.contains(text)) {
                return CygTokenTypes.TYPE;
            }
            return CygTokenTypes.IDENTIFIER;
        }

        if (c >= '0' && c <= '9') {
            tokenEnd = tokenStart + 1;
            if (c == '0' && tokenEnd < bufferEnd
                    && (buffer.charAt(tokenEnd) == 'x' || buffer.charAt(tokenEnd) == 'X')
                    && tokenEnd + 1 < bufferEnd && isHexDigit(buffer.charAt(tokenEnd + 1))) {
                tokenEnd += 2;
                while (tokenEnd < bufferEnd && isHexDigit(buffer.charAt(tokenEnd))) {
                    tokenEnd++;
                }
            } else {
                while (tokenEnd < bufferEnd && buffer.charAt(tokenEnd) >= '0' && buffer.charAt(tokenEnd) <= '9') {
                    tokenEnd++;
                }
            }
            return CygTokenTypes.NUMBER;
        }

        tokenEnd = tokenStart + 1;
        return switch (c) {
            case '{' -> CygTokenTypes.LBRACE;
            case '}' -> CygTokenTypes.RBRACE;
            case '(' -> CygTokenTypes.LPAREN;
            case ')' -> CygTokenTypes.RPAREN;
            case '[' -> CygTokenTypes.LBRACKET;
            case ']' -> CygTokenTypes.RBRACKET;
            case '<' -> CygTokenTypes.LANGLE;
            case '>' -> CygTokenTypes.RANGLE;
            case ',' -> CygTokenTypes.COMMA;
            case ':' -> CygTokenTypes.COLON;
            case ';' -> CygTokenTypes.SEMICOLON;
            case '=' -> CygTokenTypes.EQUALS;
            case '.' -> CygTokenTypes.DOT;
            case '-' -> CygTokenTypes.MINUS;
            default -> TokenType.BAD_CHARACTER;
        };
    }

    private static boolean isWhitespace(char c) {
        return c == ' ' || c == '\t' || c == '\r' || c == '\n';
    }

    private static boolean isIdentifierStart(char c) {
        return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
    }

    private static boolean isIdentifierPart(char c) {
        return isIdentifierStart(c) || (c >= '0' && c <= '9');
    }

    private static boolean isHexDigit(char c) {
        return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');
    }
}
