use std::io;

pub const MAGIC: &[u8; 4] = b"OXCL";

pub const VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct CookedMesh {
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl CookedMesh {
    pub fn to_oxcl_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16 + self.vertices.len() * 12 + self.indices.len() * 4);
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&VERSION.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&(self.vertices.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(self.indices.len() as u32).to_le_bytes());

        for v in &self.vertices {
            buf.extend_from_slice(&v[0].to_le_bytes());
            buf.extend_from_slice(&v[1].to_le_bytes());
            buf.extend_from_slice(&v[2].to_le_bytes());
        }

        for idx in &self.indices {
            buf.extend_from_slice(&idx.to_le_bytes());
        }

        buf
    }

    pub fn from_oxcl_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 16 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "File too small for OXCL header",
            ));
        }

        if &bytes[0..4] != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid magic header, expected OXCL",
            ));
        }

        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsupported OXCL version: {}", version),
            ));
        }

        let vert_count = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        let idx_count = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;

        let expected_len = 16 + vert_count * 12 + idx_count * 4;
        if bytes.len() < expected_len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "OXCL payload truncated",
            ));
        }

        let mut offset = 16;
        let mut vertices = Vec::with_capacity(vert_count);
        for _ in 0..vert_count {
            let x = f32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            let y = f32::from_le_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]);
            let z = f32::from_le_bytes([
                bytes[offset + 8],
                bytes[offset + 9],
                bytes[offset + 10],
                bytes[offset + 11],
            ]);
            offset += 12;
            vertices.push([x, y, z]);
        }

        let mut indices = Vec::with_capacity(idx_count);
        for _ in 0..idx_count {
            let idx = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            offset += 4;
            indices.push(idx);
        }

        Ok(Self { vertices, indices })
    }
}
