use opencity_core::{Footprints, StructureCode};
use opencity_geo::{import, HistDate, ImportOptions, Rules};

fn data(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn count(city: &opencity_core::City, code: StructureCode) -> usize {
    city.layer.structures().filter(|(_, s)| s.code == code).count()
}

#[test]
fn historic_town_changes_with_the_date() {
    let text = data("historic_town.geojson");
    let rules = Rules::builtin();
    let at = |year: &str| {
        let opts = ImportOptions { date: HistDate::parse(year), ..Default::default() };
        import(&text, &rules, Footprints::builtin(), &opts).unwrap()
    };
    let (c1860, r1860) = at("1860");
    let (c1920, r1920) = at("1920");
    let (c1975, r1975) = at("1975");
    println!("1860:\n{r1860}\n1920:\n{r1920}\n1975:\n{r1975}");
    assert!(r1920.wgs84);
    assert!(c1920.width() > 20 && c1920.width() < 50, "{}", c1920.width());
    // Industry existed 1870-1950, the coal plant 1905-1970, the school from 1910.
    assert_eq!(count(&c1860, StructureCode::Industrial), 0);
    assert!(count(&c1920, StructureCode::Industrial) > 0);
    assert_eq!(count(&c1975, StructureCode::Industrial), 0);
    assert_eq!(count(&c1920, StructureCode::CoalPlant), 1);
    assert_eq!(count(&c1975, StructureCode::CoalPlant), 0);
    assert_eq!(count(&c1920, StructureCode::School), 1);
    // Roads grow over time.
    assert!(count(&c1975, StructureCode::Road) > count(&c1920, StructureCode::Road));
    assert!(count(&c1920, StructureCode::Road) > count(&c1860, StructureCode::Road));
    // The apartment block keeps its floors as a zone level.
    assert!(c1920.layer.structures().any(|(_, s)| s.code == StructureCode::Residential && s.level > 0));
    // Terrain: water below sea level, elevation from `ele` samples.
    let t = &c1920.terrain;
    assert!(t.corner_heights().iter().any(|&h| h < 0));
    assert!(t.corner_heights().iter().any(|&h| h > 0));
    assert!(r1920.elevation_samples == 2);
    // Georeference kept for export.
    assert_eq!(c1920.meta.year, Some(1920));
    let g = c1920.meta.georef.unwrap();
    assert!(g.wgs84 && (g.west - 11.34).abs() < 0.01);
}

#[test]
fn fantasy_village_with_custom_rules_and_planar_coordinates() {
    let rules = Rules::parse(&data("fantasy_rules.toml")).unwrap();
    let (city, report) = import(&data("fantasy_village.geojson"), &rules, Footprints::builtin(), &ImportOptions::default()).unwrap();
    println!("{report}");
    assert!(!report.wgs84);
    assert_eq!(city.meta.world.as_deref(), Some("Eldoria"));
    for code in [
        StructureCode::Road,
        StructureCode::Residential,
        StructureCode::Commercial,
        StructureCode::Industrial,
        StructureCode::CoalPlant,
        StructureCode::PowerLine,
        StructureCode::Flora,
    ] {
        assert!(count(&city, code) > 0, "{code:?} missing\n{report}");
    }
    // The tower powers the village through the ley line.
    assert!(city.values.power_capacity > 0);
}

#[test]
fn nothing_matching_is_an_error() {
    let err = import(r#"{"type":"FeatureCollection","features":[]}"#, &Rules::builtin(), Footprints::builtin(), &ImportOptions::default());
    assert!(err.is_err());
}
