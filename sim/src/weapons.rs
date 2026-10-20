// Pattern: Type Object. A "weapon type" is a row of data in a table, not a
// Rust type. The weapons below behave differently mostly because their
// numbers differ; the few genuinely new mechanics (a swarm's several missiles,
// a ricochet) are one flag or number each, read in
// `world.rs`. More weapons later means more rows, and a little code only for
// mechanics that are more than a number (`Special`: homing, a cluster burst, a
// teleport, and later things like gravity wells).
//
// Pattern: Flyweight. A projectile in flight stores just a weapon id and its
// own position/velocity. The shared, unchanging stats live once in `WEAPONS`.

use crate::rng::Rng;

pub const BLASTER: u8 = 0;
pub const ROCKET: u8 = 1;
pub const RAILGUN: u8 = 2;
/// Not a gun: places a brick. It took the shotgun's slot (the Scatter was the
/// least interesting weapon), so building costs a loadout pick like any weapon.
pub const BUILD: u8 = 3;
pub const BOUNCER: u8 = 4;
pub const BOMB: u8 = 5;
pub const CLUSTER: u8 = 6;
pub const SWARM: u8 = 7;
pub const TELEPORT: u8 = 8;
pub const NOVA: u8 = 9;
/// What a Cluster shell splits into. Not in the loadout: only the shell makes it.
pub const BOMBLET: u8 = 10;
/// The "gadgets": they do no damage, they change the fight.
pub const SHEEP: u8 = 11;
pub const CONFUSER: u8 = 12;
pub const MIRROR: u8 = 13;
pub const OIL: u8 = 14;
pub const LASER: u8 = 15;
pub const BLACKHOLE: u8 = 16;
pub const FLASH: u8 = 17;

/// What a weapon is for. A loadout may hold at most `MAX_PER_CLASS` of each, so
/// five picks always mix at least three of the four: nobody can carry only guns,
/// or only the strongest weapons of every kind.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Class {
    /// Aimed damage from a distance.
    Gun,
    /// Area damage: they dig and they blast.
    Explosive,
    /// They change the map or your place on it.
    Tool,
    /// They do no damage: they change the fight.
    Trick,
}

pub const CLASSES: [Class; 4] = [Class::Gun, Class::Explosive, Class::Tool, Class::Trick];

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Gun => "GUNS",
            Class::Explosive => "EXPLOSIVES",
            Class::Tool => "TOOLS",
            Class::Trick => "TRICKS",
        }
    }
}

/// Which class a weapon belongs to. (The weapons nobody can pick count as guns.)
pub fn class_of(id: u8) -> Class {
    match id {
        ROCKET | BOMB | CLUSTER | NOVA => Class::Explosive,
        BUILD | TELEPORT | MIRROR | OIL => Class::Tool,
        SHEEP | CONFUSER | BLACKHOLE | FLASH => Class::Trick,
        _ => Class::Gun,
    }
}

pub const MAX_PER_CLASS: usize = 2;

/// Every weapon a player can choose from, class by class (the loadout screen
/// shows one column per class, in this order). The menu's shooting range hands
/// out all of them.
pub const ARSENAL: [u8; 17] = [
    BLASTER, BOUNCER, RAILGUN, SWARM, LASER, //
    ROCKET, BOMB, CLUSTER, NOVA, //
    BUILD, TELEPORT, MIRROR, OIL, //
    SHEEP, CONFUSER, BLACKHOLE, FLASH,
];

/// How many weapons a player carries into a match, picked from the arsenal
/// before it starts (like Liero's weapon menu). Keys 1-5 select them.
pub const LOADOUT_SIZE: usize = 5;

/// What the picker starts with.
pub const DEFAULT_LOADOUT: [u8; LOADOUT_SIZE] = [BLASTER, RAILGUN, ROCKET, BOMB, BUILD];

/// What bots carry. It breaks the class rule (three guns) on purpose: the rule
/// is there to make people's choices varied, and the bots only know how to
/// shoot with these four (see `bots.rs`).
pub const BOT_LOADOUT: [u8; LOADOUT_SIZE] = [BLASTER, BOUNCER, ROCKET, BOMB, RAILGUN];

/// May `id` join the weapons already `picked`? It must not be there already,
/// and its class must have room. One rule for the loadout screen, the random
/// roll and the host's check.
pub fn can_add(picked: &[u8], id: u8) -> bool {
    ARSENAL.contains(&id)
        && !picked.contains(&id)
        && picked.len() < LOADOUT_SIZE
        && picked.iter().filter(|&&p| class_of(p) == class_of(id)).count() < MAX_PER_CLASS
}

/// Is this an acceptable choice: the right number of different arsenal weapons,
/// within the class limits? A loadout arrives from another player's machine, and
/// nothing that comes from outside is trusted, so the host checks it before it
/// touches the world.
pub fn valid_loadout(ids: &[u8]) -> bool {
    ids.len() == LOADOUT_SIZE && ids.iter().enumerate().all(|(i, &id)| can_add(&ids[..i], id))
}

/// Five random weapons that obey the class limits: for players who would rather
/// not pick the "best" ones. Shuffle the arsenal, then take weapons from the top
/// while they fit. (The sim's own `Rng`, so a seed always rolls the same set.)
pub fn random_loadout(rng: &mut Rng) -> Vec<u8> {
    let mut deck = ARSENAL;
    for i in (1..deck.len()).rev() {
        deck.swap(i, rng.next_u32() as usize % (i + 1));
    }
    let mut picked = Vec::with_capacity(LOADOUT_SIZE);
    for id in deck {
        if can_add(&picked, id) {
            picked.push(id);
        }
    }
    picked
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// Flies through the world as a `Projectile`.
    Projectile,
    /// Instant line; resolved the moment you pull the trigger.
    Beam,
    /// Puts a brick on the first free tile along the aim, within `range`.
    /// The magazine is the number of blocks you carry.
    Build,
    /// Plants a `Deployable` (see `Special::Mirror`) `range` px in front of you.
    Deploy,
    /// An instant cone, `range` long, in front of you (see `Special::Stun`).
    Flash,
}

/// The few behaviours that are more than a number. A weapon has at most one, so
/// this is an enum and not another pile of flags.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Special {
    None,
    /// Steers towards the nearest enemy, turning at most this many radians a second.
    Homing(f32),
    /// Where it detonates it splits into this many projectiles of the given weapon.
    Cluster(u8, u8),
    /// Where it lands, its owner appears (or swaps places with the player it hit).
    Teleport,
    /// A player it hits turns into a sheep for this many seconds: small, fast
    /// and unable to shoot.
    Sheep(f32),
    /// A player it hits sees the world mirrored for this many seconds. The sim
    /// only keeps the timer; mirroring is the client's picture (see `Player::confused`).
    Confuse(f32),
    /// A pane (a `Deployable`) of this half-width in px that lasts this many
    /// seconds. Enemy projectiles that cross it fly back at their shooter.
    Mirror(f32, f32),
    /// Where it lands, an oil puddle of this radius in px spreads and stays this
    /// many seconds. Players in it cannot steer or dash, and take damage (for
    /// whoever laid it) when they slide into a wall.
    Slick(f32, f32),
    /// Where it lands, a black hole of this radius in px opens for this many
    /// seconds and pulls every player in reach towards its centre.
    Well(f32, f32),
    /// Stuns every player in the cone for this many seconds: they cannot move,
    /// dash or shoot. The second number is the cone's half-angle in radians.
    Stun(f32, f32),
}

pub struct WeaponDef {
    pub name: &'static str,
    pub kind: Kind,
    /// Damage on a direct hit (or to each player a beam passes through).
    pub damage: f32,
    /// Projectile speed in px/s. Unused by beams. A speed of 0 means the
    /// projectile is dropped at the player's feet, and `lifetime` is its fuse.
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
    /// Beam length, including its bounces (or how far away you can build, or a
    /// flash reaches) in px. Unused by projectiles.
    pub range: f32,
    /// Explosion size in px; 0 means the weapon does not explode.
    pub blast_radius: f32,
    pub blast_damage: f32,
    /// Speed change (px/s) given to a player at the centre of a blast / on a hit.
    pub knockback: f32,
    /// How much a brick is hurt by this weapon (bricks have 3 hp). Beams
    /// ignore it: they stop at the first brick (see `fire_beam`).
    pub tile_damage: u8,
    /// Projectiles per shot (the swarm fires several, each with its own spread).
    pub pellets: u8,
    /// Does the projectile (or beam) ricochet off walls instead of stopping at
    /// them? A beam still ends at a brick.
    pub bounce: bool,
    pub special: Special,
}

pub static WEAPONS: [WeaponDef; 18] = [
    WeaponDef {
        name: "Neon Blaster",
        kind: Kind::Projectile,
        // Balance: was 12 damage and 1.2 s (840 px), the best gun at every
        // range. Now it is the close-range gun, and the long range is the
        // railgun's.
        damage: 10.0,
        speed: 700.0,
        lifetime: 0.9,
        cooldown: 0.12,
        mag: 20,
        reload: 1.2,
        spread: 0.04,
        range: 0.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 60.0,
        // Chips bricks: a brick has 3 hp, so it takes three hits. Slow on
        // purpose, the rocket and bomb are the fast way through.
        tile_damage: 1,
        pellets: 1,
        bounce: false,
        special: Special::None,
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
        pellets: 1,
        bounce: false,
        special: Special::None,
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
        // Balance: the rail once cut two bricks, which made a brick no cover
        // from an instant beam. Now it stops at the first brick and destroys
        // just that one: a block is cover from one shot, then it is gone.
        tile_damage: 3,
        pellets: 1,
        bounce: false,
        special: Special::None,
    },
    WeaponDef {
        name: "Block",
        kind: Kind::Build,
        damage: 0.0,
        speed: 0.0,
        lifetime: 0.0,
        cooldown: 0.08, // fast enough to throw up a wall in one sweep of the aim
        mag: 8,
        reload: 1.5,
        spread: 0.0,
        range: 96.0, // three tiles
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 0.0,
        tile_damage: 0,
        pellets: 1,
        bounce: false,
        special: Special::None,
    },
    WeaponDef {
        name: "Bouncer",
        kind: Kind::Projectile,
        damage: 25.0, // four hits kill (22 needed five, and lost to the blaster)
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
        pellets: 1,
        bounce: true,
        special: Special::None,
    },
    WeaponDef {
        name: "Bomb",
        kind: Kind::Projectile,
        damage: 0.0,
        // Dropped at your feet. It also goes off the moment an enemy touches
        // it, so it doubles as a mine.
        // Balance: it used to be a Bomberman plus, 75 damage, 1.8 s fuse:
        // standing diagonally from it was perfectly safe, and anyone who walked
        // off within 1.4 s took nothing. Now it is round, with the same reach as
        // the plus had (3.5 tiles) in every direction, and a shorter fuse: you
        // must react within about half a second.
        speed: 0.0,
        lifetime: 1.2,
        cooldown: 0.8,
        mag: 2,
        reload: 3.0,
        spread: 0.0,
        range: 0.0,
        blast_radius: 112.0,
        blast_damage: 120.0,
        knockback: 450.0,
        tile_damage: 3,
        pellets: 1,
        bounce: false,
        special: Special::None,
    },
    // ---- the second batch ---------------------------------------------------
    WeaponDef {
        name: "Cluster",
        kind: Kind::Projectile,
        damage: 0.0,
        speed: 360.0,
        lifetime: 0.8, // bursts about 290 px out if nothing stops it first
        cooldown: 0.9,
        mag: 3,
        reload: 3.0,
        spread: 0.02,
        range: 0.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 0.0,
        tile_damage: 0,
        pellets: 1,
        bounce: false,
        special: Special::Cluster(7, BOMBLET),
    },
    WeaponDef {
        name: "Swarm",
        kind: Kind::Projectile,
        damage: 0.0,
        speed: 300.0,
        lifetime: 2.2,
        cooldown: 1.0,
        mag: 2,
        reload: 3.2,
        spread: 0.6, // a wide fan, which then curls in on the target
        range: 0.0,
        blast_radius: 30.0,
        blast_damage: 14.0,
        knockback: 150.0,
        tile_damage: 1,
        pellets: 6,
        bounce: false,
        special: Special::Homing(3.0),
    },
    WeaponDef {
        name: "Teleport",
        kind: Kind::Projectile,
        damage: 0.0,
        speed: 520.0,
        lifetime: 1.2,
        cooldown: 0.6,
        mag: 3,
        reload: 2.5,
        spread: 0.0,
        range: 0.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 0.0,
        tile_damage: 0,
        pellets: 1,
        bounce: false,
        special: Special::Teleport,
    },
    WeaponDef {
        name: "Nova",
        kind: Kind::Projectile,
        damage: 0.0,
        speed: 240.0,
        lifetime: 5.0,
        cooldown: 1.0,
        mag: 1,
        reload: 8.0,
        spread: 0.0,
        range: 0.0,
        blast_radius: 230.0, // seven tiles: it clears a room
        blast_damage: 140.0,
        knockback: 1000.0,
        tile_damage: 3,
        pellets: 1,
        bounce: false,
        special: Special::None,
    },
    // Not in anyone's loadout: what the Cluster shell splits into.
    WeaponDef {
        name: "Bomblet",
        kind: Kind::Projectile,
        damage: 0.0,
        speed: 240.0,
        lifetime: 0.55,
        cooldown: 0.0,
        mag: 0,
        reload: 0.0,
        spread: 0.0,
        range: 0.0,
        blast_radius: 40.0,
        blast_damage: 24.0,
        knockback: 300.0,
        tile_damage: 2,
        pellets: 1,
        bounce: false,
        special: Special::None,
    },
    // ---- the gadgets: no damage, they change the fight -------------------------
    WeaponDef {
        name: "Sheep Ray",
        kind: Kind::Projectile,
        damage: 0.0,
        speed: 560.0,
        lifetime: 1.0,
        cooldown: 0.6,
        mag: 2,
        reload: 4.0, // a sheep cannot shoot for 12 s: long enough to run, hide or hunt it down
        spread: 0.0,
        range: 0.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 0.0,
        tile_damage: 0,
        pellets: 1,
        bounce: false,
        special: Special::Sheep(12.0),
    },
    WeaponDef {
        name: "Confuser",
        kind: Kind::Projectile,
        damage: 0.0,
        speed: 560.0,
        lifetime: 1.0,
        cooldown: 0.5,
        mag: 3,
        reload: 3.0,
        spread: 0.0,
        range: 0.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 0.0,
        tile_damage: 0,
        pellets: 1,
        bounce: false,
        special: Special::Confuse(4.0),
    },
    WeaponDef {
        name: "Mirror Pane",
        kind: Kind::Deploy,
        damage: 0.0,
        speed: 0.0,
        lifetime: 0.0,
        cooldown: 0.5,
        mag: 2,
        reload: 4.0,
        spread: 0.0,
        range: 30.0, // how far in front of you the pane stands
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 0.0,
        tile_damage: 0,
        pellets: 1,
        bounce: false,
        special: Special::Mirror(36.0, 6.0),
    },
    WeaponDef {
        name: "Oil Slick",
        kind: Kind::Projectile,
        damage: 0.0,
        speed: 380.0,
        lifetime: 0.7, // lands about 270 px out if nothing stops it first
        cooldown: 0.7,
        mag: 2,
        reload: 4.0,
        spread: 0.0,
        range: 0.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 0.0,
        tile_damage: 0,
        pellets: 1,
        bounce: false,
        special: Special::Slick(100.0, 10.0),
    },
    // ---- the second set of gadgets ---------------------------------------------
    WeaponDef {
        name: "Bounce Laser",
        kind: Kind::Beam,
        // Hold the trigger: a pulse every 3 ticks (20 a second) makes a steady
        // beam, and the magazine is its battery: 60 pulses, three seconds.
        // Nothing here is a new mechanic apart from `bounce`: it is the railgun's
        // beam, fired very often, that turns at walls.
        damage: 3.0, // per pulse: 60 a second, about half a blaster's, but it turns corners
        speed: 0.0,
        lifetime: 0.0,
        cooldown: 0.045,
        mag: 60,
        reload: 3.5,
        spread: 0.0,
        range: 1100.0, // the whole path, over all its bounces
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 15.0,
        tile_damage: 0, // bricks stop it and are left whole
        pellets: 1,
        bounce: true,
        special: Special::None,
    },
    WeaponDef {
        name: "Black Hole",
        kind: Kind::Projectile,
        damage: 0.0,
        speed: 300.0,
        lifetime: 1.0,
        cooldown: 1.0,
        mag: 1,
        reload: 9.0, // five seconds of holding players still: very slow to come back
        spread: 0.0,
        range: 0.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 0.0,
        tile_damage: 0,
        pellets: 1,
        bounce: false,
        special: Special::Well(140.0, 5.0),
    },
    WeaponDef {
        name: "Flashgun",
        kind: Kind::Flash,
        damage: 0.0,
        speed: 0.0,
        lifetime: 0.0,
        cooldown: 0.8,
        mag: 2,
        reload: 4.0,
        spread: 0.0,
        range: 340.0,
        blast_radius: 0.0,
        blast_damage: 0.0,
        knockback: 0.0,
        tile_damage: 0,
        pellets: 1,
        bounce: false,
        special: Special::Stun(1.3, 0.5),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arsenal_is_listed_class_by_class() {
        // The loadout screen draws one column per class, straight from this order.
        let classes: Vec<Class> = ARSENAL.iter().map(|&id| class_of(id)).collect();
        let order: Vec<Class> = CLASSES.iter().flat_map(|&c| classes.iter().filter(move |&&x| x == c).copied()).collect();
        assert_eq!(classes, order);
        assert!(ARSENAL.iter().all(|&id| WEAPONS[id as usize].mag > 0), "only real weapons can be picked");
    }

    #[test]
    fn a_loadout_is_five_different_weapons_with_room_in_their_class() {
        assert!(valid_loadout(&DEFAULT_LOADOUT));
        assert!(!valid_loadout(&[BLASTER, ROCKET]), "too few");
        assert!(!valid_loadout(&[0, 1, 2, 3, 4, 5]), "too many");
        assert!(!valid_loadout(&[BLASTER, BLASTER, ROCKET, BOMB, RAILGUN]), "a weapon twice");
        assert!(!valid_loadout(&[BLASTER, ROCKET, BOMB, RAILGUN, BOMBLET]), "bomblets are not for picking");
        assert!(!valid_loadout(&[BLASTER, ROCKET, BOMB, RAILGUN, 200]), "no such weapon");
        assert!(!valid_loadout(&BOT_LOADOUT), "three guns: only bots may");
        assert!(!valid_loadout(&[ROCKET, BOMB, CLUSTER, BLASTER, BUILD]), "three explosives");
    }

    #[test]
    fn a_random_loadout_is_always_a_valid_one_and_varies() {
        let mut rng = Rng::new(7);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let picked = random_loadout(&mut rng);
            assert!(valid_loadout(&picked), "{picked:?}");
            seen.extend(picked);
        }
        assert_eq!(seen.len(), ARSENAL.len(), "every weapon turns up sometime");
    }
}
