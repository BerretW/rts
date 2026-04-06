/// Datové definice jednotek a budov – vše co lze odvodit ze statických dat.
///
/// Runtime stav (pozice, hp, výběr…) zůstává v ECS komponentách (`components.rs`).
/// Definice slouží jako "šablony" pro spawn a UI.

// ── Frakce ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Faction {
    Human,
    Orc,
    Neutral,
}

// ── Suroviny ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default)]
pub struct Resources {
    pub gold:   u32,
    pub lumber: u32,
    pub oil:    u32,
}

impl Resources {
    pub const fn new(gold: u32, lumber: u32) -> Self {
        Self { gold, lumber, oil: 0 }
    }
    pub const fn with_oil(gold: u32, lumber: u32, oil: u32) -> Self {
        Self { gold, lumber, oil }
    }
    pub const ZERO: Self = Self { gold: 0, lumber: 0, oil: 0 };
}

// ─────────────────────────────────────────────────────────────────────────────
// JEDNOTKY
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnitKind {
    // ── Humanité ──────────────────────────
    Peasant,
    Footman,
    Archer,
    Ballista,
    Knight,
    Paladin,
    Mage,
    Dwarven,        // Dwarven Demolition Squad
    GryphonRider,
    // ── Námořní (humanité) ───────────────
    Elven,          // Elven Destroyer
    Battleship,
    GnomishSub,
    Transport,
    // ── Orci ─────────────────────────────
    Peon,
    Grunt,
    TrollAxethrower,
    Catapult,
    OgreKnight,
    OgreMage,
    DeathKnight,
    Goblin,         // Goblin Sappers
    DragonRider,
    // ── Námořní (orci) ───────────────────
    TrollDestroyer,
    OgreJugger,
    GiantTurtle,
    OrcTransport,
    // ── Neutrální / speciální ─────────────
    Critter,
    Skeleton,
    Daemon,
}

/// Kompletní statistiky jednotky.
#[derive(Clone, Debug)]
pub struct UnitDef {
    pub kind:             UnitKind,
    pub name:             &'static str,
    pub faction:          Faction,
    /// Maximální HP.
    pub hp_max:           i32,
    /// Pohybová rychlost (herní px/s).
    pub speed:            f32,
    /// Dosah výhledu (dlaždice).
    pub sight:            u32,
    /// Základní útočné poškození.
    pub attack_damage:    i32,
    /// Piercing poškození (nezastavitelné zbrojí).
    pub pierce_damage:    i32,
    /// Dosah útoku (dlaždice; 0 = melee).
    pub attack_range:     f32,
    /// Cooldown útoku (s).
    pub attack_cooldown:  f32,
    /// Brnění.
    pub armor:            i32,
    /// Cena produkce.
    pub cost:             Resources,
    /// Čas výroby (s).
    pub build_time:       f32,
    /// Kde se vyrábí.
    pub produced_in:      &'static [BuildingKind],
    /// Vyžaduje výzkum? (pro Tier 2+)
    pub requires_upgrade: bool,
    /// Velikost v herních pixelech (pro renderer).
    pub sprite_size:      f32,
    /// Pozice ve spritesheet (sloupec, řádek).
    pub sprite:           (u32, u32),
    /// True = může jít po vodě.
    pub naval:            bool,
    /// True = může létat.
    pub air:              bool,
    /// Může sbírat dřevo/zlato.
    pub can_harvest:      bool,
    /// Může stavět budovy.
    pub can_build:        bool,
}

impl UnitDef {
    /// Vrátí tým-barvu pro renderer [R,G,B,A].
    pub fn team_color(faction: Faction) -> [f32; 4] {
        match faction {
            Faction::Human   => [0.20, 0.45, 1.00, 1.0],
            Faction::Orc     => [0.80, 0.20, 0.10, 1.0],
            Faction::Neutral => [0.70, 0.70, 0.70, 1.0],
        }
    }
}

// ── Tabulka všech definic jednotek ───────────────────────────────────────────

pub fn all_unit_defs() -> Vec<UnitDef> {
    use UnitKind::*;
    use Faction::*;
    use BuildingKind as B;

    vec![
        // ── Humanité – pracant ───────────────────────────────────────────
        UnitDef {
            kind: Peasant, name: "Peasant", faction: Human,
            hp_max: 30,   speed: 128.0, sight: 4,
            attack_damage: 3,  pierce_damage: 0, attack_range: 0.0, attack_cooldown: 1.5,
            armor: 0, cost: Resources::new(400, 0), build_time: 45.0,
            produced_in: &[B::TownHall, B::Keep, B::Castle],
            requires_upgrade: false, sprite_size: 32.0, sprite: (1, 0),
            naval: false, air: false, can_harvest: true, can_build: true,
        },
        // ── Humanité – pěšák ─────────────────────────────────────────────
        UnitDef {
            kind: Footman, name: "Footman", faction: Human,
            hp_max: 60,   speed: 128.0, sight: 4,
            attack_damage: 6,  pierce_damage: 3, attack_range: 0.0, attack_cooldown: 1.0,
            armor: 2, cost: Resources::new(600, 0), build_time: 60.0,
            produced_in: &[B::Barracks],
            requires_upgrade: false, sprite_size: 32.0, sprite: (2, 0),
            naval: false, air: false, can_harvest: false, can_build: false,
        },
        // ── Humanité – lučišník ──────────────────────────────────────────
        UnitDef {
            kind: Archer, name: "Elven Archer", faction: Human,
            hp_max: 40,   speed: 128.0, sight: 5,
            attack_damage: 3,  pierce_damage: 6, attack_range: 4.0, attack_cooldown: 1.0,
            armor: 0, cost: Resources::new(500, 50), build_time: 70.0,
            produced_in: &[B::Barracks],
            requires_upgrade: false, sprite_size: 32.0, sprite: (3, 0),
            naval: false, air: false, can_harvest: false, can_build: false,
        },
        // ── Humanité – balista ───────────────────────────────────────────
        UnitDef {
            kind: Ballista, name: "Ballista", faction: Human,
            hp_max: 110,  speed: 64.0,  sight: 9,
            attack_damage: 80, pierce_damage: 0, attack_range: 8.0, attack_cooldown: 3.5,
            armor: 0, cost: Resources::new(900, 300), build_time: 250.0,
            produced_in: &[B::Blacksmith],
            requires_upgrade: false, sprite_size: 48.0, sprite: (4, 0),
            naval: false, air: false, can_harvest: false, can_build: false,
        },
        // ── Humanité – rytíř ─────────────────────────────────────────────
        UnitDef {
            kind: Knight, name: "Knight", faction: Human,
            hp_max: 90,   speed: 192.0, sight: 4,
            attack_damage: 8,  pierce_damage: 4, attack_range: 0.0, attack_cooldown: 1.0,
            armor: 4, cost: Resources::new(800, 100), build_time: 90.0,
            produced_in: &[B::Stables],
            requires_upgrade: false, sprite_size: 32.0, sprite: (5, 0),
            naval: false, air: false, can_harvest: false, can_build: false,
        },
        // ── Humanité – mág ───────────────────────────────────────────────
        UnitDef {
            kind: Mage, name: "Mage", faction: Human,
            hp_max: 35,   speed: 128.0, sight: 9,
            attack_damage: 0,  pierce_damage: 9, attack_range: 5.0, attack_cooldown: 1.5,
            armor: 0, cost: Resources::new(1200, 0), build_time: 120.0,
            produced_in: &[B::MageTower],
            requires_upgrade: false, sprite_size: 32.0, sprite: (6, 0),
            naval: false, air: false, can_harvest: false, can_build: false,
        },
        // ── Humanité – gryf ──────────────────────────────────────────────
        UnitDef {
            kind: GryphonRider, name: "Gryphon Rider", faction: Human,
            hp_max: 100,  speed: 384.0, sight: 6,
            attack_damage: 16, pierce_damage: 0, attack_range: 0.0, attack_cooldown: 1.5,
            armor: 5, cost: Resources::new(2500, 0), build_time: 250.0,
            produced_in: &[B::GryphonAviary],
            requires_upgrade: false, sprite_size: 40.0, sprite: (7, 0),
            naval: false, air: true, can_harvest: false, can_build: false,
        },
        // ── Orci – peon ─────────────────────────────────────────────────
        UnitDef {
            kind: Peon, name: "Peon", faction: Orc,
            hp_max: 30,   speed: 128.0, sight: 4,
            attack_damage: 3,  pierce_damage: 0, attack_range: 0.0, attack_cooldown: 1.5,
            armor: 0, cost: Resources::new(400, 0), build_time: 45.0,
            produced_in: &[B::GreatHall, B::Stronghold, B::Fortress],
            requires_upgrade: false, sprite_size: 32.0, sprite: (1, 1),
            naval: false, air: false, can_harvest: true, can_build: true,
        },
        // ── Orci – grunt ─────────────────────────────────────────────────
        UnitDef {
            kind: Grunt, name: "Grunt", faction: Orc,
            hp_max: 60,   speed: 128.0, sight: 4,
            attack_damage: 8,  pierce_damage: 2, attack_range: 0.0, attack_cooldown: 1.0,
            armor: 2, cost: Resources::new(600, 0), build_time: 60.0,
            produced_in: &[B::OrcBarracks],
            requires_upgrade: false, sprite_size: 32.0, sprite: (2, 1),
            naval: false, air: false, can_harvest: false, can_build: false,
        },
        // ── Orci – troll sekeromet ────────────────────────────────────────
        UnitDef {
            kind: TrollAxethrower, name: "Troll Axethrower", faction: Orc,
            hp_max: 40,   speed: 128.0, sight: 5,
            attack_damage: 3,  pierce_damage: 6, attack_range: 4.0, attack_cooldown: 1.0,
            armor: 0, cost: Resources::new(500, 50), build_time: 70.0,
            produced_in: &[B::OrcBarracks],
            requires_upgrade: false, sprite_size: 32.0, sprite: (3, 1),
            naval: false, air: false, can_harvest: false, can_build: false,
        },
        // ── Orci – katapult ──────────────────────────────────────────────
        UnitDef {
            kind: Catapult, name: "Catapult", faction: Orc,
            hp_max: 110,  speed: 64.0,  sight: 9,
            attack_damage: 80, pierce_damage: 0, attack_range: 8.0, attack_cooldown: 3.5,
            armor: 0, cost: Resources::new(900, 300), build_time: 250.0,
            produced_in: &[B::OrcBlacksmith],
            requires_upgrade: false, sprite_size: 48.0, sprite: (4, 1),
            naval: false, air: false, can_harvest: false, can_build: false,
        },
        // ── Orci – rytíř ogr ─────────────────────────────────────────────
        UnitDef {
            kind: OgreKnight, name: "Ogre", faction: Orc,
            hp_max: 90,   speed: 128.0, sight: 4,
            attack_damage: 10, pierce_damage: 2, attack_range: 0.0, attack_cooldown: 1.3,
            armor: 4, cost: Resources::new(800, 100), build_time: 90.0,
            produced_in: &[B::OgreMound],
            requires_upgrade: false, sprite_size: 40.0, sprite: (5, 1),
            naval: false, air: false, can_harvest: false, can_build: false,
        },
        // ── Orci – rytíř smrti ────────────────────────────────────────────
        UnitDef {
            kind: DeathKnight, name: "Death Knight", faction: Orc,
            hp_max: 60,   speed: 192.0, sight: 9,
            attack_damage: 0,  pierce_damage: 9, attack_range: 5.0, attack_cooldown: 1.5,
            armor: 0, cost: Resources::new(1200, 0), build_time: 120.0,
            produced_in: &[B::AltarOfStorms],
            requires_upgrade: false, sprite_size: 32.0, sprite: (6, 1),
            naval: false, air: false, can_harvest: false, can_build: false,
        },
        // ── Orci – drak ───────────────────────────────────────────────────
        UnitDef {
            kind: DragonRider, name: "Dragon", faction: Orc,
            hp_max: 100,  speed: 384.0, sight: 6,
            attack_damage: 16, pierce_damage: 0, attack_range: 0.0, attack_cooldown: 1.5,
            armor: 5, cost: Resources::new(2500, 0), build_time: 250.0,
            produced_in: &[B::DragonRoost],
            requires_upgrade: false, sprite_size: 40.0, sprite: (7, 1),
            naval: false, air: true, can_harvest: false, can_build: false,
        },
    ]
}

/// Vyhledá definici podle druhu.
pub fn unit_def(kind: UnitKind) -> Option<UnitDef> {
    all_unit_defs().into_iter().find(|d| d.kind == kind)
}

// ─────────────────────────────────────────────────────────────────────────────
// BUDOVY
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    // ── Humanité ──────────────────────────
    TownHall,
    Keep,
    Castle,
    Farm,
    Barracks,
    LumberMill,
    Blacksmith,
    Tower,
    Church,
    Stables,
    MageTower,
    GryphonAviary,
    Shipyard,
    FoundryH,       // Gnomish Inventor
    OilRefinery,
    // ── Orci ──────────────────────────────
    GreatHall,
    Stronghold,
    Fortress,
    PigFarm,
    OrcBarracks,
    TrollLumberMill,
    OrcBlacksmith,
    WatchTower,
    AltarOfStorms,
    OgreMound,
    WarlockTower,
    DragonRoost,
    OrcShipyard,
    FoundryO,
    OilRefineryO,
    // ── Neutrální ─────────────────────────
    GoldMine,
    OilPlatform,
    Runestone,
}

/// Kompletní definice budovy.
#[derive(Clone, Debug)]
pub struct BuildingDef {
    pub kind:            BuildingKind,
    pub name:            &'static str,
    pub faction:         Faction,
    pub hp_max:          i32,
    pub armor:           i32,
    /// Velikost v dlaždicích (n×n).
    pub size_tiles:      u32,
    /// Cena postavení.
    pub cost:            Resources,
    /// Čas stavby (s).
    pub build_time:      f32,
    /// Jaké jednotky tato budova vyrábí.
    pub produces:        &'static [UnitKind],
    /// Jaké budovy musí existovat jako podmínka.
    pub requires:        &'static [BuildingKind],
    /// Poskytuje `n` potravy (farm efekt).
    pub food_provided:   u32,
    /// Pozice ve spritesheet (sloupec, řádek).
    pub sprite:          (u32, u32),
    /// Lze upgradovat na jinou budovu.
    pub upgrades_to:     Option<BuildingKind>,
}

pub fn all_building_defs() -> Vec<BuildingDef> {
    use BuildingKind::*;
    use UnitKind as U;
    use Faction::*;

    vec![
        // ── TownHall ──────────────────────────────────────────────────────
        BuildingDef {
            kind: TownHall, name: "Town Hall", faction: Human,
            hp_max: 1200, armor: 0, size_tiles: 4,
            cost: Resources::new(1200, 800), build_time: 255.0,
            produces: &[U::Peasant],
            requires: &[],
            food_provided: 0, sprite: (0, 2),
            upgrades_to: Some(Keep),
        },
        BuildingDef {
            kind: Keep, name: "Keep", faction: Human,
            hp_max: 1400, armor: 0, size_tiles: 4,
            cost: Resources::new(2000, 1000), build_time: 200.0,
            produces: &[U::Peasant],
            requires: &[Barracks, LumberMill],
            food_provided: 0, sprite: (1, 2),
            upgrades_to: Some(Castle),
        },
        BuildingDef {
            kind: Castle, name: "Castle", faction: Human,
            hp_max: 1600, armor: 0, size_tiles: 4,
            cost: Resources::new(2500, 1200), build_time: 200.0,
            produces: &[U::Peasant],
            requires: &[Church, Stables, MageTower],
            food_provided: 0, sprite: (2, 2),
            upgrades_to: None,
        },
        BuildingDef {
            kind: Farm, name: "Farm", faction: Human,
            hp_max: 400, armor: 0, size_tiles: 2,
            cost: Resources::new(500, 250), build_time: 100.0,
            produces: &[], requires: &[],
            food_provided: 4, sprite: (3, 2),
            upgrades_to: None,
        },
        BuildingDef {
            kind: Barracks, name: "Barracks", faction: Human,
            hp_max: 800, armor: 0, size_tiles: 3,
            cost: Resources::new(700, 450), build_time: 200.0,
            produces: &[U::Footman, U::Archer, U::Ballista],
            requires: &[Farm],
            food_provided: 0, sprite: (4, 2),
            upgrades_to: None,
        },
        BuildingDef {
            kind: LumberMill, name: "Lumber Mill", faction: Human,
            hp_max: 600, armor: 0, size_tiles: 3,
            cost: Resources::new(600, 450), build_time: 150.0,
            produces: &[], requires: &[Barracks],
            food_provided: 0, sprite: (5, 2),
            upgrades_to: None,
        },
        BuildingDef {
            kind: Blacksmith, name: "Blacksmith", faction: Human,
            hp_max: 775, armor: 0, size_tiles: 3,
            cost: Resources::new(800, 450), build_time: 200.0,
            produces: &[U::Ballista], requires: &[LumberMill],
            food_provided: 0, sprite: (6, 2),
            upgrades_to: None,
        },
        BuildingDef {
            kind: Tower, name: "Guard Tower", faction: Human,
            hp_max: 130, armor: 2, size_tiles: 1,
            cost: Resources::new(550, 200), build_time: 140.0,
            produces: &[], requires: &[Barracks],
            food_provided: 0, sprite: (7, 2),
            upgrades_to: None,
        },
        BuildingDef {
            kind: Stables, name: "Stables", faction: Human,
            hp_max: 600, armor: 0, size_tiles: 3,
            cost: Resources::new(1000, 300), build_time: 150.0,
            produces: &[U::Knight],
            requires: &[Keep],
            food_provided: 0, sprite: (0, 3),
            upgrades_to: None,
        },
        BuildingDef {
            kind: Church, name: "Church", faction: Human,
            hp_max: 700, armor: 0, size_tiles: 3,
            cost: Resources::new(900, 500), build_time: 175.0,
            produces: &[],  // research upgrades here
            requires: &[Keep],
            food_provided: 0, sprite: (1, 3),
            upgrades_to: None,
        },
        BuildingDef {
            kind: MageTower, name: "Mage Tower", faction: Human,
            hp_max: 500, armor: 0, size_tiles: 2,
            cost: Resources::new(1000, 200), build_time: 125.0,
            produces: &[U::Mage], requires: &[Keep],
            food_provided: 0, sprite: (2, 3),
            upgrades_to: None,
        },
        BuildingDef {
            kind: GryphonAviary, name: "Gryphon Aviary", faction: Human,
            hp_max: 500, armor: 0, size_tiles: 2,
            cost: Resources::new(1000, 400), build_time: 150.0,
            produces: &[U::GryphonRider], requires: &[Castle],
            food_provided: 0, sprite: (3, 3),
            upgrades_to: None,
        },
        // ── Orcí ──────────────────────────────────────────────────────────
        BuildingDef {
            kind: GreatHall, name: "Great Hall", faction: Orc,
            hp_max: 1200, armor: 0, size_tiles: 4,
            cost: Resources::new(1200, 800), build_time: 255.0,
            produces: &[U::Peon], requires: &[],
            food_provided: 0, sprite: (4, 3),
            upgrades_to: Some(Stronghold),
        },
        BuildingDef {
            kind: Stronghold, name: "Stronghold", faction: Orc,
            hp_max: 1400, armor: 0, size_tiles: 4,
            cost: Resources::new(2000, 1000), build_time: 200.0,
            produces: &[U::Peon],
            requires: &[OrcBarracks, TrollLumberMill],
            food_provided: 0, sprite: (5, 3),
            upgrades_to: Some(Fortress),
        },
        BuildingDef {
            kind: Fortress, name: "Fortress", faction: Orc,
            hp_max: 1600, armor: 0, size_tiles: 4,
            cost: Resources::new(2500, 1200), build_time: 200.0,
            produces: &[U::Peon],
            requires: &[AltarOfStorms, OgreMound, WarlockTower],
            food_provided: 0, sprite: (6, 3),
            upgrades_to: None,
        },
        BuildingDef {
            kind: PigFarm, name: "Pig Farm", faction: Orc,
            hp_max: 400, armor: 0, size_tiles: 2,
            cost: Resources::new(500, 250), build_time: 100.0,
            produces: &[], requires: &[],
            food_provided: 4, sprite: (7, 3),
            upgrades_to: None,
        },
        BuildingDef {
            kind: OrcBarracks, name: "Orc Barracks", faction: Orc,
            hp_max: 800, armor: 0, size_tiles: 3,
            cost: Resources::new(700, 450), build_time: 200.0,
            produces: &[U::Grunt, U::TrollAxethrower, U::Catapult],
            requires: &[PigFarm],
            food_provided: 0, sprite: (0, 4),
            upgrades_to: None,
        },
        BuildingDef {
            kind: TrollLumberMill, name: "Troll Lumber Mill", faction: Orc,
            hp_max: 600, armor: 0, size_tiles: 3,
            cost: Resources::new(600, 450), build_time: 150.0,
            produces: &[], requires: &[OrcBarracks],
            food_provided: 0, sprite: (1, 4),
            upgrades_to: None,
        },
        BuildingDef {
            kind: OrcBlacksmith, name: "Orc Blacksmith", faction: Orc,
            hp_max: 775, armor: 0, size_tiles: 3,
            cost: Resources::new(800, 450), build_time: 200.0,
            produces: &[U::Catapult], requires: &[TrollLumberMill],
            food_provided: 0, sprite: (2, 4),
            upgrades_to: None,
        },
        BuildingDef {
            kind: WatchTower, name: "Watch Tower", faction: Orc,
            hp_max: 130, armor: 2, size_tiles: 1,
            cost: Resources::new(550, 200), build_time: 140.0,
            produces: &[], requires: &[OrcBarracks],
            food_provided: 0, sprite: (3, 4),
            upgrades_to: None,
        },
        BuildingDef {
            kind: OgreMound, name: "Ogre Mound", faction: Orc,
            hp_max: 600, armor: 0, size_tiles: 3,
            cost: Resources::new(1000, 300), build_time: 150.0,
            produces: &[U::OgreKnight], requires: &[Stronghold],
            food_provided: 0, sprite: (4, 4),
            upgrades_to: None,
        },
        BuildingDef {
            kind: AltarOfStorms, name: "Altar of Storms", faction: Orc,
            hp_max: 700, armor: 0, size_tiles: 3,
            cost: Resources::new(900, 500), build_time: 175.0,
            produces: &[U::DeathKnight], requires: &[Stronghold],
            food_provided: 0, sprite: (5, 4),
            upgrades_to: None,
        },
        BuildingDef {
            kind: WarlockTower, name: "Temple of the Damned", faction: Orc,
            hp_max: 500, armor: 0, size_tiles: 2,
            cost: Resources::new(1000, 200), build_time: 125.0,
            produces: &[], requires: &[Stronghold],
            food_provided: 0, sprite: (6, 4),
            upgrades_to: None,
        },
        BuildingDef {
            kind: DragonRoost, name: "Dragon Roost", faction: Orc,
            hp_max: 500, armor: 0, size_tiles: 2,
            cost: Resources::new(1000, 400), build_time: 150.0,
            produces: &[U::DragonRider], requires: &[Fortress],
            food_provided: 0, sprite: (7, 4),
            upgrades_to: None,
        },
        // ── Neutrální ─────────────────────────────────────────────────────
        BuildingDef {
            kind: GoldMine, name: "Gold Mine", faction: Neutral,
            hp_max: 25500, armor: 0, size_tiles: 3,
            cost: Resources::ZERO, build_time: 0.0,
            produces: &[], requires: &[],
            food_provided: 0, sprite: (0, 5),
            upgrades_to: None,
        },
    ]
}

pub fn building_def(kind: BuildingKind) -> Option<BuildingDef> {
    all_building_defs().into_iter().find(|d| d.kind == kind)
}
