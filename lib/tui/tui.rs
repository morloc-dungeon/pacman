// Terminal frontend. The morloc-facing entry points are at the bottom of the
// file; the implementation lives in an inner module so it can use ordinary
// `use` statements. Sourced .rs files are concatenated at the pool crate root,
// where a top-level `use` would collide with another module's.

mod pac_tui_impl {
    use ratatui::backend::{CrosstermBackend, TestBackend};
    use ratatui::crossterm::{cursor, event, execute, terminal};
    use ratatui::layout::{Alignment, Position, Rect};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph};
    use ratatui::Terminal;

    const FRAME_MS: u64 = 125;
    const CMD_TICK: i64 = 5;
    const CMD_QUIT: i64 = 6;
    const CMD_SAVE: i64 = 7;

    // The terminal, borrowed for the length of a game and handed back on the
    // way out -- including on a panic, because the pool is built with
    // `panic = "unwind"`.
    //
    // A morloc pool runs in its own process group, so it is a background job as
    // far as the terminal is concerned and a raw-mode read would take SIGTTIN
    // and stop the process. Claiming the foreground process group is what makes
    // reading keys possible at all.
    struct Screen {
        tty: std::fs::File,
        prev_pgrp: i32,
    }

    // Take the terminal's foreground process group. Doing this from a
    // background group raises SIGTTOU at the caller, which would stop us, so
    // the signal is ignored across the call.
    fn set_pgrp(fd: i32, pgrp: i32) {
        unsafe {
            let old = libc::signal(libc::SIGTTOU, libc::SIG_IGN);
            libc::tcsetpgrp(fd, pgrp);
            libc::signal(libc::SIGTTOU, old);
        }
    }

    impl Screen {
        fn enter() -> std::io::Result<Screen> {
            let tty = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("/dev/tty")?;
            let fd = std::os::unix::io::AsRawFd::as_raw_fd(&tty);
            let prev_pgrp = unsafe { libc::tcgetpgrp(fd) };
            set_pgrp(fd, unsafe { libc::getpgrp() });
            terminal::enable_raw_mode()?;
            execute!(&tty, terminal::EnterAlternateScreen, cursor::Hide)?;
            Ok(Screen { tty, prev_pgrp })
        }
    }

    impl Drop for Screen {
        fn drop(&mut self) {
            let _ = execute!(&self.tty, terminal::LeaveAlternateScreen, cursor::Show);
            let _ = terminal::disable_raw_mode();
            let fd = std::os::unix::io::AsRawFd::as_raw_fd(&self.tty);
            if self.prev_pgrp > 0 {
                set_pgrp(fd, self.prev_pgrp);
            }
        }
    }

    // How a tile is drawn. A terminal cell is about twice as tall as it is
    // wide, so a tile gets two columns where there is room for them and the
    // board keeps its proportions; the one-column forms are the fallback for a
    // narrow window. Walls fill their columns; everything else is a glyph and a
    // space, which also gives the dots their arcade spacing.
    fn tile(c: char, wide: bool) -> (&'static str, Style) {
        let pac = Style::default()
            .fg(Color::Rgb(255, 255, 0))
            .add_modifier(Modifier::BOLD);
        let pellet = Style::default().fg(Color::Rgb(255, 214, 170));
        let (w2, w1, style) = match c {
            '#' => ("\u{2588}\u{2588}", "\u{2588}", Style::default().fg(Color::Rgb(33, 33, 222))),
            '.' => ("\u{b7} ", "\u{b7}", pellet),
            'o' => ("\u{25cf} ", "\u{25cf}", pellet.add_modifier(Modifier::BOLD)),
            '-' => ("\u{2500}\u{2500}", "\u{2500}", Style::default().fg(Color::Rgb(255, 184, 255))),
            '^' => ("\u{25b2} ", "\u{25b2}", pac),
            'v' => ("\u{25bc} ", "\u{25bc}", pac),
            '<' => ("\u{25c4} ", "\u{25c4}", pac),
            '>' => ("\u{25ba} ", "\u{25ba}", pac),
            'B' => ("\u{15e3} ", "\u{15e3}", Style::default().fg(Color::Rgb(255, 0, 0))),
            'P' => ("\u{15e3} ", "\u{15e3}", Style::default().fg(Color::Rgb(255, 184, 255))),
            'I' => ("\u{15e3} ", "\u{15e3}", Style::default().fg(Color::Rgb(0, 255, 222))),
            'C' => ("\u{15e3} ", "\u{15e3}", Style::default().fg(Color::Rgb(255, 184, 82))),
            'F' => (
                "\u{15e3} ",
                "\u{15e3}",
                Style::default()
                    .fg(Color::Rgb(33, 33, 222))
                    .add_modifier(Modifier::BOLD),
            ),
            '"' => ("\u{a8} ", "\u{a8}", Style::default().fg(Color::White)),
            _ => ("  ", " ", Style::default()),
        };
        (if wide { w2 } else { w1 }, style)
    }

    const KEYS: &str = "arrows or hjkl move    s save and quit    q quit";
    const KEYS_SHORT: &str = "hjkl move  s save  q quit";

    // What the board is allowed to drop when the window is short. The maze
    // itself is fixed, so these are the only rows there are to give back.
    // Ordered by what is least missed: the blank line under the board, then the
    // frame, then the key legend (which first compresses onto the status line
    // rather than vanishing). The first layout that fits is used.
    #[derive(Clone, Copy)]
    struct Layout {
        wide: bool,
        border: bool,
        spacer: bool,
        footer: bool,
        hint: bool,
    }

    const TRIMS: [(bool, bool, bool, bool); 6] = [
        (true, true, true, false),
        (true, false, true, false),
        (false, true, true, false),
        (false, false, true, false),
        (false, false, false, true),
        (false, false, false, false),
    ];

    // Rows and columns a layout needs, without building it.
    fn extent(frame: &crate::PacFrame, lay: Layout) -> (u16, u16) {
        let (body_rows, body_w) = if frame.message.is_empty() {
            (
                frame.rows.len(),
                frame.rows.iter().map(|r| r.chars().count()).max().unwrap_or(0)
                    * if lay.wide { 2 } else { 1 },
            )
        } else {
            (
                frame.message.len(),
                frame.message.iter().map(|m| m.chars().count()).max().unwrap_or(0),
            )
        };
        let board_w = body_w;
        let status_w = if frame.message.is_empty() {
            frame.status.chars().count()
                + if lay.hint { KEYS_SHORT.chars().count() + 3 } else { 0 }
        } else {
            0
        };
        let footer_w = if lay.footer && frame.message.is_empty() {
            KEYS.chars().count()
        } else {
            0
        };
        // A message sits in its own small box, so it gets a margin the board
        // (which fills its frame edge to edge) does not want.
        let content_w = board_w.max(status_w).max(footer_w)
            + if frame.message.is_empty() { 0 } else { 4 };
        // The end screen says everything the status line and legend would, so it
        // stands alone.
        let content_h = if frame.message.is_empty() {
            body_rows + 1 + usize::from(lay.spacer) + usize::from(lay.footer)
        } else {
            body_rows
        };
        let pad = if lay.border { 2 } else { 0 };
        ((content_w + pad) as u16, (content_h + pad) as u16)
    }

    fn board_lines(frame: &crate::PacFrame, lay: Layout) -> Vec<Line<'static>> {
        // A message is literal text; the board is a map of tiles. Running the
        // one through the other's renderer turns words into ghosts.
        let mut out: Vec<Line> = if frame.message.is_empty() {
            frame
                .rows
                .iter()
                .map(|row| {
                    Line::from(
                        row.chars()
                            .map(|c| {
                                let (glyph, style) = tile(c, lay.wide);
                                Span::styled(glyph, style)
                            })
                            .collect::<Vec<Span>>(),
                    )
                })
                .collect()
        } else {
            frame
                .message
                .iter()
                .map(|m| {
                    Line::styled(
                        m.clone(),
                        Style::default()
                            .fg(Color::Rgb(255, 255, 0))
                            .add_modifier(Modifier::BOLD),
                    )
                })
                .collect()
        };
        if !frame.message.is_empty() {
            return out;
        }
        if lay.spacer {
            out.push(Line::from(""));
        }
        // With no room for the legend, the keys join the status line rather than
        // leaving the player nothing to read.
        let status = if lay.hint {
            format!("{}   {}", frame.status, KEYS_SHORT)
        } else {
            frame.status.clone()
        };
        out.push(Line::styled(
            status,
            Style::default()
                .fg(Color::Rgb(255, 255, 0))
                .add_modifier(Modifier::BOLD),
        ));
        if lay.footer {
            out.push(Line::styled(
                String::from(KEYS),
                Style::default().fg(Color::DarkGray),
            ));
        }
        out
    }

    fn draw(f: &mut ratatui::Frame, frame: &crate::PacFrame) {
        let area = f.area();
        for (border, spacer, footer, hint) in TRIMS {
            let lay = Layout { wide: true, border, spacer, footer, hint };
            for lay in [lay, Layout { wide: false, ..lay }] {
                let (w, h) = extent(frame, lay);
                if w > area.width || h > area.height {
                    continue;
                }
                let rect = Rect {
                    x: area.x + (area.width - w) / 2,
                    y: area.y + (area.height - h) / 2,
                    width: w,
                    height: h,
                };
                let body = Paragraph::new(board_lines(frame, lay)).alignment(Alignment::Center);
                let body = if lay.border {
                    body.block(Block::default().borders(Borders::ALL).title(" PAC-MAN "))
                } else {
                    body
                };
                f.render_widget(body, rect);
                return;
            }
        }
        let (w, h) = extent(
            frame,
            Layout { wide: false, border: false, spacer: false, footer: false, hint: false },
        );
        f.render_widget(
            Paragraph::new(format!(
                "pac-man needs {} columns by {} rows; this window is {} by {}",
                w, h, area.width, area.height
            ))
            .alignment(Alignment::Center),
            area,
        );
    }

    // A key name the engine's `keyOf` understands. Anything else maps to a
    // name the engine will reject, which is the same as no input.
    fn key_name(k: event::KeyEvent) -> String {
        if k.modifiers.contains(event::KeyModifiers::CONTROL) {
            if let event::KeyCode::Char('c') = k.code {
                return String::from("q");
            }
        }
        match k.code {
            event::KeyCode::Up => String::from("up"),
            event::KeyCode::Down => String::from("down"),
            event::KeyCode::Left => String::from("left"),
            event::KeyCode::Right => String::from("right"),
            event::KeyCode::Esc => String::from("Escape"),
            event::KeyCode::Char(c) => c.to_string(),
            _ => String::from(""),
        }
    }

    // Hold the final screen until the player dismisses it. Redrawing each pass
    // keeps it correct across a resize, and only `q` or Escape gets out, so a
    // stray keystroke cannot skip the tally.
    fn wait_for_quit(
        term: &mut Terminal<CrosstermBackend<std::fs::File>>,
        frame: &crate::PacFrame,
    ) {
        loop {
            let _ = term.draw(|f| draw(f, frame));
            if let Ok(true) = event::poll(std::time::Duration::from_millis(200)) {
                if let Ok(event::Event::Key(k)) = event::read() {
                    if k.kind != event::KeyEventKind::Press {
                        continue;
                    }
                    match key_name(k).as_str() {
                        "q" | "Escape" => return,
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn run(
        cmds: &crate::PacCommands,
        start: &crate::PacState,
    ) -> (bool, crate::PacState) {
        let screen = match Screen::enter() {
            Ok(s) => s,
            Err(e) => rustmorloc::morloc_throw(format!(
                "pacman: cannot take the terminal: {}. Run it from a tty.",
                e
            )),
        };
        let writer = match screen.tty.try_clone() {
            Ok(w) => w,
            Err(e) => rustmorloc::morloc_throw(format!("pacman: cannot write to the terminal: {}", e)),
        };
        let mut term = match Terminal::new(CrosstermBackend::new(writer)) {
            Ok(t) => t,
            Err(e) => rustmorloc::morloc_throw(format!("pacman: cannot drive the terminal: {}", e)),
        };

        let mut state = start.clone();
        let mut last = CMD_TICK;
        let save = loop {
            let frame = cmds.view.call1(&state);
            let _ = term.draw(|f| draw(f, &frame));
            if frame.done {
                // A game that ended on its own shows its tally; one the player
                // walked out of does not.
                if last != CMD_QUIT && last != CMD_SAVE {
                    wait_for_quit(&mut term, &frame);
                }
                break frame.save;
            }
            let cmd = match event::poll(std::time::Duration::from_millis(FRAME_MS)) {
                Ok(true) => match event::read() {
                    Ok(event::Event::Key(k)) if k.kind == event::KeyEventKind::Press => {
                        cmds.keyOf.call1(&key_name(k))
                    }
                    _ => CMD_TICK,
                },
                _ => CMD_TICK,
            };
            last = cmd;
            state = cmds.step.call2(&cmd, &state);
        };

        drop(screen);
        (save, state)
    }

    pub fn offscreen(
        cmds: &crate::PacCommands,
        state: &crate::PacState,
        width: i64,
        height: i64,
    ) -> Vec<String> {
        let w = width.clamp(1, 400) as u16;
        let h = height.clamp(1, 400) as u16;
        let mut term = match Terminal::new(TestBackend::new(w, h)) {
            Ok(t) => t,
            Err(e) => rustmorloc::morloc_throw(format!("pacman: off-screen render failed: {}", e)),
        };
        let frame = cmds.view.call1(state);
        if let Err(e) = term.draw(|f| draw(f, &frame)) {
            rustmorloc::morloc_throw(format!("pacman: off-screen render failed: {}", e));
        }
        let buf = term.backend().buffer();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| match buf.cell(Position::new(x, y)) {
                        Some(c) => c.symbol().to_string(),
                        None => String::from(" "),
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }
}

pub fn pac_tui_run(cmds: &PacCommands, start: &PacState) -> (bool, PacState) {
    pac_tui_impl::run(cmds, start)
}

pub fn pac_tui_frame(cmds: &PacCommands, state: &PacState, width: i64, height: i64) -> Vec<String> {
    pac_tui_impl::offscreen(cmds, state, width, height)
}

pub fn pac_tui_noop() {}
