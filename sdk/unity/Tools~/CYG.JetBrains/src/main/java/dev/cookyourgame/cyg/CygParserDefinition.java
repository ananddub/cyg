package dev.cookyourgame.cyg;

import com.intellij.extapi.psi.ASTWrapperPsiElement;
import com.intellij.lang.ASTNode;
import com.intellij.lang.ParserDefinition;
import com.intellij.lang.PsiParser;
import com.intellij.lexer.Lexer;
import com.intellij.openapi.project.Project;
import com.intellij.psi.FileViewProvider;
import com.intellij.psi.PsiElement;
import com.intellij.psi.PsiFile;
import com.intellij.psi.tree.IFileElementType;
import com.intellij.psi.tree.TokenSet;
import org.jetbrains.annotations.NotNull;

public final class CygParserDefinition implements ParserDefinition {

    public static final IFileElementType FILE = new IFileElementType(CygLanguage.INSTANCE);

    private static final TokenSet COMMENTS = TokenSet.create(
            CygTokenTypes.LINE_COMMENT, CygTokenTypes.BLOCK_COMMENT);

    @Override
    public @NotNull Lexer createLexer(Project project) {
        return new CygLexer();
    }

    @Override
    public @NotNull PsiParser createParser(Project project) {
        return new CygParser();
    }

    @Override
    public @NotNull IFileElementType getFileNodeType() {
        return FILE;
    }

    @Override
    public @NotNull TokenSet getCommentTokens() {
        return COMMENTS;
    }

    @Override
    public @NotNull TokenSet getStringLiteralElements() {
        return TokenSet.EMPTY;
    }

    @Override
    public @NotNull PsiElement createElement(@NotNull ASTNode node) {
        return new ASTWrapperPsiElement(node);
    }

    @Override
    public @NotNull PsiFile createFile(@NotNull FileViewProvider viewProvider) {
        return new CygPsiFile(viewProvider);
    }
}
