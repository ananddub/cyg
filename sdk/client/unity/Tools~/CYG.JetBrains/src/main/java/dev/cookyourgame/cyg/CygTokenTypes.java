package dev.cookyourgame.cyg;

import com.intellij.psi.tree.IElementType;

public final class CygTokenTypes {

    public static final IElementType IDENTIFIER = new CygTokenType("IDENTIFIER");
    public static final IElementType KEYWORD = new CygTokenType("KEYWORD");
    public static final IElementType TYPE = new CygTokenType("TYPE");
    public static final IElementType NUMBER = new CygTokenType("NUMBER");

    public static final IElementType LBRACE = new CygTokenType("LBRACE");
    public static final IElementType RBRACE = new CygTokenType("RBRACE");
    public static final IElementType LPAREN = new CygTokenType("LPAREN");
    public static final IElementType RPAREN = new CygTokenType("RPAREN");
    public static final IElementType LBRACKET = new CygTokenType("LBRACKET");
    public static final IElementType RBRACKET = new CygTokenType("RBRACKET");
    public static final IElementType LANGLE = new CygTokenType("LANGLE");
    public static final IElementType RANGLE = new CygTokenType("RANGLE");

    public static final IElementType COMMA = new CygTokenType("COMMA");
    public static final IElementType COLON = new CygTokenType("COLON");
    public static final IElementType SEMICOLON = new CygTokenType("SEMICOLON");
    public static final IElementType EQUALS = new CygTokenType("EQUALS");
    public static final IElementType DOT = new CygTokenType("DOT");
    public static final IElementType MINUS = new CygTokenType("MINUS");

    public static final IElementType LINE_COMMENT = new CygTokenType("LINE_COMMENT");
    public static final IElementType BLOCK_COMMENT = new CygTokenType("BLOCK_COMMENT");

    private CygTokenTypes() {
    }
}
