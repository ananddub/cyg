package dev.cookyourgame.cyg;

import com.intellij.openapi.project.Project;
import com.intellij.openapi.vfs.VirtualFile;
import com.intellij.platform.lsp.api.LspServerSupportProvider;
import org.jetbrains.annotations.NotNull;

public final class CygLspServerSupportProvider implements LspServerSupportProvider {

    @Override
    public void fileOpened(@NotNull Project project, @NotNull VirtualFile file, @NotNull LspServerStarter starter) {
        if (file.getFileType() == CygFileType.INSTANCE) {
            starter.ensureServerStarted(new CygLspServerDescriptor(project));
        }
    }
}
