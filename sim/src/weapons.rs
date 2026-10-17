// Pattern: Type Object. A "weapon type" is a row of data in a table, not a
// Rust type. The three weapons below behave differently only because their
// numbers differ. Adding the other twelve weapons later is mostly adding rows
// (plus a little code for the genuinely new mechanics like gravity wells).
//
// Pattern: Flyweight. A projectile in flight stores just a weapon id and its
// own position/velocity. The shared, unchanging stats live once in `WEAPONS`.

pub const BLASTER: u8 = 0;
pub const ROCKET: u8 = 1;
pub const RAILGUN: u8 = 2;

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
}

pub static WEAPONS: [WeaponDef; 3] = [
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
    },
];
