package dev.cookyourgame.cyg;

import com.intellij.extapi.psi.PsiFileBase;
import com.intellij.openapi.fileTypes.FileType;
import com.intellij.psi.FileViewProvider;
import org.jetbrains.annotations.NotNull;

public final class CygPsiFile extends PsiFileBase {

    public CygPsiFile(@NotNull FileViewProvider viewProvider) {
        super(viewProvider, CygLanguage.INSTANCE);
    }

    @Override
    public @NotNull FileType getFileType() {
        return CygFileType.INSTANCE;
    }

    @Override
    public String toString() {
        return "CYG File";
    }
}
