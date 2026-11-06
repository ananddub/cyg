use crate::host::*;

impl<'a> HotReload<'a> {
    pub(crate) fn detect_rebuild(&mut self, server: &mut GameServer, cfg: &GameConfig) {
        let mut recompilation_triggered = false;
        if let Ok(src_current) = std::fs::metadata(&self.source_path).and_then(|m| m.modified()) {
            if self.loaded_src_time != Some(src_current) {
                tracing::info!(
                    "[server] 📝 Detected change in {} — recompiling room logic...",
                    cfg.source.display()
                );
                match crate::compile::build(self.project_dir, cfg) {
                    Ok(_) => {
                        self.loaded_src_time = Some(src_current);
                        recompilation_triggered = true;
                    }
                    Err(e) => {
                        tracing::error!(
                            "[server] ⚠️ Compilation error in {}: {e}",
                            cfg.source.display()
                        );
                        self.loaded_src_time = Some(src_current);
                    }
                }
            }
        }

        if !self.console_active && self.wasm_available {
            if let Ok(current) = std::fs::metadata(self.wasm_path).and_then(|m| m.modified()) {
                if self.loaded_wasm_time != Some(current) || recompilation_triggered {
                    let server_perms = server.permissions;
                    let result =
                        self.runtime.load_or_compile_cached(self.wasm_path).and_then(|module| {
                            let linker =
                                crate::rooms::build_host_linker(&*self.runtime, server_perms);
                            self.runtime.prepare(&linker, &module)
                        });
                    match result {
                        Ok(pre) => {
                            self.loaded_wasm_time = Some(current);
                            tracing::info!("[server] 🔥 DEV HOT-RELOAD: room logic updated!");
                            server.hot_reload(pre);
                        }
                        Err(e) => {
                            tracing::error!("[server] ⚠️ hot-reload compilation failed: {e}");
                        }
                    }
                }
            }
        }
    }
}
