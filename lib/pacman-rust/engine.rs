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

// Scatter corners, one per ghost kind.
const PAC_SCATTER: [(i64, i64); 4] = [(0, 25), (0, 2), (30, 27), (30, 0)];

// Frames per second the engine assumes. The frontend is expected to send one
// tick command at this rate.
pub const PAC_TPS: i64 = 8;

// Scatter/chase durations in ticks, alternating scatter first. The last entry
// is chase forever.
const PAC_PHASES: [i64; 8] = [
    7 * PAC_TPS,
    20 * PAC_TPS,
    7 * PAC_TPS,
    20 * PAC_TPS,
    5 * PAC_TPS,
    20 * PAC_TPS,
    5 * PAC_TPS,
    i64::MAX,
];

const PAC_FRIGHT_TICKS: i64 = 6 * PAC_TPS;

// Ticks each ghost waits in the house before leaving.
const PAC_RELEASE: [i64; 4] = [0, 0, 4 * PAC_TPS, 8 * PAC_TPS];

// Left half of every maze row. The right half is the mirror image.
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

// Every maze row is its left half mirrored, so a half's pellets count twice.
fn pac_total_pellets() -> i64 {
    PAC_MAZE_HALF
        .iter()
        .map(|h| 2 * h.chars().filter(|c| *c == '.' || *c == 'o').count() as i64)
        .sum()
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

fn pac_start_ghosts() -> Vec<PacGhost> {
    vec![
        PacGhost { pos: PAC_DOOR_OUT, dir: 1, kind: 0, state: 1, timer: 0 },
        PacGhost { pos: (14, 13), dir: 0, kind: 1, state: 0, timer: PAC_RELEASE[1] },
        PacGhost { pos: (14, 11), dir: 0, kind: 2, state: 0, timer: PAC_RELEASE[2] },
        PacGhost { pos: (14, 16), dir: 0, kind: 3, state: 0, timer: PAC_RELEASE[3] },
    ]
}

pub fn pac_new_game(lives: i64) -> PacState {
    let maze = pac_fresh_maze();
    let pellets = maze
        .iter()
        .map(|r| r.chars().filter(|c| *c == '.' || *c == 'o').count() as i64)
        .sum();
    PacState {
        maze,
        pac: PAC_PAC_START,
        dir: 1,
        want: 1,
        ghosts: pac_start_ghosts(),
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
fn pac_ghost_moves(g: &PacGhost, ticks: i64) -> bool {
    match g.state {
        2 => ticks % 2 == 0,
        3 => true,
        _ => ticks % 4 != 0,
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
    s.ghosts = pac_start_ghosts();
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
    let mut out = s.clone();
    out.ticks = s.ticks + 1;

    // Scatter/chase schedule. A phase change reverses every hunting ghost.
    let mut phase_elapsed = out.ticks;
    let mut phase = 0usize;
    while phase < PAC_PHASES.len() - 1 && phase_elapsed > PAC_PHASES[phase] {
        phase_elapsed -= PAC_PHASES[phase];
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
            out.fright = PAC_FRIGHT_TICKS;
            out.chain = 0;
            for g in out.ghosts.iter_mut() {
                if g.state == 1 {
                    g.state = 2;
                    g.dir = (g.dir + 2) % 4;
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
        if pac_ghost_moves(&g2, out.ticks) {
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
    let title = if s.outcome == 1 { "YOU WIN" } else { "GAME OVER" };
    let eaten = pac_total_pellets() - s.pellets;
    vec![
        String::new(),
        String::from(title),
        String::new(),
        format!("SCORE {:>10}", s.score),
        format!("DOTS EATEN {:>5}", eaten),
        format!("LIVES LEFT {:>5}", s.lives),
        String::new(),
        String::from("press q to quit"),
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
            "SCORE {:>6}  LIVES {}  DOTS {:>3}  {}",
            s.score, s.lives, s.pellets, mode
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
