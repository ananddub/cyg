use zed_extension_api::{self as zed, Command, LanguageServerId, Result, Worktree};

const LANGUAGE_SERVER_ID: &str = "cyg-lsp";
const RELEASE_DLL: &str = "sdk/unity/Tools~/CYG.Lsp/bin/Release/net10.0/CYG.Lsp.dll";
const DEBUG_DLL: &str = "sdk/unity/Tools~/CYG.Lsp/bin/Debug/net10.0/CYG.Lsp.dll";

struct CygExtension;

impl zed::Extension for CygExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        if language_server_id.as_ref() != LANGUAGE_SERVER_ID {
            return Err(format!("Unknown CYG language server: {language_server_id}"));
        }

        let env = worktree.shell_env();

        if let Some(command) = worktree.which("cyg-lsp") {
            return Ok(Command {
                command,
                args: Vec::new(),
                env,
            });
        }

        let dotnet = worktree.which("dotnet").ok_or_else(|| {
            "CYG: neither `cyg-lsp` nor `dotnet` was found on PATH. Install the .NET 10 SDK and \
             run sdk/unity/Tools~/CYG.Zed/install.sh, or add a `cyg-lsp` launcher to PATH."
                .to_string()
        })?;

        let root = worktree.root_path();
        for relative in [RELEASE_DLL, DEBUG_DLL] {
            let dll = format!("{root}/{relative}");
            let runtime_config = dll.replace("CYG.Lsp.dll", "CYG.Lsp.runtimeconfig.json");
            if worktree.read_text_file(&runtime_config).is_ok() {
                return Ok(Command {
                    command: dotnet,
                    args: vec![dll],
                    env,
                });
            }
        }

        Err(
            "CYG: the language server is not installed. Build it with `dotnet publish \
             sdk/unity/Tools~/CYG.Lsp/CYG.Lsp.csproj -c Release` or run \
             sdk/unity/Tools~/CYG.Zed/install.sh to install `cyg-lsp` on PATH."
                .to_string(),
        )
    }
}

zed::register_extension!(CygExtension);
