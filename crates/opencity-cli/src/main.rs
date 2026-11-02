//! Headless OpenCity.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use opencity_assets::{find_data_dir, Catalog};
use opencity_core::{save, scenario, City, Footprints, StructureCode, Terrain};
use opencity_mapgen::{MapParams, Relief, Shape, TreeDensity, Water};

#[derive(Parser)]
#[command(version, about = "Headless OpenCity: create, simulate, inspect and convert cities")]
struct Cli {
    /// Data directory (the one containing graphism.conf).
    #[arg(long, global = true, env = "OPENCITY_DATA")]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a new city and save it.
    New {
        #[command(flatten)]
        map: MapArgs,
        /// Lay out the demo town.
        #[arg(long)]
        demo: bool,
        /// GlassBox game rules (e.g. "glassbox"); default: classic OpenCity rules.
        #[arg(long)]
        game_rules: Option<String>,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Run the simulation for a number of years and print yearly statistics.
    Simulate {
        /// City to load (a new map is generated otherwise).
        #[arg(long)]
        load: Option<PathBuf>,
        #[command(flatten)]
        map: MapArgs,
        #[arg(long)]
        demo: bool,
        /// GlassBox game rules (e.g. "glassbox"); default: classic OpenCity rules.
        #[arg(long)]
        game_rules: Option<String>,
        #[arg(long, default_value_t = 10)]
        years: u32,
        #[arg(long)]
        save: Option<PathBuf>,
        /// Write the GlassBox metrics (one row per tick) to this CSV file.
        #[arg(long)]
        metrics: Option<PathBuf>,
    },
    /// Print a summary of a saved city.
    Info { file: PathBuf },
    /// Convert a save file from the original C++ OpenCity.
    ImportLegacy { input: PathBuf, output: PathBuf },
    /// Build a city from GeoJSON or Overpass JSON (OpenFantasyMap, OpenHistoryMap, OSM...).
    ImportGeojson {
        input: PathBuf,
        #[command(flatten)]
        geo: GeoArgs,
        /// Simulate this many years after the import.
        #[arg(long, default_value_t = 0)]
        years: u32,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Write a saved city as GeoJSON (OSM-style tags + opencity:* properties).
    ExportGeojson {
        input: PathBuf,
        /// Skip water tiles and elevation points.
        #[arg(long)]
        no_terrain: bool,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Download 0 A.D. art from its official repository and convert it into
    /// the ancient culture skins (no Docker needed).
    #[command(name = "bake-0ad")]
    Bake0ad {
        /// Read a local 0 A.D. checkout instead of downloading.
        #[arg(long)]
        from: Option<PathBuf>,
        /// Download cache (default: the user cache directory).
        #[arg(long)]
        cache: Option<PathBuf>,
    },
    /// Render a top-down PNG of a city (or of a freshly generated map).
    Render {
        #[arg(long)]
        load: Option<PathBuf>,
        #[command(flatten)]
        map: MapArgs,
        /// Pixels per tile.
        #[arg(long, default_value_t = 8)]
        scale: u32,
        #[arg(short, long)]
        out: PathBuf,
    },
}

#[derive(Args)]
struct MapArgs {
    #[arg(long, default_value_t = 64)]
    width: u32,
    #[arg(long, default_value_t = 64)]
    length: u32,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, value_enum, default_value = "plain")]
    relief: ReliefArg,
    #[arg(long, value_enum, default_value = "lake")]
    water: WaterArg,
    #[arg(long, value_enum, default_value = "none")]
    shape: ShapeArg,
    #[arg(long, value_enum, default_value = "sparse")]
    trees: TreesArg,
    /// Greyscale PNG heightmap instead of the generator.
    #[arg(long)]
    heightmap: Option<PathBuf>,
}

/// Options shared by every GeoJSON import.
#[derive(Args, Clone)]
pub struct GeoArgs {
    /// Tag mapping rules (TOML); defaults to OSM-style tags.
    #[arg(long)]
    rules: Option<PathBuf>,
    /// Import the city as it was on this date (YYYY, YYYY-MM or YYYY-MM-DD).
    #[arg(long)]
    date: Option<String>,
    /// Coordinate system of the input.
    #[arg(long, value_enum, default_value = "auto")]
    crs: CrsArg,
    /// Metres (WGS84) or map units (planar) per tile.
    #[arg(long)]
    units_per_tile: Option<f64>,
    /// Largest city side in tiles.
    #[arg(long, default_value_t = 128)]
    max_size: u32,
    /// Only use features inside min_x,min_y,max_x,max_y (source coordinates).
    #[arg(long, value_delimiter = ',', num_args = 4)]
    bbox: Option<Vec<f64>>,
    /// Metres of elevation per terrain step.
    #[arg(long, default_value_t = 8.0)]
    metres_per_step: f64,
    /// Fantasy world the map belongs to (overrides the file's `world` member; selects world skins).
    #[arg(long)]
    world: Option<String>,
}

fn import_geojson(path: &Path, geo: &GeoArgs, fp: Footprints) -> Result<City> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let rules = match &geo.rules {
        Some(r) => opencity_geo::Rules::parse(&std::fs::read_to_string(r).with_context(|| format!("reading {}", r.display()))?)?,
        None => opencity_geo::Rules::builtin(),
    };
    let date = match &geo.date {
        Some(d) => Some(opencity_geo::HistDate::parse(d).with_context(|| format!("bad date {d:?}"))?),
        None => None,
    };
    let opts = opencity_geo::ImportOptions {
        crs: geo.crs.into(),
        units_per_tile: geo.units_per_tile,
        max_size: geo.max_size,
        date,
        bbox: geo.bbox.as_ref().map(|b| [b[0], b[1], b[2], b[3]]),
        metres_per_step: geo.metres_per_step,
        world: geo.world.clone(),
        name: path.file_stem().map(|s| s.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let (mut city, report) = opencity_geo::import(&text, &rules, fp, &opts)?;
    city.meta.source = Some(path.display().to_string());
    eprint!("{report}");
    Ok(city)
}

macro_rules! value_enum {
    ($name:ident => $target:ty { $($v:ident),* }) => {
        #[derive(Clone, Copy, clap::ValueEnum)]
        enum $name { $($v),* }
        impl From<$name> for $target {
            fn from(v: $name) -> Self { match v { $($name::$v => <$target>::$v),* } }
        }
    };
}
value_enum!(ReliefArg => Relief { Plain, Hill, Mountain });
value_enum!(WaterArg => Water { Dry, Lake, Coast });
value_enum!(ShapeArg => Shape { None, Island, Volcano, Crater });
value_enum!(TreesArg => TreeDensity { Sparse, Normal, Dense });
value_enum!(CrsArg => opencity_geo::CrsMode { Auto, Wgs84, Planar });

impl MapArgs {
    fn params(&self) -> MapParams {
        MapParams {
            width: self.width,
            length: self.length,
            relief: self.relief.into(),
            water: self.water.into(),
            shape: self.shape.into(),
            trees: self.trees.into(),
            seed: self.seed,
            heightmap: self.heightmap.clone(),
        }
    }
}

fn footprints(data_dir: Option<&Path>) -> Footprints {
    match find_data_dir(data_dir).and_then(|d| Catalog::load(&d)) {
        Ok(c) => c.footprints(),
        Err(e) => {
            log::warn!("{e}; using built-in model sizes");
            Footprints::builtin()
        }
    }
}

fn new_city(map: &MapArgs, demo: bool, fp: Footprints) -> Result<City> {
    let params = map.params();
    let m = opencity_mapgen::generate(&params)?;
    let terrain = Terrain::from_corner_heights(m.width, m.length, &m.corner_heights);
    let mut city = City::new(terrain, Some(&m.tree_density), fp, params.seed);
    if demo {
        scenario::demo_town(&mut city);
    }
    Ok(city)
}

/// GlassBox metric history as CSV: `tick,<metric>,...`.
fn write_metrics(city: &City, path: &Path) -> Result<()> {
    let gb = city.glassbox.as_ref().context("--metrics needs GlassBox rules (--game-rules)")?;
    let names: std::collections::BTreeSet<&String> = gb.metric_history.iter().flat_map(|(_, m)| m.keys()).collect();
    let mut out = String::from("tick");
    for n in &names {
        out.push(',');
        out.push_str(n);
    }
    out.push('\n');
    for (tick, m) in &gb.metric_history {
        out.push_str(&tick.to_string());
        for n in &names {
            out.push_str(&format!(",{:.3}", m.get(*n).copied().unwrap_or(0.0)));
        }
        out.push('\n');
    }
    std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))?;
    println!("wrote {} ({} ticks)", path.display(), gb.metric_history.len());
    Ok(())
}

/// Switches a city to (or re-attaches) a GlassBox rules set.
fn attach_rules(city: &mut City, rules: Option<&str>, data_dir: Option<&Path>) -> Result<()> {
    if let Some(id) = rules {
        let dir = find_data_dir(data_dir)?;
        city.use_rules(opencity_assets::rules::load(&dir, id)?);
    }
    Ok(())
}

fn load_city(path: &Path, fp: Footprints) -> Result<City> {
    let f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    Ok(save::load(BufReader::new(f), fp)?)
}

fn save_city(city: &City, path: &Path) -> Result<()> {
    let f = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    save::save(city, BufWriter::new(f))?;
    println!("saved {}", path.display());
    Ok(())
}

fn print_summary(city: &City) {
    let v = city.values;
    println!("size      {} x {}", city.width(), city.length());
    println!("date      {}", city.date);
    println!("fund      {}", city.fund);
    println!("population {}", city.population);
    println!("R/C/I     {}/{}/{}", v.residential, v.commercial, v.industrial);
    println!("power     {} / {}", v.power, v.power_capacity);
    println!("tiles     {} occupied, {} vehicles", city.layer.occupied(), city.vehicles.len());
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let cli = Cli::parse();
    let fp = || footprints(cli.data_dir.as_deref());
    match &cli.command {
        Command::New { map, demo, game_rules, out } => {
            let mut city = new_city(map, *demo, fp())?;
            attach_rules(&mut city, game_rules.as_deref(), cli.data_dir.as_deref())?;
            print_summary(&city);
            save_city(&city, out)?;
        }
        Command::Simulate { load, map, demo, game_rules, years, save, metrics } => {
            let mut city = match load {
                Some(p) => load_city(p, fp())?,
                None => new_city(map, *demo, fp())?,
            };
            let rules = game_rules.clone().or_else(|| city.glassbox.as_ref().map(|g| g.rules_id.clone()));
            attach_rules(&mut city, rules.as_deref(), cli.data_dir.as_deref())?;
            println!("{:>5} {:>9} {:>7} {:>6} {:>6} {:>6} {:>13}", "year", "fund", "people", "R", "C", "I", "power");
            let target = city.date.year + years;
            while city.date.year < target {
                let year = city.date.year;
                // One simulated day at a time.
                city.update(opencity_core::city::MS_PER_DAY);
                if city.date.year != year {
                    let v = city.values;
                    println!(
                        "{:>5} {:>9} {:>7} {:>6} {:>6} {:>6} {:>6}/{:<6}",
                        city.date.year, city.fund, city.population, v.residential, v.commercial, v.industrial, v.power, v.power_capacity
                    );
                }
            }
            if let Some(p) = save {
                save_city(&city, p)?;
            }
            if let Some(p) = metrics {
                write_metrics(&city, p)?;
            }
        }
        Command::Info { file } => print_summary(&load_city(file, fp())?),
        Command::ImportLegacy { input, output } => {
            let data = std::fs::read(input).with_context(|| format!("reading {}", input.display()))?;
            let city = save::import_legacy(&data, fp())?;
            print_summary(&city);
            save_city(&city, output)?;
        }
        Command::ImportGeojson { input, geo, years, out } => {
            let mut city = import_geojson(input, geo, fp())?;
            let target = city.date.year + years;
            while city.date.year < target {
                city.update(opencity_core::city::MS_PER_DAY);
            }
            print_summary(&city);
            save_city(&city, out)?;
        }
        Command::ExportGeojson { input, no_terrain, out } => {
            let city = load_city(input, fp())?;
            let opts = opencity_geo::ExportOptions { terrain: !no_terrain, ..Default::default() };
            let json = opencity_geo::export(&city, &opts);
            std::fs::write(out, serde_json::to_string(&json)?).with_context(|| format!("writing {}", out.display()))?;
            println!("wrote {} ({} features)", out.display(), json["features"].as_array().map_or(0, Vec::len));
        }
        Command::Bake0ad { from, cache } => {
            let dir = find_data_dir(cli.data_dir.as_deref())?;
            let opts = opencity_zeroad::Options { cache_dir: cache.clone(), local_repo: from.clone() };
            let report = opencity_zeroad::bake(&dir, &opts, &|m| eprintln!("{m}"))?;
            for (name, models, textures) in &report.cultures {
                println!("{name:<12} {models:>3} models {textures:>3} textures");
            }
            println!("downloaded {} MB; 0 A.D. art installed in {}", report.downloaded_bytes >> 20, dir.join("skins").display());
            println!("Start citysim and pick an ancient culture in File > New city. The art is CC BY-SA 3.0 (0 A.D., Wildfire Games).");
        }
        Command::Render { load, map, scale, out } => {
            let city = match load {
                Some(p) => load_city(p, fp())?,
                None => new_city(map, false, fp())?,
            };
            render(&city, *scale).save(out)?;
            println!("wrote {}", out.display());
        }
    }
    Ok(())
}

/// Top-down map: terrain shaded by height, structures by type.
fn render(city: &City, scale: u32) -> image::RgbImage {
    let mut img = image::RgbImage::new(city.width() * scale, city.length() * scale);
    for l in 0..city.length() {
        for w in 0..city.width() {
            let h = city.terrain.tile_min(w, l) as i32;
            let ground = if h < 0 {
                [30, 60 + (h + 10).max(0) as u8 * 10, 150]
            } else {
                let v = (h * 12).min(120) as u8;
                [70 + v, 110 + v / 2, 50 + v / 3]
            };
            let color = match city.layer.structure_at(w, l).map(|s| (s.code, s.level)) {
                None => ground,
                Some((code, level)) => {
                    let boost = (level as u32 * 2).min(100) as u8;
                    match code {
                        StructureCode::Residential => [40, 160 + boost / 2, 40],
                        StructureCode::Commercial => [40, 80, 160 + boost / 2],
                        StructureCode::Industrial => [180 + boost / 2, 160, 40],
                        StructureCode::Road => [90, 90, 90],
                        StructureCode::PowerLine => [220, 220, 120],
                        StructureCode::Flora => [20, 90, 20],
                        StructureCode::Park => [80, 200, 80],
                        c if c.is_power_plant() => [200, 60, 60],
                        _ => [200, 200, 200],
                    }
                }
            };
            for y in 0..scale {
                for x in 0..scale {
                    img.put_pixel(w * scale + x, l * scale + y, image::Rgb(color));
                }
            }
        }
    }
    img
}
