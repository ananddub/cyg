package dev.cookyourgame.cyg;

import com.intellij.lexer.Lexer;
import com.intellij.openapi.editor.DefaultLanguageHighlighterColors;
import com.intellij.openapi.editor.colors.TextAttributesKey;
import com.intellij.openapi.fileTypes.SyntaxHighlighterBase;
import com.intellij.psi.tree.IElementType;
import org.jetbrains.annotations.NotNull;

import java.util.HashMap;
import java.util.Map;

import static com.intellij.openapi.editor.colors.TextAttributesKey.createTextAttributesKey;

public final class CygSyntaxHighlighter extends SyntaxHighlighterBase {

    public static final TextAttributesKey KEYWORD =
            createTextAttributesKey("CYG_KEYWORD", DefaultLanguageHighlighterColors.KEYWORD);
    public static final TextAttributesKey TYPE =
            createTextAttributesKey("CYG_TYPE", DefaultLanguageHighlighterColors.KEYWORD);
    public static final TextAttributesKey NUMBER =
            createTextAttributesKey("CYG_NUMBER", DefaultLanguageHighlighterColors.NUMBER);
    public static final TextAttributesKey LINE_COMMENT =
            createTextAttributesKey("CYG_LINE_COMMENT", DefaultLanguageHighlighterColors.LINE_COMMENT);
    public static final TextAttributesKey BLOCK_COMMENT =
            createTextAttributesKey("CYG_BLOCK_COMMENT", DefaultLanguageHighlighterColors.BLOCK_COMMENT);
    public static final TextAttributesKey OPERATION_SIGN =
            createTextAttributesKey("CYG_OPERATION_SIGN", DefaultLanguageHighlighterColors.OPERATION_SIGN);
    public static final TextAttributesKey BRACES =
            createTextAttributesKey("CYG_BRACES", DefaultLanguageHighlighterColors.BRACES);
    public static final TextAttributesKey BRACKETS =
            createTextAttributesKey("CYG_BRACKETS", DefaultLanguageHighlighterColors.BRACKETS);
    public static final TextAttributesKey PARENTHESES =
            createTextAttributesKey("CYG_PARENTHESES", DefaultLanguageHighlighterColors.PARENTHESES);
    public static final TextAttributesKey COMMA =
            createTextAttributesKey("CYG_COMMA", DefaultLanguageHighlighterColors.COMMA);
    public static final TextAttributesKey DOT =
            createTextAttributesKey("CYG_DOT", DefaultLanguageHighlighterColors.DOT);
    public static final TextAttributesKey SEMICOLON =
            createTextAttributesKey("CYG_SEMICOLON", DefaultLanguageHighlighterColors.SEMICOLON);

    private static final Map<IElementType, TextAttributesKey> KEYS = new HashMap<>();

    static {
        KEYS.put(CygTokenTypes.KEYWORD, KEYWORD);
        KEYS.put(CygTokenTypes.TYPE, TYPE);
        KEYS.put(CygTokenTypes.NUMBER, NUMBER);
        KEYS.put(CygTokenTypes.LINE_COMMENT, LINE_COMMENT);
        KEYS.put(CygTokenTypes.BLOCK_COMMENT, BLOCK_COMMENT);
        KEYS.put(CygTokenTypes.LBRACE, BRACES);
        KEYS.put(CygTokenTypes.RBRACE, BRACES);
        KEYS.put(CygTokenTypes.LBRACKET, BRACKETS);
        KEYS.put(CygTokenTypes.RBRACKET, BRACKETS);
        KEYS.put(CygTokenTypes.LPAREN, PARENTHESES);
        KEYS.put(CygTokenTypes.RPAREN, PARENTHESES);
        KEYS.put(CygTokenTypes.LANGLE, PARENTHESES);
        KEYS.put(CygTokenTypes.RANGLE, PARENTHESES);
        KEYS.put(CygTokenTypes.COMMA, COMMA);
        KEYS.put(CygTokenTypes.DOT, DOT);
        KEYS.put(CygTokenTypes.SEMICOLON, SEMICOLON);
        KEYS.put(CygTokenTypes.COLON, OPERATION_SIGN);
        KEYS.put(CygTokenTypes.EQUALS, OPERATION_SIGN);
        KEYS.put(CygTokenTypes.MINUS, OPERATION_SIGN);
    }

    @Override
    public @NotNull Lexer getHighlightingLexer() {
        return new CygLexer();
    }

    @Override
    public TextAttributesKey @NotNull [] getTokenHighlights(IElementType tokenType) {
        return pack(KEYS.get(tokenType));
    }
}
