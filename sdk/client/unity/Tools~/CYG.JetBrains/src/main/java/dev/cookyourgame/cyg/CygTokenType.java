package dev.cookyourgame.cyg;

import com.intellij.psi.tree.IElementType;
import org.jetbrains.annotations.NotNull;

final class CygTokenType extends IElementType {

    CygTokenType(@NotNull String debugName) {
        super(debugName, CygLanguage.INSTANCE);
    }

    @Override
    public String toString() {
        return "CygTokenType." + super.toString();
    }
}
