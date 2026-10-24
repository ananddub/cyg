package dev.cookyourgame.cyg;

import com.intellij.execution.ExecutionException;
import com.intellij.execution.configurations.GeneralCommandLine;
import com.intellij.execution.configurations.PathEnvironmentVariableUtil;
import com.intellij.ide.plugins.IdeaPluginDescriptor;
import com.intellij.ide.plugins.PluginManagerCore;
import com.intellij.openapi.extensions.PluginId;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.vfs.VirtualFile;
import com.intellij.platform.lsp.api.ProjectWideLspServerDescriptor;
import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Nullable;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

public final class CygLspServerDescriptor extends ProjectWideLspServerDescriptor {

    private static final String PLUGIN_ID = "dev.cookyourgame.cyg";
    private static final String SERVER_DLL = "CYG.Lsp.dll";

    public CygLspServerDescriptor(@NotNull Project project) {
        super(project, "CYG Language Server");
    }

    @Override
    public boolean isSupportedFile(@NotNull VirtualFile file) {
        return file.getFileType() == CygFileType.INSTANCE;
    }

    @Override
    public @NotNull GeneralCommandLine createCommandLine() throws ExecutionException {
        GeneralCommandLine command = resolveCommand();
        if (command != null) {
            return command;
        }
        throw new ExecutionException(
                "CYG language server was not found. Install the .NET 10 runtime and put `dotnet` on PATH, "
                        + "or install the `cyg-lsp` launcher (sdk/unity/Tools~/CYG.Zed/install.sh).");
    }

    private @Nullable GeneralCommandLine resolveCommand() {
        File dotnet = PathEnvironmentVariableUtil.findExecutableInPathOnAnyOS("dotnet");

        Path bundled = bundledServer();
        if (dotnet != null && bundled != null) {
            return command(dotnet.getAbsolutePath(), bundled);
        }

        File launcher = PathEnvironmentVariableUtil.findExecutableInPathOnAnyOS("cyg-lsp");
        if (launcher != null) {
            return command(launcher.getAbsolutePath(), null);
        }

        Path local = localServer();
        if (dotnet != null && local != null) {
            return command(dotnet.getAbsolutePath(), local);
        }

        return null;
    }

    private @Nullable Path bundledServer() {
        IdeaPluginDescriptor descriptor = PluginManagerCore.getPlugin(PluginId.getId(PLUGIN_ID));
        if (descriptor == null) {
            return null;
        }
        Path dll = descriptor.getPluginPath().resolve("server").resolve(SERVER_DLL);
        return Files.isRegularFile(dll) ? dll : null;
    }

    private @Nullable Path localServer() {
        String basePath = getProject().getBasePath();
        if (basePath != null) {
            for (String relative : new String[]{
                    "sdk/unity/Tools~/CYG.Lsp/bin/Release/net10.0/" + SERVER_DLL,
                    "sdk/unity/Tools~/CYG.Lsp/bin/Debug/net10.0/" + SERVER_DLL}) {
                Path dll = Path.of(basePath).resolve(relative);
                if (Files.isRegularFile(dll)) {
                    return dll;
                }
            }
        }

        Path installed = Path.of(System.getProperty("user.home"), ".local", "share", "cyg-lsp", SERVER_DLL);
        return Files.isRegularFile(installed) ? installed : null;
    }

    private @NotNull GeneralCommandLine command(@NotNull String executable, @Nullable Path serverDll) {
        GeneralCommandLine command = serverDll == null
                ? new GeneralCommandLine(executable)
                : new GeneralCommandLine(executable, serverDll.toString());
        command.withCharset(StandardCharsets.UTF_8);

        String basePath = getProject().getBasePath();
        if (basePath != null && !basePath.isBlank()) {
            command.withWorkingDirectory(Path.of(basePath));
        }
        return command;
    }
}
