// Rust implementation of the Pac-Man interface declared in lib/pacman.
//
// The maze is the arcade first level: 28 columns by 31 rows, defined by its
// left half and mirrored, so it is symmetric and correctly sized by
// construction. Tiles are '#' wall, '.' dot, 'o' energizer, '-' ghost-house
// door (ghosts only), ' ' open.
//
// Everything here is a pure function of the state it is given. Nothing reads
// the clock, the filesystem, or the terminal.

#[derive(Clone)]
pub struct PacGhost {
    pub pos: (i64, i64),
    pub dir: i64,
    pub kind: i64,
    pub state: i64,
    pub timer: i64,
}

#[derive(Clone)]
pub struct PacState {
    pub maze: Vec<String>,
    pub pac: (i64, i64),
    pub dir: i64,
    pub want: i64,
    pub ghosts: Vec<PacGhost>,
    pub score: i64,
    pub lives: i64,
    pub pellets: i64,
    pub fright: i64,
    pub chain: i64,
    pub phase: i64,
    pub ticks: i64,
    pub seed: i64,
    pub outcome: i64,
    pub savefile: String,
    pub level: i64,
}

#[derive(Clone)]
pub struct PacDifficulty {
    pub ghostSkip: i64,
    pub frightMove: i64,
    pub frightTicks: i64,
    pub release: Vec<i64>,
    pub phases: Vec<i64>,
}

#[derive(Clone)]
pub struct PacFrame {
    pub rows: Vec<String>,
    pub status: String,
    pub done: bool,
    pub save: bool,
    pub message: Vec<String>,
}

// The engine bundled as a value. morloc builds this struct; the fields are the
// fat trait objects a function value is stored in.
#[derive(Clone)]
pub struct PacCommands {
    pub newGame: std::rc::Rc<dyn rustmorloc::MorlocFn1<i64, PacState>>,
    pub keyOf: std::rc::Rc<dyn rustmorloc::MorlocFn1<String, i64>>,
    pub step: std::rc::Rc<dyn rustmorloc::MorlocFn2<i64, PacState, PacState>>,
    pub view: std::rc::Rc<dyn rustmorloc::MorlocFn1<PacState, PacFrame>>,
}

// ---------------------------------------------------------------------------
// Geometry

pub const PAC_ROWS: i64 = 31;
pub const PAC_COLS: i64 = 28;

// Directions, in the arcade's tie-breaking order: up, left, down, right.
const PAC_DR: [i64; 4] = [-1, 0, 1, 0];
const PAC_DC: [i64; 4] = [0, -1, 0, 1];

const PAC_PAC_START: (i64, i64) = (23, 13);
const PAC_DOOR_OUT: (i64, i64) = (11, 13);
const PAC_HOUSE_MID: (i64, i64) = (14, 13);

// The row that wraps from one side of the board to the other. It carries no dots.
const PAC_TUNNEL_ROW: i64 = 14;

// Scatter corners, one per ghost kind.
const PAC_SCATTER: [(i64, i64); 4] = [(0, 25), (0, 2), (30, 27), (30, 0)];

// Frames per second the engine assumes. The frontend is expected to send one
// tick command at this rate.
pub const PAC_TPS: i64 = 8;

// ---------------------------------------------------------------------------
// The difficulty curve
//
// One function of the level number. Level one reproduces the arcade's opening
// pace exactly; after that the ghosts quicken, the house empties sooner, the
// scatter breaks shorten, and an energizer frightens for less and less until it
// stops frightening at all. Nothing here knows what the board looks like.

pub fn pac_level_difficulty(level: i64) -> PacDifficulty {
    let l = level.max(1);
    // A hunting ghost loses one frame in four at first, then one in six, one in
    // eight, and finally none. It never gets a turn Pac-Man does not.
    let ghost_skip = if l >= 13 {
        0
    } else if l >= 6 {
        8
    } else if l >= 3 {
        6
    } else {
        4
    };
    // Frightened ghosts stay slow however deep the game gets; what changes is
    // how long they stay frightened.
    let fright_move = 2;
    let fright_ticks = (6 * PAC_TPS - 4 * (l - 1)).max(0);
    let wait = (4 * PAC_TPS - 3 * (l - 1)).max(0);
    let long_scatter = (7 * PAC_TPS - 4 * (l - 1)).max(PAC_TPS);
    let short_scatter = (5 * PAC_TPS - 4 * (l - 1)).max(PAC_TPS);
    let chase = 20 * PAC_TPS;
    PacDifficulty {
        ghostSkip: ghost_skip,
        frightMove: fright_move,
        frightTicks: fright_ticks,
        release: vec![0, 0, wait, 2 * wait],
        phases: vec![
            long_scatter,
            chase,
            long_scatter,
            chase,
            short_scatter,
            chase,
            short_scatter,
            i64::MAX,
        ],
    }
}

pub fn pac_difficulty_problems(d: &PacDifficulty) -> Vec<String> {
    let mut bad: Vec<String> = Vec::new();
    if d.ghostSkip == 1 {
        bad.push(String::from("a ghost that skips every frame never moves"));
    }
    if d.ghostSkip < 0 {
        bad.push(format!("ghostSkip is {}, want zero or more", d.ghostSkip));
    }
    if d.frightMove < 1 {
        bad.push(format!("frightMove is {}, want one or more", d.frightMove));
    }
    if d.frightTicks < 0 {
        bad.push(format!("frightTicks is {}, want zero or more", d.frightTicks));
    }
    if d.release.len() != 4 {
        bad.push(format!("{} release times, want one per ghost", d.release.len()));
    }
    if d.release.iter().any(|w| *w < 0) {
        bad.push(String::from("a ghost is released before the game starts"));
    }
    if d.phases.len() < 2 {
        bad.push(String::from("too few scatter and chase phases to alternate"));
    }
    if d.phases.iter().any(|p| *p <= 0) {
        bad.push(String::from("a phase lasts no time at all"));
    }
    bad
}

// Left half of every maze row.// Left half of every maze row. The right half is the mirror image.
const PAC_MAZE_HALF: [&str; 31] = [
    "##############",
    "#............#",
    "#.####.#####.#",
    "#o####.#####.#",
    "#.####.#####.#",
    "#.............",
    "#.####.##.####",
    "#.####.##.####",
    "#......##....#",
    "######.##### #",
    "     #.#####.#",
    "     #.##     ",
    "     #.## ###-",
    "######.## #   ",
    "          #   ",
    "######.## #   ",
    "     #.## ####",
    "     #.##     ",
    "     #.## ####",
    "######.## ####",
    "#............#",
    "#.####.#####.#",
    "#.####.#####.#",
    "#o..##....... ",
    "###.##.##.####",
    "###.##.##.####",
    "#......##....#",
    "#.##########.#",
    "#.##########.#",
    "#.............",
    "##############",
];

// ---------------------------------------------------------------------------
// Map generation
//
// A map is built on its left half and mirrored, so it is symmetric and exactly
// 28 wide by construction. Three regions are fixed because the rules depend on
// them: the ghost house and the tile above its door, the side tunnel, and the
// tile Pac-Man starts on. The rest is random wall blocks, each kept a corridor
// apart from every other, and anything the flood fill cannot reach afterwards
// is filled in. Pellets go only on reached tiles, so no map can strand one.

// Half-grid tiles the fixed regions occupy, with a corridor of clearance.
fn pac_reserved(r: i64, c: i64) -> bool {
    if (10..=17).contains(&r) && c >= 8 {
        return true;
    }
    if (13..=15).contains(&r) && c <= 9 {
        return true;
    }
    if (22..=24).contains(&r) && c >= 11 {
        return true;
    }
    false
}

fn pac_stamp_fixed(half: &mut [Vec<char>]) {
    for c in 0..14usize {
        half[0][c] = '#';
        half[30][c] = '#';
    }
    for r in 0..31usize {
        half[r][0] = if r as i64 == PAC_TUNNEL_ROW { ' ' } else { '#' };
    }
    // Ghost house: a box with a door on the mirror axis, open above it.
    for c in 10..14usize {
        half[12][c] = if c == 13 { '-' } else { '#' };
        half[16][c] = '#';
    }
    for r in 13..16usize {
        half[r][10] = '#';
        for c in 11..14usize {
            half[r][c] = ' ';
        }
    }
    for c in 9..14usize {
        half[11][c] = ' ';
    }
    for c in 0..10usize {
        half[PAC_TUNNEL_ROW as usize][c] = ' ';
    }
    half[23][13] = ' ';
}

fn pac_block_fits(half: &[Vec<char>], r0: i64, c0: i64, w: i64, h: i64) -> bool {
    if r0 < 1 || c0 < 1 || r0 + h > 30 || c0 + w > 14 {
        return false;
    }
    // A one-tile halo keeps every block a corridor apart from its neighbours
    // and from the fixed regions, so corridors never close up.
    for r in (r0 - 1)..=(r0 + h) {
        for c in (c0 - 1)..=(c0 + w) {
            if r < 0 || r > 30 || c < 0 || c > 13 {
                continue;
            }
            if pac_reserved(r, c) || half[r as usize][c as usize] == '#' {
                return false;
            }
        }
    }
    true
}

fn pac_carve(level: i64) -> Vec<Vec<char>> {
    let mut half: Vec<Vec<char>> = vec![vec![' '; 14]; 31];
    let mut seed = pac_rand(level.wrapping_mul(2654435761) ^ 0x5f3a);
    // Chunky blocks first, then thin walls to divide what they left over. A gap
    // three tiles across takes a one-tile wall with a corridor either side,
    // which is what keeps the board from turning into open plaza.
    let passes: [(i64, i64, i64, i64); 3] = [(2, 3, 2, 3), (1, 1, 2, 4), (2, 4, 1, 1)];
    for (wmin, wspan, hmin, hspan) in passes {
        for _ in 0..3000 {
            seed = pac_rand(seed);
            let w = wmin + (seed >> 3) % wspan;
            seed = pac_rand(seed);
            let h = hmin + (seed >> 3) % hspan;
            seed = pac_rand(seed);
            let r0 = 1 + (seed >> 3) % 28;
            seed = pac_rand(seed);
            let c0 = 1 + (seed >> 3) % 12;
            if !pac_block_fits(&half, r0, c0, w, h) {
                continue;
            }
            for r in r0..(r0 + h) {
                for c in c0..(c0 + w) {
                    half[r as usize][c as usize] = '#';
                }
            }
        }
    }
    pac_stamp_fixed(&mut half);
    half
}

fn pac_mirror(half: &[Vec<char>]) -> Vec<Vec<char>> {
    half.iter()
        .map(|row| {
            let mut full: Vec<char> = row.clone();
            full.extend(row.iter().rev());
            full
        })
        .collect()
}

// Tiles Pac-Man can stand on, reached from where he starts.
fn pac_reachable(grid: &[Vec<char>]) -> Vec<Vec<bool>> {
    let mut seen = vec![vec![false; PAC_COLS as usize]; PAC_ROWS as usize];
    let mut stack = vec![PAC_PAC_START];
    while let Some((r, c)) = stack.pop() {
        if r < 0 || r >= PAC_ROWS {
            continue;
        }
        let c = pac_wrap(c);
        // Anything that is not wall or door is floor, pellets included: this
        // runs both before pellets are placed and over a finished map.
        let t = grid[r as usize][c as usize];
        if seen[r as usize][c as usize] || t == '#' || t == '-' {
            continue;
        }
        seen[r as usize][c as usize] = true;
        for d in 0..4usize {
            stack.push((r + PAC_DR[d], c + PAC_DC[d]));
        }
    }
    seen
}

fn pac_in_house(r: i64, c: i64) -> bool {
    (11..=16).contains(&r) && (10..=17).contains(&c)
}

pub fn pac_make_maze(level: i64) -> Vec<String> {
    if level <= 1 {
        return pac_fresh_maze();
    }
    let mut grid = pac_mirror(&pac_carve(level));
    let seen = pac_reachable(&grid);
    // Anything the flood fill missed is scenery, not playfield.
    for r in 0..PAC_ROWS {
        for c in 0..PAC_COLS {
            if grid[r as usize][c as usize] == ' '
                && !seen[r as usize][c as usize]
                && !pac_in_house(r, c)
            {
                grid[r as usize][c as usize] = '#';
            }
        }
    }
    // Dots on every reached tile except the tunnel row, which has none, and the
    // tile Pac-Man is standing on.
    let mut dots: Vec<(i64, i64)> = Vec::new();
    for r in 0..PAC_ROWS {
        for c in 0..PAC_COLS {
            // Pac-Man's tile is left bare, and so is its mirror: a dot on one
            // and not the other would break the board's symmetry.
            let under_pac = r == PAC_PAC_START.0
                && (c == PAC_PAC_START.1 || c == PAC_COLS - 1 - PAC_PAC_START.1);
            if seen[r as usize][c as usize]
                && r != PAC_TUNNEL_ROW
                && !under_pac
                && !pac_in_house(r, c)
            {
                grid[r as usize][c as usize] = '.';
                dots.push((r, c));
            }
        }
    }
    // One energizer per quadrant, on the dot furthest from the middle.
    let mid = (PAC_ROWS / 2, PAC_COLS / 2);
    for quad in 0..4 {
        let pick = dots
            .iter()
            .filter(|(r, c)| ((*r < mid.0) as i64) * 2 + ((*c < mid.1) as i64) == quad)
            .max_by_key(|(r, c)| pac_dist2((*r, *c), mid));
        if let Some(&(r, c)) = pick {
            grid[r as usize][c as usize] = 'o';
        }
    }
    grid.into_iter().map(|r| r.into_iter().collect()).collect()
}

fn pac_fresh_maze() -> Vec<String> {
    PAC_MAZE_HALF
        .iter()
        .map(|h| {
            let mut row = String::with_capacity(28);
            row.push_str(h);
            row.extend(h.chars().rev());
            row
        })
        .collect()
}

fn pac_at(maze: &[String], r: i64, c: i64) -> char {
    if r < 0 || r >= PAC_ROWS || c < 0 || c >= PAC_COLS {
        return '#';
    }
    maze[r as usize].as_bytes()[c as usize] as char
}

fn pac_set(maze: &mut [String], r: i64, c: i64, ch: char) {
    let row: String = maze[r as usize]
        .chars()
        .enumerate()
        .map(|(i, x)| if i as i64 == c { ch } else { x })
        .collect();
    maze[r as usize] = row;
}

// Wrap a column through the side tunnel.
fn pac_wrap(c: i64) -> i64 {
    (c + PAC_COLS) % PAC_COLS
}

// Can this actor stand on the tile? The ghost-house door is passable only to
// a ghost that is leaving or returning.
fn pac_open(maze: &[String], r: i64, c: i64, door: bool) -> bool {
    let ch = pac_at(maze, r, pac_wrap(c));
    ch != '#' && (ch != '-' || door)
}

fn pac_ahead(pos: (i64, i64), dir: i64, n: i64) -> (i64, i64) {
    // The arcade's up-direction target overflow: aiming up also shifts the
    // target left by the same amount. Reproduced deliberately.
    let mut r = pos.0 + PAC_DR[dir as usize] * n;
    let mut c = pos.1 + PAC_DC[dir as usize] * n;
    if dir == 0 {
        c -= n;
    }
    if r < -8 {
        r = -8;
    }
    if c < -8 {
        c = -8;
    }
    (r, c)
}

fn pac_dist2(a: (i64, i64), b: (i64, i64)) -> i64 {
    let dr = a.0 - b.0;
    let dc = a.1 - b.1;
    dr * dr + dc * dc
}

fn pac_rand(seed: i64) -> i64 {
    (seed.wrapping_mul(1103515245).wrapping_add(12345)) & 0x7fffffff
}

// ---------------------------------------------------------------------------
// Construction

fn pac_start_ghosts(d: &PacDifficulty) -> Vec<PacGhost> {
    let wait = |i: usize| *d.release.get(i).unwrap_or(&0);
    vec![
        PacGhost { pos: PAC_DOOR_OUT, dir: 1, kind: 0, state: 1, timer: wait(0) },
        PacGhost { pos: (14, 13), dir: 0, kind: 1, state: 0, timer: wait(1) },
        PacGhost { pos: (14, 11), dir: 0, kind: 2, state: 0, timer: wait(2) },
        PacGhost { pos: (14, 16), dir: 0, kind: 3, state: 0, timer: wait(3) },
    ]
}

pub fn pac_new_game(lives: i64) -> PacState {
    let maze = pac_make_maze(1);
    let pellets = maze
        .iter()
        .map(|r| r.chars().filter(|c| *c == '.' || *c == 'o').count() as i64)
        .sum();
    PacState {
        maze,
        pac: PAC_PAC_START,
        dir: 1,
        want: 1,
        ghosts: pac_start_ghosts(&pac_level_difficulty(1)),
        score: 0,
        lives,
        pellets,
        fright: 0,
        chain: 0,
        phase: 0,
        ticks: 0,
        seed: 1,
        outcome: 0,
        savefile: String::from("pacman.sav"),
        level: 1,
    }
}

// Carry the player forward onto the next board: the score and what is left of
// their lives, nothing else.
fn pac_next_level(s: &PacState) -> PacState {
    let level = s.level + 1;
    let maze = pac_make_maze(level);
    let pellets = maze
        .iter()
        .map(|r| r.chars().filter(|c| *c == '.' || *c == 'o').count() as i64)
        .sum();
    PacState {
        maze,
        pac: PAC_PAC_START,
        dir: 1,
        want: 1,
        ghosts: pac_start_ghosts(&pac_level_difficulty(level)),
        score: s.score,
        lives: s.lives,
        pellets,
        fright: 0,
        chain: 0,
        phase: 0,
        ticks: 0,
        seed: s.seed,
        outcome: 0,
        savefile: s.savefile.clone(),
        level,
    }
}

pub fn pac_with_save_file(path: &String, s: &PacState) -> PacState {
    let mut t = s.clone();
    t.savefile = path.clone();
    t
}

// ---------------------------------------------------------------------------
// Input

pub fn pac_key_of(k: &String) -> i64 {
    match k.as_str() {
        "up" | "k" | "w" => 1,
        "down" | "j" => 2,
        "left" | "h" | "a" => 3,
        "right" | "l" | "d" => 4,
        "s" => 7,
        "c" => 8,
        "q" | "Escape" => 6,
        _ => 0,
    }
}

// Command direction (1..4) to internal direction (0..3).
fn pac_cmd_dir(cmd: i64) -> i64 {
    match cmd {
        1 => 0,
        3 => 1,
        2 => 2,
        4 => 3,
        _ => -1,
    }
}

// ---------------------------------------------------------------------------
// Ghost behaviour

fn pac_target(g: &PacGhost, s: &PacState, blinky: (i64, i64), scatter: bool) -> (i64, i64) {
    if g.state == 3 {
        return PAC_DOOR_OUT;
    }
    if scatter {
        return PAC_SCATTER[g.kind as usize];
    }
    match g.kind {
        0 => s.pac,
        1 => pac_ahead(s.pac, s.dir, 4),
        2 => {
            let p = pac_ahead(s.pac, s.dir, 2);
            (2 * p.0 - blinky.0, 2 * p.1 - blinky.1)
        }
        _ => {
            if pac_dist2(g.pos, s.pac) > 64 {
                s.pac
            } else {
                PAC_SCATTER[3]
            }
        }
    }
}

// Should this ghost move on this tick? Hunting ghosts run at three quarters
// speed, frightened ghosts at half, and eyes at full.
fn pac_ghost_moves(g: &PacGhost, ticks: i64, d: &PacDifficulty) -> bool {
    match g.state {
        2 => d.frightMove <= 1 || ticks % d.frightMove == 0,
        3 => true,
        _ => d.ghostSkip <= 0 || ticks % d.ghostSkip != 0,
    }
}

fn pac_move_ghost(g: &PacGhost, s: &PacState, blinky: (i64, i64), scatter: bool, seed: i64) -> PacGhost {
    let mut out = g.clone();

    // Waiting in the house.
    if g.state == 0 {
        if g.timer > 0 {
            out.timer = g.timer - 1;
            return out;
        }
        if g.pos.1 != PAC_HOUSE_MID.1 {
            out.pos = (g.pos.0, g.pos.1 + if g.pos.1 < PAC_HOUSE_MID.1 { 1 } else { -1 });
            return out;
        }
        out.pos = (g.pos.0 - 1, g.pos.1);
        out.dir = 0;
        if out.pos.0 <= PAC_DOOR_OUT.0 {
            out.pos = PAC_DOOR_OUT;
            out.state = 1;
            out.dir = 1;
        }
        return out;
    }

    // Eyes that have reached the door drop back into the house.
    if g.state == 3 && g.pos == PAC_DOOR_OUT {
        out.pos = PAC_HOUSE_MID;
        out.state = 0;
        out.timer = PAC_TPS;
        return out;
    }

    let target = pac_target(g, s, blinky, scatter);
    let back = (g.dir + 2) % 4;
    let mut best = -1i64;
    let mut best_d = i64::MAX;
    let mut legal: Vec<i64> = Vec::new();
    for d in 0..4i64 {
        if d == back {
            continue;
        }
        let r = g.pos.0 + PAC_DR[d as usize];
        let c = g.pos.1 + PAC_DC[d as usize];
        if !pac_open(&s.maze, r, c, g.state == 3) {
            continue;
        }
        legal.push(d);
        let dist = pac_dist2((r, pac_wrap(c)), target);
        if dist < best_d {
            best_d = dist;
            best = d;
        }
    }
    if legal.is_empty() {
        // Dead end: the only way out is back the way we came.
        out.dir = back;
    } else if g.state == 2 {
        out.dir = legal[(seed as usize) % legal.len()];
    } else {
        out.dir = best;
    }
    let r = g.pos.0 + PAC_DR[out.dir as usize];
    let c = pac_wrap(g.pos.1 + PAC_DC[out.dir as usize]);
    if pac_open(&s.maze, r, c, g.state == 3) {
        out.pos = (r, c);
    }
    out
}

// ---------------------------------------------------------------------------
// The step function

fn pac_reset_positions(s: &mut PacState) {
    s.pac = PAC_PAC_START;
    s.dir = 1;
    s.want = 1;
    s.ghosts = pac_start_ghosts(&pac_level_difficulty(s.level));
    s.fright = 0;
    s.chain = 0;
    s.phase = 0;
    s.ticks = 0;
}

// A ghost and Pac-Man share a tile, or swapped tiles passing through each
// other. Either counts as a collision on a grid.
fn pac_touching(before: (i64, i64), after: (i64, i64), gb: (i64, i64), ga: (i64, i64)) -> bool {
    after == ga || (after == gb && before == ga)
}

fn pac_tick(s: &PacState) -> PacState {
    let d = pac_level_difficulty(s.level);
    let mut out = s.clone();
    out.ticks = s.ticks + 1;

    // Scatter/chase schedule. A phase change reverses every hunting ghost.
    let mut phase_elapsed = out.ticks;
    let mut phase = 0usize;
    while phase < d.phases.len() - 1 && phase_elapsed > d.phases[phase] {
        phase_elapsed -= d.phases[phase];
        phase += 1;
    }
    let reversed = (phase as i64) != s.phase;
    out.phase = phase as i64;
    let scatter = phase % 2 == 0;

    if out.fright > 0 {
        out.fright -= 1;
        if out.fright == 0 {
            out.chain = 0;
            for g in out.ghosts.iter_mut() {
                if g.state == 2 {
                    g.state = 1;
                }
            }
        }
    }

    // Pac-Man takes the queued turn if it is legal, else carries on.
    let before = out.pac;
    let mut dir = out.dir;
    let wr = out.pac.0 + PAC_DR[out.want as usize];
    let wc = out.pac.1 + PAC_DC[out.want as usize];
    if pac_open(&out.maze, wr, wc, false) {
        dir = out.want;
    }
    let nr = out.pac.0 + PAC_DR[dir as usize];
    let nc = pac_wrap(out.pac.1 + PAC_DC[dir as usize]);
    if pac_open(&out.maze, nr, nc, false) {
        out.pac = (nr, nc);
    }
    out.dir = dir;

    // Eat.
    let old_score = out.score;
    let tile = pac_at(&out.maze, out.pac.0, out.pac.1);
    if tile == '.' || tile == 'o' {
        pac_set(&mut out.maze, out.pac.0, out.pac.1, ' ');
        out.pellets -= 1;
        if tile == 'o' {
            out.score += 50;
            out.fright = d.frightTicks;
            out.chain = 0;
            // Deep in the game an energizer is worth points and nothing else.
            if d.frightTicks > 0 {
                for g in out.ghosts.iter_mut() {
                    if g.state == 1 {
                        g.state = 2;
                        g.dir = (g.dir + 2) % 4;
                    }
                }
            }
        } else {
            out.score += 10;
        }
    }

    // Move the ghosts. Blinky's tile is Inky's second reference point, so it is
    // read before anyone moves.
    let blinky = out.ghosts[0].pos;
    let mut seed = out.seed;
    let mut moved: Vec<PacGhost> = Vec::with_capacity(out.ghosts.len());
    for g in out.ghosts.iter() {
        let mut g2 = g.clone();
        if reversed && (g2.state == 1 || g2.state == 2) {
            g2.dir = (g2.dir + 2) % 4;
        }
        if pac_ghost_moves(&g2, out.ticks, &d) {
            seed = pac_rand(seed);
            moved.push(pac_move_ghost(&g2, &out, blinky, scatter, seed));
        } else {
            moved.push(g2);
        }
    }
    out.seed = seed;

    // Collisions.
    let mut died = false;
    for (i, g) in moved.iter_mut().enumerate() {
        let gb = out.ghosts[i].pos;
        if g.state == 0 || g.state == 3 {
            continue;
        }
        if !pac_touching(before, out.pac, gb, g.pos) {
            continue;
        }
        if g.state == 2 {
            out.score += 200 << out.chain;
            out.chain = (out.chain + 1).min(3);
            g.state = 3;
        } else {
            died = true;
        }
    }
    out.ghosts = moved;

    if old_score < 10000 && out.score >= 10000 {
        out.lives += 1;
    }

    if died {
        out.lives -= 1;
        if out.lives <= 0 {
            out.lives = 0;
            out.outcome = 2;
        } else {
            pac_reset_positions(&mut out);
        }
    } else if out.pellets == 0 {
        out.outcome = 1;
    }

    out
}

pub fn pac_step(cmd: i64, s: &PacState) -> PacState {
    if s.outcome != 0 {
        // A finished game takes no orders except the one that starts the next
        // board, and only when there is a next board to start.
        if cmd == 8 && s.outcome == 1 {
            return pac_next_level(s);
        }
        return s.clone();
    }
    match cmd {
        6 => {
            let mut out = s.clone();
            out.outcome = 3;
            out
        }
        7 => {
            let mut out = s.clone();
            out.outcome = 4;
            out
        }
        5 => pac_tick(s),
        1 | 2 | 3 | 4 => {
            let mut out = s.clone();
            out.want = pac_cmd_dir(cmd);
            out
        }
        _ => s.clone(),
    }
}

// ---------------------------------------------------------------------------
// Rendering

fn pac_ghost_char(g: &PacGhost) -> char {
    match g.state {
        2 => 'F',
        3 => '"',
        _ => match g.kind {
            0 => 'B',
            1 => 'P',
            2 => 'I',
            _ => 'C',
        },
    }
}

fn pac_pac_char(dir: i64) -> char {
    match dir {
        0 => '^',
        1 => '<',
        2 => 'v',
        _ => '>',
    }
}

// The end of a game is its own screen: the board has nothing left to say, and
// the player wants the tally. Built here rather than in a frontend so every
// frontend shows the same thing and the suite can check it.
fn pac_score_rows(s: &PacState) -> Vec<String> {
    let cleared = s.outcome == 1;
    let title = if cleared {
        format!("LEVEL {} CLEARED", s.level)
    } else {
        String::from("GAME OVER")
    };
    vec![
        String::new(),
        title,
        String::new(),
        format!("SCORE {:>10}", s.score),
        format!("DOTS LEFT {:>7}", s.pellets),
        format!("LIVES LEFT {:>5}", s.lives),
        String::new(),
        String::from(if cleared {
            "press c to go on, q to quit"
        } else {
            "press q to quit"
        }),
    ]
}

pub fn pac_view(s: &PacState) -> PacFrame {
    let mut grid: Vec<Vec<char>> = s.maze.iter().map(|r| r.chars().collect()).collect();
    for g in s.ghosts.iter() {
        if g.pos.0 >= 0 && g.pos.0 < PAC_ROWS {
            grid[g.pos.0 as usize][g.pos.1 as usize] = pac_ghost_char(g);
        }
    }
    grid[s.pac.0 as usize][s.pac.1 as usize] = pac_pac_char(s.dir);

    let mode = match s.outcome {
        1 => "YOU WIN",
        2 => "GAME OVER",
        3 => "QUIT",
        4 => "SAVED",
        _ => {
            if s.fright > 0 {
                "POWER"
            } else if s.phase % 2 == 0 {
                "SCATTER"
            } else {
                "CHASE"
            }
        }
    };

    let message: Vec<String> = if s.outcome == 1 || s.outcome == 2 {
        pac_score_rows(s)
    } else {
        Vec::new()
    };

    PacFrame {
        rows: grid.into_iter().map(|r| r.into_iter().collect()).collect(),
        status: format!(
            "LEVEL {}  SCORE {:>6}  LIVES {}  DOTS {:>3}  {}",
            s.level, s.score, s.lives, s.pellets, mode
        ),
        done: s.outcome != 0,
        save: s.outcome == 4,
        message,
    }
}

// ---------------------------------------------------------------------------
// Persistence support

pub fn pac_to_save(r: &(bool, PacState)) -> Vec<PacState> {
    if r.0 {
        let mut s = r.1.clone();
        // A saved game is stored mid-play: the outcome that ended the session
        // is not part of it, or resuming would land on a finished game.
        s.outcome = 0;
        vec![s]
    } else {
        Vec::new()
    }
}

pub fn pac_each<A, F: Fn(&A)>(xs: &Vec<A>, f: F) {
    for x in xs.iter() {
        f(x);
    }
}

// ---------------------------------------------------------------------------
// Map validation
//
// What it means for a map to be playable. A generator is checked against this
// rather than against a picture of what it should look like, so a new generator
// is judged by the same rule as a hand-drawn board.

pub fn pac_maze_problems(maze: &Vec<String>) -> Vec<String> {
    let mut bad: Vec<String> = Vec::new();
    if maze.len() as i64 != PAC_ROWS {
        bad.push(format!("{} rows, want {}", maze.len(), PAC_ROWS));
        return bad;
    }
    for (i, row) in maze.iter().enumerate() {
        if row.chars().count() as i64 != PAC_COLS {
            bad.push(format!("row {} is {} wide, want {}", i, row.chars().count(), PAC_COLS));
        }
        if let Some(c) = row.chars().find(|c| !"#.o- ".contains(*c)) {
            bad.push(format!("row {} holds {:?}, not a tile", i, c));
        }
    }
    if !bad.is_empty() {
        return bad;
    }

    let grid: Vec<Vec<char>> = maze.iter().map(|r| r.chars().collect()).collect();
    let at = |r: i64, c: i64| grid[r as usize][pac_wrap(c) as usize];

    for row in [0, PAC_ROWS - 1] {
        if (0..PAC_COLS).any(|c| at(row, c) != '#') {
            bad.push(format!("row {} is not a wall", row));
        }
    }
    for (i, row) in maze.iter().enumerate() {
        let f: Vec<char> = row.chars().collect();
        if (0..14).any(|c| f[c] != f[27 - c]) {
            bad.push(format!("row {} is not symmetric", i));
        }
    }
    if at(PAC_PAC_START.0, PAC_PAC_START.1) != ' ' {
        bad.push(String::from("Pac-Man starts inside a wall"));
    }
    if at(PAC_DOOR_OUT.0, PAC_DOOR_OUT.1) != ' ' {
        bad.push(String::from("no room above the ghost-house door"));
    }
    if at(12, 13) != '-' || at(12, 14) != '-' {
        bad.push(String::from("the ghost house has no door"));
    }
    for c in [PAC_HOUSE_MID.1 - 2, PAC_HOUSE_MID.1, PAC_HOUSE_MID.1 + 3] {
        if at(PAC_HOUSE_MID.0, c) != ' ' {
            bad.push(format!("no room in the ghost house at column {}", c));
        }
    }
    // A ghost leaves by walking up the middle to the door; that run must be open.
    for r in (PAC_DOOR_OUT.0)..=(PAC_HOUSE_MID.0) {
        if at(r, PAC_HOUSE_MID.1) == '#' {
            bad.push(format!("the ghosts cannot get out at row {}", r));
        }
    }
    if at(PAC_TUNNEL_ROW, 0) == '#' || at(PAC_TUNNEL_ROW, PAC_COLS - 1) == '#' {
        bad.push(String::from("the tunnel is walled off"));
    }

    let seen = pac_reachable(&grid);
    let mut dots = 0;
    for r in 0..PAC_ROWS {
        for c in 0..PAC_COLS {
            let t = at(r, c);
            if t != '.' && t != 'o' {
                continue;
            }
            dots += 1;
            if !seen[r as usize][pac_wrap(c) as usize] {
                bad.push(format!("the pellet at {},{} cannot be reached", r, c));
            }
        }
    }
    if dots < 100 {
        bad.push(format!("only {} pellets, too few to play", dots));
    }
    let energizers = maze.iter().map(|r| r.matches('o').count()).sum::<usize>();
    if energizers != 4 {
        bad.push(format!("{} energizers, want 4", energizers));
    }
    bad
}
