//! The map assets a hosted game carries into its server process.
//!
//! The console stores a game's maps in `game_maps`; the control plane fetches
//! the active versions once at boot and hands them to the host loop, where they
//! take priority over the project folder's local files (`arena.oxcl`,
//! `arena_2d.json`, `arena_map.json`, `navmesh.nav2d`). A local-only project
//! (no database) runs with [`BootMaps::default`] and never notices the
//! difference.

/// One active map's bytes, as stored by the console upload.
#[derive(Clone, Debug)]
pub struct MapAsset {
    /// `game_maps.format`: `oxcl`, `map2d`, `map3d` or `navmesh`.
    pub format: MapFormat,
    /// The stored name — a log line's best friend.
    pub name: String,
    pub version: i32,
    pub bytes: Vec<u8>,
}

/// The four map kinds the host knows how to parse. Kept as an enum (not free
/// strings) so a new format must be wired through the host's loader too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapFormat {
    /// Cooked 3D collision mesh (`ox-game cook <map.glb>`, OXCL magic bytes).
    Oxcl,
    /// `arena_2d.json` — 2D boxes/circles static arena.
    Map2d,
    /// `arena_map.json` — 3D boxes/walls static arena.
    Map3d,
    /// Baked 2D navmesh for bot pathfinding.
    Navmesh,
}

impl MapFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            MapFormat::Oxcl => "oxcl",
            MapFormat::Map2d => "map2d",
            MapFormat::Map3d => "map3d",
            MapFormat::Navmesh => "navmesh",
        }
    }

    /// The `game_maps.format` value the console stored, or `None` for a name
    /// this engine does not parse (the upload is refused before the database
    /// ever sees it, so this only guards against hand-edited rows).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "oxcl" => Some(MapFormat::Oxcl),
            "map2d" => Some(MapFormat::Map2d),
            "map3d" => Some(MapFormat::Map3d),
            "navmesh" => Some(MapFormat::Navmesh),
            _ => None,
        }
    }
}

/// The active maps of the hosted game, keyed by format. One slot each — the
/// console keeps at most one active version per format per game.
#[derive(Clone, Debug, Default)]
pub struct BootMaps {
    pub oxcl: Option<MapAsset>,
    pub map2d: Option<MapAsset>,
    pub map3d: Option<MapAsset>,
    pub navmesh: Option<MapAsset>,
}

impl BootMaps {
    /// Insert by format; the last active version of a format wins (the console
    /// allows only one, but a hand-edited row must not panic a boot).
    pub fn insert(&mut self, asset: MapAsset) {
        match asset.format {
            MapFormat::Oxcl => self.oxcl = Some(asset),
            MapFormat::Map2d => self.map2d = Some(asset),
            MapFormat::Map3d => self.map3d = Some(asset),
            MapFormat::Navmesh => self.navmesh = Some(asset),
        }
    }

    /// `true` when no map came from the console — the common local-project
    /// case, and the signal for the host to read its local files in peace.
    pub fn is_empty(&self) -> bool {
        self.oxcl.is_none() && self.map2d.is_none() && self.map3d.is_none() && self.navmesh.is_none()
    }
}
