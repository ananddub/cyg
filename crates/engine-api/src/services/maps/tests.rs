
#[cfg(test)]
mod tests {
    use super::super::*;

    fn good_2d() -> Vec<u8> {
        br#"{"arena_width": 800, "arena_height": 600, "obstacles": [{"x":0,"y":0,"w":10,"h":10}]}"#.to_vec()
    }

    #[test]
    fn map2d_gate_accepts_a_well_formed_arena() {
        assert!(MapService::ensure_valid_format(MapFormat::Map2d, &good_2d()).is_ok());
    }

    #[test]
    fn arena_json_without_its_dimension_field_is_refused() {
        let bad = br#"{"obstacles": []}"#.to_vec();
        let err = MapService::ensure_valid_format(MapFormat::Map2d, &bad).unwrap_err();
        assert!(err.message.contains("arena_width"), "{}", err.message);
    }

    #[test]
    fn map3d_gate_expects_arena_depth_not_height() {
        let depth = br#"{"arena_width": 100, "arena_depth": 200}"#.to_vec();
        let height = br#"{"arena_width": 100, "arena_height": 200}"#.to_vec();
        assert!(MapService::ensure_valid_format(MapFormat::Map3d, &depth).is_ok());
        let err = MapService::ensure_valid_format(MapFormat::Map3d, &height).unwrap_err();
        assert!(err.message.contains("arena_depth"), "{}", err.message);
    }

    #[test]
    fn non_json_arena_bytes_are_refused_with_the_parse_reason() {
        let err = MapService::ensure_valid_format(MapFormat::Map2d, b"not json").unwrap_err();
        assert!(err.message.contains("not valid JSON"), "{}", err.message);
    }

    #[test]
    fn oxcl_gate_requires_the_cooked_magic() {
        let cooked = {
            let mut bytes = b"OXCL".to_vec();
            bytes.extend_from_slice(&[0u8; 32]);
            bytes
        };
        assert!(MapService::ensure_valid_format(MapFormat::Oxcl, &cooked).is_ok());
        let err = MapService::ensure_valid_format(MapFormat::Oxcl, b"glTF raw scene bytes").unwrap_err();
        assert!(err.message.contains("OXCL"), "{}", err.message);
    }

    #[test]
    fn empty_and_oversized_uploads_are_refused() {
        assert!(MapService::ensure_valid_format(MapFormat::Navmesh, &[]).is_err());
        let oversized = vec![0u8; MAX_MAP_BYTES + 1];
        assert!(MapService::ensure_valid_format(MapFormat::Navmesh, &oversized).is_err());
        assert!(MapService::ensure_valid_format(MapFormat::Navmesh, &[0u8; 8]).is_ok());
    }

    #[test]
    fn boot_maps_last_active_version_of_a_format_wins() {
        let mut maps = BootMaps::default();
        assert!(maps.is_empty());
        maps.insert(MapAsset {
            format: MapFormat::Map2d,
            name: "a".into(),
            version: 1,
            bytes: good_2d(),
        });
        maps.insert(MapAsset {
            format: MapFormat::Map2d,
            name: "b".into(),
            version: 2,
            bytes: good_2d(),
        });
        assert!(!maps.is_empty());
        assert_eq!(maps.map2d.as_ref().unwrap().name, "b");
        assert!(maps.oxcl.is_none());
    }

    #[test]
    fn format_parse_is_the_str_table_the_upload_and_host_share() {
        assert_eq!(MapFormat::parse("oxcl"), Some(MapFormat::Oxcl));
        assert_eq!(MapFormat::parse("map2d"), Some(MapFormat::Map2d));
        assert_eq!(MapFormat::parse("map3d"), Some(MapFormat::Map3d));
        assert_eq!(MapFormat::parse("navmesh"), Some(MapFormat::Navmesh));
        assert_eq!(MapFormat::parse("scene.gltf"), None);
        for format in [
            MapFormat::Oxcl,
            MapFormat::Map2d,
            MapFormat::Map3d,
            MapFormat::Navmesh,
        ] {
            assert_eq!(MapFormat::parse(format.as_str()), Some(format));
        }
    }
}
