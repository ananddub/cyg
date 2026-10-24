package dev.cookyourgame.cyg;

import com.intellij.openapi.fileTypes.LanguageFileType;
import org.jetbrains.annotations.NotNull;

import javax.swing.Icon;

public final class CygFileType extends LanguageFileType {

    public static final CygFileType INSTANCE = new CygFileType();

    private CygFileType() {
        super(CygLanguage.INSTANCE);
    }

    @Override
    public @NotNull String getName() {
        return "CYG";
    }

    @Override
    public @NotNull String getDescription() {
        return "CYG deterministic simulation schema";
    }

    @Override
    public @NotNull String getDefaultExtension() {
        return "cyg";
    }

    @Override
    public @NotNull Icon getIcon() {
        return CygIcons.FILE;
    }
}
