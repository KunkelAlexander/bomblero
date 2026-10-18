// The terrain: a grid of square tiles.
//
// Pattern: Spatial Partition. The grid IS the partition. To ask "what does this
// circle touch?" we look up the 4 or so tiles under it instead of testing
// against every wall. It also makes destruction trivial (change one cell) and
// networking cheap (a changed tile is just `(x, y, tile)`).

use glam::{vec2, Vec2};
use serde::{Deserialize, Serialize};

pub const TILE: f32 = 32.0;
pub const MAP_W: usize = 40;
pub const MAP_H: usize = 22;
pub const BRICK_HP: u8 = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Tile {
    Empty,
    /// Indestructible wall.
    Solid,
    /// Destructible wall. The number is its remaining hit points.
    Brick(u8),
}

impl Tile {
    /// Does this tile stop players and projectiles?
    pub fn blocks(self) -> bool {
        self != Tile::Empty
    }
}

pub struct Tilemap {
    tiles: Vec<Tile>,
    /// Where players may (re)appear. Read from the 'S' characters in the map.
    pub spawns: Vec<Vec2>,
}

impl Tilemap {
    /// Build a map from ASCII art. '#' solid, 'b' brick, '.' empty, 'S' spawn.
    /// Maps live in the source as text so they are easy to read and edit, and
    /// we need no asset files.
    pub fn from_ascii(art: &str) -> Tilemap {
        let rows: Vec<&str> = art.lines().collect();
        assert_eq!(rows.len(), MAP_H, "map must have {MAP_H} rows");
        let mut tiles = Vec::with_capacity(MAP_W * MAP_H);
        let mut spawns = Vec::new();
        for (y, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), MAP_W, "map row {y} must be {MAP_W} wide");
            for (x, ch) in row.chars().enumerate() {
                tiles.push(match ch {
                    '#' => Tile::Solid,
                    'b' => Tile::Brick(BRICK_HP),
                    'S' => {
                        spawns.push(tile_center(x as i32, y as i32));
                        Tile::Empty
                    }
                    _ => Tile::Empty,
                });
            }
        }
        Tilemap { tiles, spawns }
    }

    /// Anything outside the map counts as solid, so nothing can ever leave it
    /// and callers never need bounds checks.
    pub fn get(&self, tx: i32, ty: i32) -> Tile {
        if tx < 0 || ty < 0 || tx >= MAP_W as i32 || ty >= MAP_H as i32 {
            return Tile::Solid;
        }
        self.tiles[ty as usize * MAP_W + tx as usize]
    }

    pub fn set(&mut self, tx: i32, ty: i32, tile: Tile) {
        if tx >= 0 && ty >= 0 && tx < MAP_W as i32 && ty < MAP_H as i32 {
            self.tiles[ty as usize * MAP_W + tx as usize] = tile;
        }
    }

    /// The whole grid, row by row. A player who joins mid-match gets this once;
    /// after that only `TileChanged` events are sent.
    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }

    /// Overwrite the whole grid (the receiving end of `tiles`).
    /// Ignores a slice of the wrong size rather than trusting the sender.
    pub fn load_tiles(&mut self, tiles: &[Tile]) {
        if tiles.len() == self.tiles.len() {
            self.tiles.copy_from_slice(tiles);
        }
    }

    pub fn tile_at(&self, p: Vec2) -> Tile {
        let (tx, ty) = tile_coords(p);
        self.get(tx, ty)
    }

    /// Hurt a brick. Returns the tile's new value if something changed, so the
    /// caller can announce it (the sim turns that into a `TileChanged` event).
    pub fn damage(&mut self, tx: i32, ty: i32, amount: u8) -> Option<Tile> {
        if let Tile::Brick(hp) = self.get(tx, ty) {
            let new = if hp > amount { Tile::Brick(hp - amount) } else { Tile::Empty };
            self.set(tx, ty, new);
            Some(new)
        } else {
            None
        }
    }

    /// Does a circle overlap any blocking tile?
    pub fn circle_hits_wall(&self, c: Vec2, r: f32) -> bool {
        let (x0, y0) = tile_coords(c - Vec2::splat(r));
        let (x1, y1) = tile_coords(c + Vec2::splat(r));
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                if self.get(tx, ty).blocks() {
                    // Nearest point of the tile's square to the circle centre.
                    let min = vec2(tx as f32 * TILE, ty as f32 * TILE);
                    let nearest = c.clamp(min, min + Vec2::splat(TILE));
                    if nearest.distance_squared(c) < r * r {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Distance from `from` along `dir` (unit vector) to the first blocking
    /// tile, or `max` if nothing is in the way.
    pub fn distance_to_wall(&self, from: Vec2, dir: Vec2, max: f32) -> f32 {
        let mut d = 0.0;
        while d < max {
            if self.tile_at(from + dir * d).blocks() {
                return d;
            }
            d += 4.0;
        }
        max
    }

    /// Can `a` see `b`? Marches along the segment in small steps.
    /// Why marching and not an exact grid walk: the tiles are 32 px and we step
    /// 4 px, so it can only miss a corner graze, and it is much easier to read.
    pub fn line_of_sight(&self, a: Vec2, b: Vec2) -> bool {
        let d = b - a;
        let steps = (d.length() / 4.0).ceil().max(1.0) as i32;
        (1..steps).all(|i| !self.tile_at(a + d * (i as f32 / steps as f32)).blocks())
    }
}

pub fn tile_coords(p: Vec2) -> (i32, i32) {
    ((p.x / TILE).floor() as i32, (p.y / TILE).floor() as i32)
}

pub fn tile_center(tx: i32, ty: i32) -> Vec2 {
    vec2((tx as f32 + 0.5) * TILE, (ty as f32 + 0.5) * TILE)
}

/// Map 1: "Grid". Walls come in different shapes (L, T, Z and long bars with
/// hooks), bricks sit in patches around them, and the middle is an open plaza
/// with a long lane through it, so rockets, rails and bolts each have somewhere
/// they shine. Mirrored left-right and top-bottom so no spawn is luckier than
/// another; every spawn can reach every other without breaking a brick.
pub const MAP_GRID: &str = "\
########################################
#S..bb...#b..bb..........bb..b#...bb..S#
#.##.....#bb......bbbb......bb#.....##.#
#.#...........###.b..b.###...........#.#
#......######..#...bb...#..######......#
#.b....b....#...bb####bb...#....b....b.#
#.b..#.bb...#bb..#....#..bb#...bb.#..b.#
#..b.#...##.b.b.##....##.b.b.##...#.b..#
#..bb#..bb##................##bb..#bb..#
#....##..b....................b..##....#
#.bb....#..bb......S.......bb..#....bb.#
#.bb....#..bb.......S......bb..#....bb.#
#....##..b....................b..##....#
#..bb#..bb##................##bb..#bb..#
#..b.#...##.b.b.##....##.b.b.##...#.b..#
#.b..#.bb...#bb..#....#..bb#...bb.#..b.#
#.b....b....#...bb####bb...#....b....b.#
#......######..#...bb...#..######......#
#.#...........###.b..b.###...........#.#
#.##.....#bb......bbbb......bb#.....##.#
#S..bb...#b..bb..........bb..b#...bb..S#
########################################";

/// Map 2: "Anthill". Liero's own idea: the arena is earth (bricks), and the
/// only open space is a nest of ant tunnels. What is new here: the map is
/// yours to change. Anything that blows up is a shovel, and a Block seals a
/// tunnel behind you. Rock layers (walls) run through the earth, so however
/// much is dug, the nest never becomes one big room: it opens up into long
/// galleries joined by a few shafts. The lane through the middle is plugged
/// with earth on each side; blasted open, it is the longest sightline there is.
pub const MAP_ANTHILL: &str = "########################################
#S..bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb..S#
#...bbbbbbbbbbb..S....S..bbbbbbbbbbb...#
#.#####bb#######.######.#######bb#####.#
#.bbbbbbbbbbbbbb.bbbbbb.bbbbbbbbbbbbbb.#
#....bb##bb......bbbbbb......bb##bb....#
#.#b.........bbbbbbbbbbbbbb.........b#.#
#.####bbb###.##############.###bbb####.#
#.bbbbbbbbbb................bbbbbbbbbb.#
#...bbbbbbbb...#........#...bbbbbbbb...#
#S......b......................b......S#
#S......b......................b......S#
#...bbbbbbbb...#........#...bbbbbbbb...#
#.bbbbbbbbbb................bbbbbbbbbb.#
#.####bbb###.##############.###bbb####.#
#.#b.........bbbbbbbbbbbbbb.........b#.#
#....bb##bb......bbbbbb......bb##bb....#
#.bbbbbbbbbbbbbb.bbbbbb.bbbbbbbbbbbbbb.#
#.#####bb#######.######.#######bb#####.#
#...bbbbbbbbbbb..S....S..bbbbbbbbbbb...#
#S..bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb..S#
########################################";

/// Map 3: "Pinball". The opposite of the Anthill: a wide open table where
/// nothing breaks and there is almost nowhere to hide. Single pillars are the
/// bumpers, the staircases are slingshots and a diamond sits in the middle. A
/// Bouncer bolt hits a staircase on a step's face or right on its corner, so
/// it comes off at angles a flat wall never gives. What is new here: cover is
/// something you bring. A Block is the only wall you can count on.
pub const MAP_PINBALL: &str = "\
########################################
#.........S..................S.........#
#.S............#........#............S.#
#.....#.....#...#......#...#.....#.....#
#................#....#................#
#......................................#
#........#....................#........#
#...#..............................#...#
#............#.....##.....#............#
#.S..#............####............#..S.#
#...###..........######..........###...#
#...###..........######..........###...#
#.S..#............####............#..S.#
#............#.....##.....#............#
#...#..............................#...#
#........#....................#........#
#......................................#
#................#....#................#
#.....#.....#...#......#...#.....#.....#
#.S............#........#............S.#
#.........S..................S.........#
########################################";

/// The menu's map: a plaza with a shooting range, and three alcoves at the
/// bottom, one per menu option. The player WALKS to a choice instead of picking
/// it from a list, so the menu is also the controls tutorial. It is an ordinary
/// map: the menu runs on the same sim as a match, with one player and no bots.
pub const MAP_HUB: &str = "\
########################################
#......................................#
#......................................#
#......................................#
#......................................#
#......................................#
#......................................#
#.......bbbbbb............bbbbbb.......#
#.......bbbbbb............bbbbbb.......#
#..................S...................#
#......................................#
#......................................#
#####.....########....########.....#####
####.......######......######.......####
####.......######......######.......####
####.......######......######.......####
####.......######......######.......####
####.......######......######.......####
####.......######......######.......####
####.......######......######.......####
####.......######......######.......####
########################################";

/// A map a match can be played on.
///
/// Pattern: Type Object, as with `WeaponDef`: a map is a row of data, and a new
/// map is a new row. Online, only its index in `MAPS` travels (in the `Welcome`),
/// never the name or the art: both ends have the same table.
pub struct MapDef {
    pub name: &'static str,
    /// One line for the setup screen.
    pub blurb: &'static str,
    pub art: &'static str,
}

/// The maps of the setup screen, in its order. The hub is not one of them: it
/// is the menu, not a place to play a match.
pub const MAPS: [MapDef; 3] = [
    MapDef { name: "Grid", blurb: "walls, bricks and an open plaza", art: MAP_GRID },
    MapDef { name: "Anthill", blurb: "solid earth: dig your own way", art: MAP_ANTHILL },
    MapDef { name: "Pinball", blurb: "open, unbreakable, bouncy", art: MAP_PINBALL },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_map_parses_and_has_spawns() {
        let map = Tilemap::from_ascii(MAP_GRID);
        assert_eq!(map.spawns.len(), 6);
        // Every spawn must be a free tile, or a player would start inside a wall.
        for s in &map.spawns {
            assert!(!map.circle_hits_wall(*s, 10.0));
        }
    }

    /// What every playable map must offer: a spawn for each of the (up to) four
    /// players, and spawns that can all reach each other without breaking a
    /// brick. Otherwise someone could start a match walled in, and the bots,
    /// who only dig when they have to, might never find anyone.
    #[test]
    fn every_map_is_playable() {
        for def in &MAPS {
            let map = Tilemap::from_ascii(def.art);
            assert!(map.spawns.len() >= 4, "{} needs at least 4 spawns", def.name);
            // Flood fill from the first spawn through empty tiles.
            let (sx, sy) = tile_coords(map.spawns[0]);
            let mut seen = vec![false; MAP_W * MAP_H];
            let mut todo = vec![(sx, sy)];
            while let Some((x, y)) = todo.pop() {
                if map.get(x, y) != Tile::Empty || seen[y as usize * MAP_W + x as usize] {
                    continue;
                }
                seen[y as usize * MAP_W + x as usize] = true;
                todo.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
            }
            for s in &map.spawns {
                let (x, y) = tile_coords(*s);
                assert!(seen[y as usize * MAP_W + x as usize], "{}: spawn ({x}, {y}) is cut off", def.name);
            }
        }
    }

    #[test]
    fn hub_map_parses_and_its_spawn_is_free() {
        let map = Tilemap::from_ascii(MAP_HUB);
        assert_eq!(map.spawns.len(), 1);
        assert!(!map.circle_hits_wall(map.spawns[0], 10.0));
    }

    #[test]
    fn brick_takes_three_hits() {
        let mut map = Tilemap::from_ascii(MAP_GRID);
        // Tile (4,1) is a brick in the Grid map.
        assert_eq!(map.get(4, 1), Tile::Brick(3));
        assert_eq!(map.damage(4, 1, 1), Some(Tile::Brick(2)));
        assert_eq!(map.damage(4, 1, 2), Some(Tile::Empty));
        assert_eq!(map.damage(4, 1, 1), None); // nothing left to hurt
    }

    #[test]
    fn solid_is_indestructible_and_outside_is_solid() {
        let mut map = Tilemap::from_ascii(MAP_GRID);
        assert_eq!(map.damage(0, 0, 99), None);
        assert_eq!(map.get(-5, 3), Tile::Solid);
    }
}
