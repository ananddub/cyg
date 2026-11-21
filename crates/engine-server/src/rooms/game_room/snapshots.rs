use crate::rooms::*;

impl GameRoom {
    pub fn export_snapshot(&mut self) -> Option<Vec<u8>> {
        let bytes = self.export_snapshot_bytes()?;
        self.snapshot_seq += 1;
        Some(bytes)
    }

    fn export_snapshot_bytes(&mut self) -> Option<Vec<u8>> {
        if !self.storage_enabled {
            return None;
        }
        match self.backend {
            RoomBackend::Relay => None,
            RoomBackend::Wasm => {
                if let Some(ref mut wasm) = self.wasm {
                    if let Ok(packed) = wasm.on_save() {
                        let ptr = ((packed >> 32) as u32) as usize;
                        let len = (packed as u32) as usize;
                        // The length comes from the guest: bound it by the wasm
                        // memory size before sizing the buffer.
                        let mem_size = wasm
                            .raw_instance()
                            .raw_memory_slice()
                            .map(|m| m.len())
                            .unwrap_or(0);
                        if len > 0 && ptr + len <= mem_size {
                            let mut buf = vec![0u8; len];
                            if wasm
                                .read_memory_into(ptr as u32, len as u32, &mut buf)
                                .is_ok()
                            {
                                let snap = RoomSnapshotData::Wasm(buf);
                                return serde_json::to_vec(&snap).ok();
                            }
                        }
                    }
                }
                None
            }
            RoomBackend::Ecs => {
                if let Some(ref mut ecs) = self.ecs {
                    let snap = RoomSnapshotData::Wasm(ecs.export_tlv());
                    serde_json::to_vec(&snap).ok()
                } else {
                    None
                }
            }
        }
    }

    pub fn import_snapshot(&mut self, bytes: &[u8]) -> Result<(), String> {
        if !self.storage_enabled {
            return Err("storage permission denied".into());
        }
        let snap: RoomSnapshotData = serde_json::from_slice(bytes)
            .map_err(|e| format!("Failed to parse room snapshot: {e}"))?;

        match snap {
            RoomSnapshotData::Wasm(wasm_bytes) => {
                if let Some(ref mut wasm) = self.wasm {
                    let len = wasm_bytes.len() as i32;
                    let ptr = wasm
                        .on_alloc(len)
                        .map_err(|e| format!("on_alloc failed: {e}"))?;
                    wasm.write_memory(ptr as u32, &wasm_bytes)
                        .map_err(|e| format!("write_memory failed: {e}"))?;
                    wasm.on_load(ptr, len)
                        .map_err(|e| format!("on_load failed: {e}"))?;
                    Ok(())
                } else {
                    Err("Room has no WASM instance".into())
                }
            }
        }
    }
}
