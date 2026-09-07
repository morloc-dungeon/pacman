// Rust implementation of the test harness declared in lib/pacman/test.
//
// Per-test lines go to stderr and the verdict to stdout, so a caller can diff
// the verdict alone while a failure still explains itself on the terminal.

fn pht_paint(code: &str, msg: &str) -> String {
    format!("\u{1b}[{}m{}\u{1b}[0m", code, msg)
}

pub fn pht_equal<A: PartialEq + std::fmt::Debug>(
    msg: &String,
    x: &A,
    y: &A,
    r: (i64, i64),
) -> (i64, i64) {
    if x == y {
        eprintln!("  {} ... {}", msg, pht_paint("0;32", "PASS"));
        (r.0, r.1 + 1)
    } else {
        eprintln!("  {} ... {}", msg, pht_paint("0;31", "FAIL"));
        eprintln!("    expected: {:?}", y);
        eprintln!("    got:      {:?}", x);
        (r.0 + 1, r.1 + 1)
    }
}

pub fn pht_msg<A: Clone>(msg: &String, x: &A) -> A {
    eprintln!("{}", pht_paint("0;34", msg));
    x.clone()
}

pub fn pht_result(r: (i64, i64)) -> (i64, i64) {
    if r.0 == 0 {
        eprintln!("{}", pht_paint("0;32", &format!("All {} tests pass", r.1)));
    } else {
        eprintln!("{}", pht_paint("0;31", &format!("{}/{} tests failed", r.0, r.1)));
    }
    r
}
