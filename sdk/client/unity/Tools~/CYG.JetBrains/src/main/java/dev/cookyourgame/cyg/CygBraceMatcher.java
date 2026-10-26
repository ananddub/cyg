package dev.cookyourgame.cyg;

import com.intellij.lang.BracePair;
import com.intellij.lang.PairedBraceMatcher;
import com.intellij.psi.PsiFile;
import com.intellij.psi.tree.IElementType;
import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Nullable;

public final class CygBraceMatcher implements PairedBraceMatcher {

    private static final BracePair[] PAIRS = {
            new BracePair(CygTokenTypes.LBRACE, CygTokenTypes.RBRACE, true),
            new BracePair(CygTokenTypes.LPAREN, CygTokenTypes.RPAREN, false),
            new BracePair(CygTokenTypes.LBRACKET, CygTokenTypes.RBRACKET, false),
            new BracePair(CygTokenTypes.LANGLE, CygTokenTypes.RANGLE, false),
    };

    @Override
    public BracePair @NotNull [] getPairs() {
        return PAIRS;
    }

    @Override
    public boolean isPairedBracesAllowedBeforeType(@NotNull IElementType lbraceType, @Nullable IElementType contextType) {
        return true;
    }

    @Override
    public int getCodeConstructStart(PsiFile file, int openingBraceOffset) {
        return openingBraceOffset;
    }
}
