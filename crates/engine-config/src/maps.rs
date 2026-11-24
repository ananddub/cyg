#[derive(Clone, Debug)]
pub struct MapAsset {
    pub format: MapFormat,

    pub name: String,
    pub version: i32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapFormat {
    Oxcl,

    Map2d,

    Map3d,

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

#[derive(Clone, Debug, Default)]
pub struct BootMaps {
    pub oxcl: Option<MapAsset>,
    pub map2d: Option<MapAsset>,
    pub map3d: Option<MapAsset>,
    pub navmesh: Option<MapAsset>,
}

impl BootMaps {
    pub fn insert(&mut self, asset: MapAsset) {
        match asset.format {
            MapFormat::Oxcl => self.oxcl = Some(asset),
            MapFormat::Map2d => self.map2d = Some(asset),
            MapFormat::Map3d => self.map3d = Some(asset),
            MapFormat::Navmesh => self.navmesh = Some(asset),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.oxcl.is_none()
            && self.map2d.is_none()
            && self.map3d.is_none()
            && self.navmesh.is_none()
    }
}
