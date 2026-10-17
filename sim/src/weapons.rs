// Pattern: Type Object. A "weapon type" is a row of data in a table, not a
// Rust type. The weapons below behave differently mostly because their
// numbers differ; the few genuinely new mechanics (a shotgun's several pellets,
// a ricochet, a plus-shaped blast) are one flag or number each, read in
// `world.rs`. More weapons later means more rows, and a little code only for
// mechanics like gravity wells.
//
// Pattern: Flyweight. A projectile in flight stores just a weapon id and its
// own position/velocity. The shared, unchanging stats live once in `WEAPONS`.

pub const BLASTER: u8 = 0;
pub const ROCKET: u8 = 1;
pub const RAILGUN: u8 = 2;
pub const SCATTER: u8 = 3;
pub const BOUNCER: u8 = 4;
pub const BOMB: u8 = 5;

/// What every player carries, in slot order (keys 1-6). A per-player loadout
/// comes with the loadout screen in M4.
pub const LOADOUT: [u8; 6] = [BLASTER, SCATTER, ROCKET, BOUNCER, BOMB, RAILGUN];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// Flies through the world as a `Projectile`.
    Projectile,
    /// Instant line; resolved the moment you pull the trigger.
    Beam,
}

pub struct WeaponDef {
    pub name: &'static str,
    pub kind: Kind,
    /// Damage on a direct hit (or to each player a beam passes through).
    pub damage: f32,
    /// Projectile speed in px/s. Unused by beams.
    pub speed: f32,
    /// Seconds before an unspent projectile disappears (or explodes).
    pub lifetime: f32,
    /// Seconds between shots.
    pub cooldown: f32,
    /// Rounds per magazine, and seconds to reload once it is empty.
    pub mag: u8,
    pub reload: f32,
    /// Random aim error in radians (each side).
    pub spread: f32,
    /// Beam length in px. Unused by projectiles.
    pub range: f32,
    /// Explosion size in px; 0 means the weapon does not explode.
    pub blast_radius: f32,
    pub blast_damage: f32,
    /// Speed change (px/s) given to a player at the centre of a blast / on a hit.
    pub knockback: f32,
    /// How much a brick is hurt by this weapon (bricks have 3 hp).
    pub tile_damage: u8,
    /// How many bricks a beam can cut through before it stops.
    pub beam_bricks: u8,
    /// Projectiles per shot (a shotgun fires many, each with its own spread).
    pub pellets: u8,
    /// Does the projectile ricochet off walls instead of stopping at them?
    pub bounce: bool,
    /// Is the blast a plus shape along the tile grid (Bomberman) instead of a
    /// circle? Then `blast_radius` is the arm length. A speed of 0 means the
    /// projectile is dropped at the player's feet, and `lifetime` is its fuse.
    pub cross: bool,
}

pub static WEAPONS: [WeaponDef; 6] = [
    WeaponDef {
        name: "Neon Blaster",
        kind: Kind::Projectile,
        damage: 12.0,
        speed: 700.0,
        lifetime: 1.2,
        cooldown: 0.12,
        mag: 20,
        reload: 1.2,
        spread: 0.04,
        range: 0.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 60.0,
        tile_damage: 0,
        beam_bricks: 0,
        pellets: 1,
        bounce: false,
        cross: false,
    },
    WeaponDef {
        name: "Rocket",
        kind: Kind::Projectile,
        damage: 0.0,
        speed: 420.0,
        lifetime: 3.0,
        cooldown: 0.7,
        mag: 4,
        reload: 2.2,
        spread: 0.0,
        range: 0.0,
        blast_radius: 72.0,
        blast_damage: 60.0,
        knockback: 650.0,
        tile_damage: 3,
        beam_bricks: 0,
        pellets: 1,
        bounce: false,
        cross: false,
    },
    WeaponDef {
        name: "Railgun",
        kind: Kind::Beam,
        damage: 70.0,
        speed: 0.0,
        lifetime: 0.0,
        cooldown: 0.5,
        mag: 1,
        reload: 2.5,
        spread: 0.0,
        range: 1500.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 350.0,
        tile_damage: 3,
        beam_bricks: 2,
        pellets: 1,
        bounce: false,
        cross: false,
    },
    WeaponDef {
        name: "Scatter",
        kind: Kind::Projectile,
        damage: 9.0,
        speed: 650.0,
        lifetime: 0.35, // short life = short range (~230 px)
        cooldown: 0.8,
        mag: 6,
        reload: 1.8,
        spread: 0.2,
        range: 0.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 45.0,
        tile_damage: 1,
        beam_bricks: 0,
        pellets: 8,
        bounce: false,
        cross: false,
    },
    WeaponDef {
        name: "Bouncer",
        kind: Kind::Projectile,
        damage: 22.0,
        speed: 520.0,
        lifetime: 3.0,
        cooldown: 0.35,
        mag: 8,
        reload: 1.6,
        spread: 0.02,
        range: 0.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 120.0,
        tile_damage: 0,
        beam_bricks: 0,
        pellets: 1,
        bounce: true,
        cross: false,
    },
    WeaponDef {
        name: "Bomb",
        kind: Kind::Projectile,
        damage: 0.0,
        speed: 0.0,
        lifetime: 1.8,
        cooldown: 0.8,
        mag: 2,
        reload: 3.0,
        spread: 0.0,
        range: 0.0,
        blast_radius: 96.0, // three tiles per arm
        blast_damage: 75.0,
        knockback: 450.0,
        tile_damage: 3,
        beam_bricks: 0,
        pellets: 1,
        bounce: false,
        cross: true,
    },
];
