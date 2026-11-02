#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialQueryResult {
    pub entity_id: u32,
    pub distance_squared: f64,
    pub distance: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VisibilityDelta {

    pub entered: Vec<u32>,

    pub left: Vec<u32>,
}
