use axum::Json;
use serde::Serialize;
use uuid::Uuid;

use crate::error::V2Error;
use crate::services::deps::{blobs, q};

/// The most a preview will ever load into a response: an OXCL header that
/// promises more than this is treated as corrupt rather than allocated.
const MAX_PREVIEW_VERTICES: u32 = 2_000_000;
const MAX_PREVIEW_INDICES: u32 = 6_000_000;
const OXCL_HEADER: &[u8; 4] = b"OXCL";
const OXCL_HEADER_LEN: usize = 16;

#[derive(Debug, Serialize)]
pub struct MapPreview {
    pub name: String,
    pub version: i32,
    pub format: String,
    pub sha256: String,
    /// Present for `oxcl`: the cooked collision mesh, ready for WebGL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh: Option<CookedMeshDto>,
    /// Why a format carries no preview yet, so the UI states the fact instead
    /// of showing an empty canvas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CookedMeshDto {
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl super::MapService {
    pub async fn preview_map(
        &self,
        state: &crate::ApiState,
        game_id: Uuid,
        version: i32,
    ) -> Result<Json<MapPreview>, V2Error> {
        let row = q(state)
            .await?
            .game_map_at_version(game_id, version)
            .await?
            .ok_or_else(|| V2Error::not_found(format!("Map version {version} not found.")))?;
        let blob_key = row.blob_key.clone().ok_or_else(|| {
            V2Error::failed_precondition("That map predates the blob store and has no bytes.")
        })?;
        let bytes = blobs(state).await?.get(&blob_key).await?.ok_or_else(|| {
            V2Error::data_loss("The map's bytes are missing from the blob store.")
        })?;

        let (mesh, note) = match row.format.as_str() {
            "oxcl" => match parse_oxcl(&bytes) {
                Ok(mesh) => (Some(mesh), None),
                Err(message) => (None, Some(message)),
            },
            other => (
                None,
                Some(format!(
                    "No preview for {other} maps yet; only OXCL collision meshes render."
                )),
            ),
        };
        Ok(Json(MapPreview {
            name: row.name.clone(),
            version: row.version,
            format: row.format.clone(),
            sha256: row.sha256.clone(),
            mesh,
            note,
        }))
    }
}

/// The OXCL layout `CookedMesh::to_oxcl_bytes` writes: a 16-byte header of
/// magic, version, flags, vertex count and index count, then the flat arrays.
/// Truncated or oversized files answer with a message, never a panic.
pub fn parse_oxcl(bytes: &[u8]) -> Result<CookedMeshDto, String> {
    if bytes.len() < OXCL_HEADER_LEN {
        return Err("The file is too small to be an OXCL mesh.".to_owned());
    }
    if &bytes[0..4] != OXCL_HEADER {
        return Err("The file does not start with the OXCL magic.".to_owned());
    }
    let vertices = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes")) as usize;
    let indices = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes")) as usize;
    if vertices > MAX_PREVIEW_VERTICES as usize || indices > MAX_PREVIEW_INDICES as usize {
        return Err(format!(
            "The mesh claims {vertices} vertices / {indices} indices; the preview caps are \
             {MAX_PREVIEW_VERTICES} / {MAX_PREVIEW_INDICES}."
        ));
    }
    let vertices_bytes = vertices * 12;
    let indices_bytes = indices * 4;
    let expected = OXCL_HEADER_LEN + vertices_bytes + indices_bytes;
    if bytes.len() != expected {
        return Err(format!(
            "The mesh header promises {expected} bytes but the file holds {}.",
            bytes.len()
        ));
    }
    let mut read = OXCL_HEADER_LEN;
    let mut parsed_vertices = Vec::with_capacity(vertices);
    for _ in 0..vertices {
        parsed_vertices.push([
            f32::from_le_bytes(bytes[read..read + 4].try_into().expect("4 bytes")),
            f32::from_le_bytes(bytes[read + 4..read + 8].try_into().expect("4 bytes")),
            f32::from_le_bytes(bytes[read + 8..read + 12].try_into().expect("4 bytes")),
        ]);
        read += 12;
    }
    let mut parsed_indices = Vec::with_capacity(indices);
    for _ in 0..indices {
        parsed_indices.push(u32::from_le_bytes(
            bytes[read..read + 4].try_into().expect("4 bytes"),
        ));
        read += 4;
    }
    Ok(CookedMeshDto {
        vertices: parsed_vertices,
        indices: parsed_indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oxcl_bytes(vertices: &[[f32; 3]], indices: &[u32]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(OXCL_HEADER);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(vertices.len() as u32).to_le_bytes());
        out.extend_from_slice(&(indices.len() as u32).to_le_bytes());
        for vertex in vertices {
            for scalar in vertex {
                out.extend_from_slice(&scalar.to_le_bytes());
            }
        }
        for index in indices {
            out.extend_from_slice(&index.to_le_bytes());
        }
        out
    }

    #[test]
    fn a_valid_oxcl_mesh_parses_round_trip() {
        let mesh = oxcl_bytes(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1, 2],
        );
        let parsed = parse_oxcl(&mesh).expect("valid mesh");
        assert_eq!(parsed.vertices.len(), 3);
        assert_eq!(parsed.indices, vec![0, 1, 2]);
        assert_eq!(parsed.vertices[1], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn truncated_and_wrong_magic_files_fail_with_a_message() {
        let message = parse_oxcl(&[0, 1, 2]).expect_err("too small");
        assert!(message.contains("too small"), "{message}");
        let message = parse_oxcl(&vec![0u8; 64]).expect_err("wrong magic");
        assert!(message.contains("magic"), "{message}");
        let good = oxcl_bytes(&[[0.0; 3]; 2], &[0, 1, 0, 1]);
        let message = parse_oxcl(&good[..good.len() - 3]).expect_err("truncated tail");
        assert!(message.contains("promises"), "{message}");
    }

    #[test]
    fn a_lying_header_never_allocates() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(OXCL_HEADER);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&u32::from(MAX_PREVIEW_VERTICES + 1).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let message = parse_oxcl(&bytes).expect_err("over-cap vertex count");
        assert!(message.contains("caps"), "{message}");
    }
}
