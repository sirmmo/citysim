//! Identifiers shared by the simulation, the asset catalog and the renderer.
//!
//! These mirror the enums of the original C++ code (`enum.h`,
//! `opencity_direction.h`, `opencity_structure_type.h`). Graphic codes keep
//! their legacy numeric values because the asset catalog (`graphism.conf`)
//! and legacy save files refer to them by number.

use serde::{Deserialize, Serialize};

/// What a tile *is*, gameplay-wise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum StructureCode {
    Test = 1,
    /// A non-origin tile of a multi-tile structure.
    Part = 3,
    Residential = 10,
    Commercial = 11,
    Industrial = 12,
    Park = 13,
    Flora = 14,
    Road = 20,
    PowerLine = 31,
    CoalPlant = 32,
    NuclearPlant = 35,
    FireDept = 36,
    PoliceDept = 37,
    Hospital = 38,
    School = 40,
    /// Railway station: joins a rail line to the streets around it.
    Station = 41,
}

impl StructureCode {
    pub const fn legacy_id(self) -> u16 {
        self as u16
    }

    pub fn from_legacy_id(id: u16) -> Option<Self> {
        use StructureCode::*;
        Some(match id {
            1 => Test,
            3 => Part,
            10 => Residential,
            11 => Commercial,
            12 => Industrial,
            13 => Park,
            14 => Flora,
            20 => Road,
            31 => PowerLine,
            32 => CoalPlant,
            35 => NuclearPlant,
            36 => FireDept,
            37 => PoliceDept,
            38 => Hospital,
            40 => School,
            41 => Station,
            _ => return None,
        })
    }

    pub const fn is_zone(self) -> bool {
        matches!(self, Self::Residential | Self::Commercial | Self::Industrial)
    }

    pub const fn is_path(self) -> bool {
        matches!(self, Self::Road | Self::PowerLine)
    }

    pub const fn is_power_plant(self) -> bool {
        matches!(self, Self::CoalPlant | Self::NuclearPlant)
    }

    pub const fn is_public_service(self) -> bool {
        matches!(self, Self::FireDept | Self::PoliceDept | Self::Hospital | Self::School | Self::Station)
    }

    /// Structures placed as one fixed-size block (the size comes from the model).
    pub const fn is_block(self) -> bool {
        self.is_power_plant() || self.is_public_service()
    }

    /// Default graphic code of a freshly built structure.
    pub const fn base_graphic(self) -> GraphicCode {
        use StructureCode::*;
        match self {
            Test => GraphicCode::TEST_BUILDING,
            Part => GraphicCode::EMPTY,
            Residential => GraphicCode::RES_ZONE0,
            Commercial => GraphicCode::COM_ZONE0,
            Industrial => GraphicCode::IND_ZONE0,
            Park => GraphicCode::PARK0,
            Flora => GraphicCode::TREE_BEGIN,
            Road => GraphicCode::ROAD_BASE,
            PowerLine => GraphicCode::ELINE_BASE,
            CoalPlant => GraphicCode::EPLANT_COAL,
            NuclearPlant => GraphicCode::EPLANT_NUCLEAR,
            FireDept => GraphicCode::FIRE_DEPT,
            PoliceDept => GraphicCode::POLICE_DEPT,
            Hospital => GraphicCode::HOSPITAL_DEPT,
            School => GraphicCode::EDUCATION_DEPT,
            Station => GraphicCode::RAIL_STATION,
        }
    }

    pub const fn structure_type(self) -> StructureType {
        use StructureCode::*;
        match self {
            Residential | Park => StructureType::Residence,
            Commercial => StructureType::Commerce,
            Industrial => StructureType::Industry,
            CoalPlant | NuclearPlant => StructureType::Electricity,
            Road | PowerLine => StructureType::Path,
            FireDept | PoliceDept | Hospital | School | Station => StructureType::Government,
            Flora => StructureType::Tree,
            Part | Test => StructureType::Undefined,
        }
    }

    /// Cost to build one tile (or one block for plants/services).
    pub const fn build_cost(self) -> u32 {
        use StructureCode::*;
        match self {
            Residential | Commercial | Road | Flora => 5,
            Industrial => 8,
            Park => 100,
            PowerLine => 3,
            CoalPlant => 2000,
            NuclearPlant => 5000,
            FireDept | PoliceDept | Hospital | School => 1500,
            Station => 3000,
            Test | Part => 0,
        }
    }

    /// Monthly upkeep per tile.
    pub const fn maintenance_cost(self) -> u32 {
        use StructureCode::*;
        match self {
            Residential | Commercial | Industrial | Part | Test => 0,
            Park | PowerLine | Road | Flora => 1,
            CoalPlant => 20,
            NuclearPlant => 80,
            FireDept | PoliceDept | Hospital | School => 15,
            Station => 25,
        }
    }

    /// Cost to bulldoze one tile carrying this code at the given level.
    pub const fn destroy_cost(self, level: u8) -> u32 {
        use StructureCode::*;
        let level = level as u32;
        match self {
            Residential | Commercial => 2 * (level + 1),
            Industrial => 3 * (level + 1),
            Park => 10,
            PowerLine => 1,
            Road | Flora => 4,
            Part | CoalPlant | FireDept | PoliceDept | Hospital | School | Station => 200,
            NuclearPlant => 800,
            Test => 0,
        }
    }
}

/// Broad structure family, used by the legacy save format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum StructureType {
    Undefined = 0,
    Residence,
    Commerce,
    Industry,
    Water,
    Electricity,
    Gas,
    Government,
    Path,
    Tree,
    Vehicle,
}

/// Index of a 3D model in the asset catalog (legacy `OPENCITY_GRAPHIC_CODE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GraphicCode(pub u16);

impl GraphicCode {
    pub const EMPTY: Self = Self(0);
    pub const TEST_BUILDING: Self = Self(1);
    pub const RES_ZONE0: Self = Self(10);
    pub const COM_ZONE0: Self = Self(20);
    pub const IND_ZONE0: Self = Self(30);
    /// First of 15 road pieces, see [`path_graphic`].
    pub const ROAD_BASE: Self = Self(150);
    /// First of 15 power line pieces, see [`path_graphic`].
    pub const ELINE_BASE: Self = Self(170);
    pub const EPLANT_COAL: Self = Self(300);
    pub const EPLANT_NUCLEAR: Self = Self(303);
    pub const PARK0: Self = Self(320);
    pub const TREE_BEGIN: Self = Self(330);
    pub const TREE_END: Self = Self(334);
    pub const FIRE_DEPT: Self = Self(350);
    pub const POLICE_DEPT: Self = Self(360);
    pub const HOSPITAL_DEPT: Self = Self(370);
    pub const EDUCATION_DEPT: Self = Self(390);
    pub const RAIL_STATION: Self = Self(395);
    pub const VEHICLE_STD: Self = Self(400);
    pub const VEHICLE_PICKUP: Self = Self(440);
    pub const VEHICLE_SPORT: Self = Self(441);
    pub const MAX: u16 = 491;

    pub const fn offset(self, n: u16) -> Self {
        Self(self.0 + n)
    }
}

/// Bit set of path connections.
pub mod connect {
    pub const NORTH: u8 = 1;
    pub const EAST: u8 = 2;
    pub const SOUTH: u8 = 4;
    pub const WEST: u8 = 8;
}

/// Graphic code of a road/power-line piece connected towards `mask`
/// (see [`connect`]). Replaces the 300-line state machine of
/// `PathStructure::AddNeighbour/RemoveNeighbour` with a lookup table that
/// follows the legacy model order (`OC_ROAD_O_N` .. `OC_ROAD_S_N_W_E`).
pub fn path_graphic(base: GraphicCode, mask: u8) -> GraphicCode {
    // index: bit0=N, bit1=E, bit2=S, bit3=W
    const OFFSET: [u16; 16] = [
        0,  // none -> O_N
        0,  // N    -> O_N
        1,  // E    -> O_E
        6,  // N E  -> N_E
        2,  // S    -> O_S
        4,  // N S  -> S_N
        8,  // E S  -> S_E
        10, // N E S -> S_N_E
        3,  // W    -> O_W
        7,  // N W  -> N_W
        5,  // E W  -> W_E
        13, // N E W -> N_W_E
        9,  // S W  -> S_W
        12, // N S W -> S_N_W
        11, // E S W -> S_W_E
        14, // all  -> S_N_W_E
    ];
    base.offset(OFFSET[(mask & 0xF) as usize])
}

/// Cardinal directions on the map grid (`w` grows east, `l` grows south).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub const ALL: [Direction; 4] = [Self::North, Self::East, Self::South, Self::West];

    pub const fn delta(self) -> (i32, i32) {
        match self {
            Self::North => (0, -1),
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
        }
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }

    pub const fn mask(self) -> u8 {
        match self {
            Self::North => connect::NORTH,
            Self::East => connect::EAST,
            Self::South => connect::SOUTH,
            Self::West => connect::WEST,
        }
    }
}

/// Kinds of road. They share the road network (they connect to each other
/// and carry the same traffic) but differ in cost, speed, capacity and width,
/// and in whether buildings can open onto them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadKind {
    /// Modern asphalt city street.
    #[default]
    Street,
    /// Fast, wide, high-capacity road with no access for buildings.
    Highway,
    /// Beaten earth: cheap, slow, narrow.
    DirtRoad,
    /// Cobbled or flagstone street of pre-industrial towns.
    PavedStreet,
    /// Railway track: trains only, fast, no access for buildings. Stations
    /// join it to the streets.
    Rail,
}

impl RoadKind {
    pub const ALL: [RoadKind; 5] = [RoadKind::Street, RoadKind::Highway, RoadKind::DirtRoad, RoadKind::PavedStreet, RoadKind::Rail];

    /// Stable identifier (saves, rules, skins).
    pub const fn id(self) -> &'static str {
        match self {
            Self::Street => "street",
            Self::Highway => "highway",
            Self::DirtRoad => "dirt_road",
            Self::PavedStreet => "paved_street",
            Self::Rail => "rail",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Street => "Street",
            Self::Highway => "Highway",
            Self::DirtRoad => "Dirt road",
            Self::PavedStreet => "Paved street",
            Self::Rail => "Railway",
        }
    }

    /// Cost to build one tile.
    pub const fn build_cost(self) -> u32 {
        match self {
            Self::Street => 5,
            Self::Highway => 25,
            Self::DirtRoad => 1,
            Self::PavedStreet => 3,
            Self::Rail => 20,
        }
    }

    /// Monthly upkeep per tile.
    pub const fn maintenance_cost(self) -> u32 {
        match self {
            Self::Street | Self::PavedStreet => 1,
            Self::Highway | Self::Rail => 4,
            Self::DirtRoad => 0,
        }
    }

    /// Travel speed relative to a street: trips cover `speed` tiles in the
    /// time a street takes for one.
    pub const fn speed(self) -> f32 {
        match self {
            Self::Street => 1.0,
            Self::Highway => 2.5,
            Self::DirtRoad => 0.5,
            Self::PavedStreet => 0.8,
            Self::Rail => 4.0,
        }
    }

    /// Traffic it carries before slowing down, relative to a street.
    pub const fn capacity(self) -> f32 {
        match self {
            Self::Street => 1.0,
            Self::Highway => 3.0,
            Self::DirtRoad => 0.4,
            Self::PavedStreet => 0.7,
            Self::Rail => 4.0,
        }
    }

    /// Whether buildings can open onto it (and plots next to it develop).
    pub const fn access(self) -> bool {
        !matches!(self, Self::Highway | Self::Rail)
    }

    /// Whether it is track rather than road: cars never use it, and it is
    /// not part of the street network.
    pub const fn is_rail(self) -> bool {
        matches!(self, Self::Rail)
    }

    /// Lanes, for display.
    pub const fn lanes(self) -> u8 {
        match self {
            Self::Highway => 4,
            Self::DirtRoad | Self::Rail => 1,
            Self::Street | Self::PavedStreet => 2,
        }
    }

    /// Paved width in tiles, for display (highways spill over their verges).
    pub const fn width(self) -> f32 {
        match self {
            Self::Street => 1.0,
            Self::Highway => 1.3,
            Self::DirtRoad => 0.5,
            Self::PavedStreet => 0.75,
            Self::Rail => 0.6,
        }
    }

    /// The tool that builds it.
    pub const fn tool(self) -> Tool {
        match self {
            Self::Street => Tool::Road,
            Self::Highway => Tool::Highway,
            Self::DirtRoad => Tool::DirtRoad,
            Self::PavedStreet => Tool::PavedStreet,
            Self::Rail => Tool::Rail,
        }
    }
}

/// Player tools (legacy `OPENCITY_TOOL_CODE`, minus the experimental agents).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tool {
    Destroy,
    ZoneResidential,
    ZoneCommercial,
    ZoneIndustrial,
    RaiseTerrain,
    LowerTerrain,
    Road,
    Highway,
    DirtRoad,
    PavedStreet,
    PowerLine,
    CoalPlant,
    NuclearPlant,
    Park,
    Trees,
    FireDept,
    PoliceDept,
    Hospital,
    School,
    TestBuilding,
    Rail,
    Station,
}

impl Tool {
    pub const ALL: [Tool; 22] = [
        Tool::Destroy,
        Tool::ZoneResidential,
        Tool::ZoneCommercial,
        Tool::ZoneIndustrial,
        Tool::RaiseTerrain,
        Tool::LowerTerrain,
        Tool::Road,
        Tool::Highway,
        Tool::DirtRoad,
        Tool::PavedStreet,
        Tool::PowerLine,
        Tool::CoalPlant,
        Tool::NuclearPlant,
        Tool::Park,
        Tool::Trees,
        Tool::FireDept,
        Tool::PoliceDept,
        Tool::Hospital,
        Tool::School,
        Tool::TestBuilding,
        Tool::Rail,
        Tool::Station,
    ];

    /// Stable identifier, used by skins and GeoJSON rules.
    pub const fn id(self) -> &'static str {
        match self {
            Tool::Destroy => "bulldozer",
            Tool::ZoneResidential => "residential",
            Tool::ZoneCommercial => "commercial",
            Tool::ZoneIndustrial => "industrial",
            Tool::RaiseTerrain => "raise",
            Tool::LowerTerrain => "lower",
            Tool::Road => "road",
            Tool::Highway => "highway",
            Tool::DirtRoad => "dirt_road",
            Tool::PavedStreet => "paved_street",
            Tool::PowerLine => "power_line",
            Tool::CoalPlant => "coal_plant",
            Tool::NuclearPlant => "nuclear_plant",
            Tool::Park => "park",
            Tool::Trees => "trees",
            Tool::FireDept => "fire_station",
            Tool::PoliceDept => "police",
            Tool::Hospital => "hospital",
            Tool::School => "school",
            Tool::TestBuilding => "test_building",
            Tool::Rail => "rail",
            Tool::Station => "station",
        }
    }

    /// Structure this tool builds, if any.
    pub const fn structure(self) -> Option<StructureCode> {
        use StructureCode as S;
        Some(match self {
            Tool::ZoneResidential => S::Residential,
            Tool::ZoneCommercial => S::Commercial,
            Tool::ZoneIndustrial => S::Industrial,
            Tool::Road | Tool::Highway | Tool::DirtRoad | Tool::PavedStreet | Tool::Rail => S::Road,
            Tool::PowerLine => S::PowerLine,
            Tool::CoalPlant => S::CoalPlant,
            Tool::NuclearPlant => S::NuclearPlant,
            Tool::Park => S::Park,
            Tool::Trees => S::Flora,
            Tool::FireDept => S::FireDept,
            Tool::PoliceDept => S::PoliceDept,
            Tool::Hospital => S::Hospital,
            Tool::School => S::School,
            Tool::TestBuilding => S::Test,
            Tool::Station => S::Station,
            Tool::Destroy | Tool::RaiseTerrain | Tool::LowerTerrain => return None,
        })
    }

    /// The kind of road a road tool lays.
    pub const fn road_kind(self) -> Option<RoadKind> {
        Some(match self {
            Tool::Road => RoadKind::Street,
            Tool::Highway => RoadKind::Highway,
            Tool::DirtRoad => RoadKind::DirtRoad,
            Tool::PavedStreet => RoadKind::PavedStreet,
            Tool::Rail => RoadKind::Rail,
            _ => return None,
        })
    }

    /// Whether the tool acts on a dragged rectangle (vs. a single anchor tile).
    pub const fn is_area(self) -> bool {
        match self.structure() {
            Some(s) => !s.is_block(),
            None => matches!(self, Tool::Destroy),
        }
    }

    pub const fn sound(self) -> Sound {
        match self {
            Tool::Destroy => Sound::Destroy,
            Tool::ZoneResidential | Tool::ZoneCommercial | Tool::ZoneIndustrial => Sound::Zone,
            Tool::RaiseTerrain | Tool::LowerTerrain => Sound::Terrain,
            Tool::Road | Tool::Highway | Tool::DirtRoad | Tool::PavedStreet | Tool::Rail => Sound::Road,
            Tool::PowerLine => Sound::PowerLine,
            Tool::Park | Tool::Trees => Sound::Park,
            _ => Sound::Plant,
        }
    }
}

/// Sound effects the game can request (legacy `OPENCITY_SOUND_EFFECT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sound {
    Destroy,
    PowerLine,
    Plant,
    Park,
    Zone,
    Road,
    Terrain,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_table_matches_legacy_order() {
        use connect::*;
        let b = GraphicCode::ROAD_BASE;
        assert_eq!(path_graphic(b, 0), GraphicCode(150));
        assert_eq!(path_graphic(b, EAST), GraphicCode(151));
        assert_eq!(path_graphic(b, SOUTH), GraphicCode(152));
        assert_eq!(path_graphic(b, WEST), GraphicCode(153));
        assert_eq!(path_graphic(b, NORTH | SOUTH), GraphicCode(154));
        assert_eq!(path_graphic(b, WEST | EAST), GraphicCode(155));
        assert_eq!(path_graphic(b, NORTH | EAST), GraphicCode(156));
        assert_eq!(path_graphic(b, NORTH | WEST), GraphicCode(157));
        assert_eq!(path_graphic(b, SOUTH | EAST), GraphicCode(158));
        assert_eq!(path_graphic(b, SOUTH | WEST), GraphicCode(159));
        assert_eq!(path_graphic(b, SOUTH | NORTH | EAST), GraphicCode(160));
        assert_eq!(path_graphic(b, SOUTH | WEST | EAST), GraphicCode(161));
        assert_eq!(path_graphic(b, SOUTH | NORTH | WEST), GraphicCode(162));
        assert_eq!(path_graphic(b, NORTH | WEST | EAST), GraphicCode(163));
        assert_eq!(path_graphic(b, 15), GraphicCode(164));
    }

    #[test]
    fn road_kinds_have_tools_and_ids() {
        for k in RoadKind::ALL {
            assert_eq!(k.tool().road_kind(), Some(k));
            assert_eq!(RoadKind::from_id(k.id()), Some(k));
            assert_eq!(k.tool().structure(), Some(StructureCode::Road));
        }
        assert!(!RoadKind::Highway.access() && RoadKind::DirtRoad.access());
    }

    #[test]
    fn legacy_ids_round_trip() {
        for id in 0..100 {
            if let Some(code) = StructureCode::from_legacy_id(id) {
                assert_eq!(code.legacy_id(), id);
            }
        }
    }
}
