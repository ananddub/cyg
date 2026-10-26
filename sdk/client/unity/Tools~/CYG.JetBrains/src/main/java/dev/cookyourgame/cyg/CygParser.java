package dev.cookyourgame.cyg;

import com.intellij.lang.ASTNode;
import com.intellij.lang.PsiBuilder;
import com.intellij.lang.PsiParser;
import com.intellij.psi.tree.IElementType;
import org.jetbrains.annotations.NotNull;

 /**
 * Flat parser: all language intelligence comes from the CYG language server,
 * the platform only needs a PSI tree with lexer tokens.
 */
public final class CygParser implements PsiParser {

    @Override
    public @NotNull ASTNode parse(@NotNull IElementType root, @NotNull PsiBuilder builder) {
        PsiBuilder.Marker marker = builder.mark();
        while (!builder.eof()) {
            builder.advanceLexer();
        }
        marker.done(root);
        return builder.getTreeBuilt();
    }
}
