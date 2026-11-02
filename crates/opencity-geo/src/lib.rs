//! GeoJSON bridge for OpenCity.
//!
//! Turns map data into a playable city and back:
//!
//! * **Input**: GeoJSON (any feature/geometry type) or Overpass/OSM JSON.
//!   Coordinates can be WGS84 longitude/latitude (OpenStreetMap,
//!   OpenHistoricalMap, OpenHistoryMap) or planar map units (OpenFantasyMap
//!   worlds, which make no Earth assumption).
//! * **Mapping**: a declarative [`Rules`] file maps feature properties to
//!   OpenCity structures; the built-in rules understand OSM-style tags.
//! * **History**: features carry `start_date` / `end_date`; import a city as
//!   it was on a given date.
//! * **Terrain**: elevation from `ele` on points or contour lines, water from
//!   water polygons and rivers.
//! * **Output**: [`export`] writes a city back to GeoJSON with OSM-style tags
//!   plus `opencity:*` properties, in the source coordinate system.

pub mod export;
pub mod import;
pub mod rules;
pub mod source;

pub use export::{ExportOptions, export};
pub use import::{CrsMode, HistDate, ImportOptions, ImportReport, import};
pub use rules::{Build, Rules};

#[derive(Debug, thiserror::Error)]
pub enum GeoError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid map data: {0}")]
    Format(String),
    #[error("invalid rules: {0}")]
    Rules(String),
    #[error("nothing to import: {0}")]
    Empty(String),
}
