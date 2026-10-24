package dev.cookyourgame.cyg;

import com.intellij.lang.Language;

public final class CygLanguage extends Language {

    public static final CygLanguage INSTANCE = new CygLanguage();

    private CygLanguage() {
        super("CYG");
    }

    @Override
    public boolean isCaseSensitive() {
        return true;
    }
}
