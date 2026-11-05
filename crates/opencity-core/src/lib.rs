//! OpenCity simulation engine.
//!
//! A Rust port of the simulation part of OpenCity 0.0.7 (C++/SDL, 2004-2010).
//! Everything here is pure, deterministic for a given seed, and free of any
//! graphics or audio dependency, so it runs the same in the game, in the
//! headless CLI and in tests.

pub mod city;
pub mod codes;
pub mod footprint;
pub mod gbx;
pub mod glassbox;
pub mod goals;
pub mod layer;
pub mod netmap;
pub mod rail;
pub mod region;
pub mod rules;
pub mod save;
pub mod scenario;
pub mod series;
pub mod sim;
pub mod terrain;
pub mod traffic;

pub use city::{City, CityMeta, Date, GeoRef, Record, Speed, TileInfo, ToolError, ToolOutcome};
pub use codes::{Direction, GraphicCode, RoadKind, Sound, StructureCode, StructureType, Tool};
pub use footprint::{Footprint, Footprints};
pub use layer::{BuildError, BuildingLayer, Rect, Status, Structure};
pub use terrain::Terrain;
