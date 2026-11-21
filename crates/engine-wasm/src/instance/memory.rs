use super::WasmInstance;
use crate::error::WasmError;

impl<T: 'static> WasmInstance<T> {
    pub fn read_memory(&self, offset: u32, len: u32) -> Result<Vec<u8>, WasmError> {
        let memory = self.memory.ok_or(WasmError::MissingMemory)?;
        let data = memory.data(&self.store);
        let start = offset as usize;
        let end = start.saturating_add(len as usize);

        if end > data.len() {
            return Err(WasmError::OutOfBounds {
                offset: start,
                len: len as usize,
                size: data.len(),
            });
        }

        Ok(data[start..end].to_vec())
    }

    pub fn read_memory_into(
        &self,
        offset: u32,
        len: u32,
        out: &mut Vec<u8>,
    ) -> Result<(), WasmError> {
        let memory = self.memory.ok_or(WasmError::MissingMemory)?;
        let data = memory.data(&self.store);
        let start = offset as usize;
        let end = start.saturating_add(len as usize);

        if end > data.len() {
            return Err(WasmError::OutOfBounds {
                offset: start,
                len: len as usize,
                size: data.len(),
            });
        }

        out.clear();
        out.extend_from_slice(&data[start..end]);
        Ok(())
    }

    pub fn write_memory(&mut self, offset: u32, bytes: &[u8]) -> Result<(), WasmError> {
        let memory = self.memory.ok_or(WasmError::MissingMemory)?;
        let data = memory.data_mut(&mut self.store);
        let start = offset as usize;
        let end = start.saturating_add(bytes.len());

        if end > data.len() {
            return Err(WasmError::OutOfBounds {
                offset: start,
                len: bytes.len(),
                size: data.len(),
            });
        }

        data[start..end].copy_from_slice(bytes);
        Ok(())
    }

    pub fn raw_memory_slice(&self) -> Result<&[u8], WasmError> {
        let memory = self.memory.ok_or(WasmError::MissingMemory)?;
        Ok(memory.data(&self.store))
    }

    pub fn raw_memory_mut(&mut self) -> Result<&mut [u8], WasmError> {
        let memory = self.memory.ok_or(WasmError::MissingMemory)?;
        Ok(memory.data_mut(&mut self.store))
    }

    pub fn snapshot_memory(&self) -> Result<Vec<u8>, WasmError> {
        self.raw_memory_slice().map(|s| s.to_vec())
    }

    pub fn restore_memory(&mut self, snapshot: &[u8]) -> Result<(), WasmError> {
        let memory = self.memory.ok_or(WasmError::MissingMemory)?;
        let data = memory.data_mut(&mut self.store);
        // A prefix restore would splice a snapshot taken from a different
        // memory layout onto a shifted heap; require an exact-size match.
        if snapshot.len() != data.len() {
            return Err(WasmError::OutOfBounds {
                offset: 0,
                len: snapshot.len(),
                size: data.len(),
            });
        }
        data.copy_from_slice(snapshot);
        Ok(())
    }
}
