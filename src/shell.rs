//! Ctrl-F1 Shell: `1 Go to Shell; 2 Shell Command: 0`.
//! 1 opens a Windows command prompt in the current folder (in its own
//! window, so the document stays open); 2 runs one command and shows what
//! it printed, like WP 5.1's "DOS Command".

use crate::grid::*;
use crate::keys::Input;
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};

pub fn menu(wp: &mut Wp) {
    wp.push(MenuLine::new(&["~Go to Shell", "Shell ~Command"], |wp, k| {
        if k == 0 {
            go_to_shell(wp);
            return None;
        }
        Some(Prompt::new("Shell Command:", "", |wp, s| {
            let cmd = s?;
            if cmd.trim().is_empty() {
                return None;
            }
            let out = run_command(wp, &cmd);
            Some(Box::new(Output { cmd, lines: out.lines().map(|l| l.to_string()).collect(), scroll: 0 }) as Box<dyn Screen>)
        }))
    }));
}

/// Folder the shell starts in: the document's folder, else the default.
fn shell_dir(wp: &Wp) -> std::path::PathBuf {
    wp.ed_ref().path.as_ref().and_then(|p| p.parent()).filter(|p| p.is_dir()).map(|p| p.to_path_buf()).unwrap_or_else(|| wp.cwd.clone())
}

fn go_to_shell(wp: &mut Wp) {
    let dir = shell_dir(wp);
    #[cfg(windows)]
    let r = std::process::Command::new("cmd.exe")
        .args(["/C", "start", "Blue Screen Writer 51 - Shell", "cmd.exe", "/K", "echo Type EXIT to close this window."])
        .current_dir(&dir)
        .spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new("x-terminal-emulator").current_dir(&dir).spawn();
    match r {
        Ok(_) => wp.flash("Shell opened in its own window -- type EXIT there to close it"),
        Err(e) => wp.flash(&format!("ERROR: Can't start the shell -- {e}")),
    }
}

/// Run `cmd` and return its output (stdout + stderr).
pub fn run_command(wp: &Wp, cmd: &str) -> String {
    let dir = shell_dir(wp);
    #[cfg(windows)]
    let r = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd.exe").arg("/C").raw_arg(cmd).current_dir(&dir).creation_flags(CREATE_NO_WINDOW).output()
    };
    #[cfg(not(windows))]
    let r = std::process::Command::new("sh").arg("-c").arg(cmd).current_dir(&dir).output();
    match r {
        Ok(o) => {
            let mut s = decode(&o.stdout);
            let e = decode(&o.stderr);
            if !e.is_empty() {
                if !s.is_empty() && !s.ends_with('\n') {
                    s.push('\n');
                }
                s.push_str(&e);
            }
            s.replace("\r\n", "\n")
        }
        Err(e) => format!("ERROR: {e}"),
    }
}

/// Console output is UTF-8 or the OEM code page (850 / 437).
fn decode(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&c| if c < 0x80 { c as char } else { CP850_HIGH[(c - 0x80) as usize] }).collect(),
    }
}

const CP850_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', 'ø', '£',
    'Ø', '×', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '®', '¬', '½', '¼', '¡', '«', '»', '░', '▒', '▓', '│', '┤', 'Á', 'Â', 'À', '©', '╣',
    '║', '╗', '╝', '¢', '¥', '┐', '└', '┴', '┬', '├', '─', '┼', 'ã', 'Ã', '╚', '╔', '╩', '╦', '╠', '═', '╬', '¤', 'ð', 'Ð', 'Ê', 'Ë', 'È', 'ı', 'Í',
    'Î', 'Ï', '┘', '┌', '█', '▄', '¦', 'Ì', '▀', 'Ó', 'ß', 'Ô', 'Ò', 'õ', 'Õ', 'µ', 'þ', 'Þ', 'Ú', 'Û', 'Ù', 'ý', 'Ý', '¯', '´', '\u{AD}', '±', '‗',
    '¾', '¶', '§', '÷', '¸', '°', '¨', '·', '¹', '³', '²', '■', '\u{A0}',
];

/// The command's output; any key returns (PgUp/PgDn/arrows scroll first).
struct Output {
    cmd: String,
    lines: Vec<String>,
    scroll: usize,
}

impl Screen for Output {
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        use crate::keys::Func;
        let page = wp.rows.saturating_sub(4).max(1);
        let max = self.lines.len().saturating_sub(page);
        let cur = self.scroll;
        let next = match i {
            Input::Func(Func::Down) => Some((cur + 1).min(max)),
            Input::Func(Func::Up) => Some(cur.saturating_sub(1)),
            Input::Func(Func::PgDn | Func::ScreenDown) => Some((cur + page).min(max)),
            Input::Func(Func::PgUp | Func::ScreenUp) => Some(cur.saturating_sub(page)),
            _ => None,
        };
        match next {
            Some(n) => {
                self.scroll = n;
                Outcome::Keep
            }
            None => Outcome::Close,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        g.text(0, 0, &format!("Shell Command: {}", self.cmd), wp.theme.bold);
        let page = g.rows.saturating_sub(4);
        let start = self.scroll;
        for (k, l) in self.lines.iter().skip(start).take(page).enumerate() {
            let t: String = l.chars().take(g.cols).collect();
            g.text(0, 2 + k, &t, st);
        }
        let r = g.rows - 1;
        let more = if self.lines.len() > page { "  (PgUp/PgDn to scroll)" } else { "" };
        let c = g.text(0, r, &format!("Press any key to continue{more}"), st);
        g.cursor = Some((c, r));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Func;

    #[test]
    fn shell_command_shows_output() {
        let mut wp = Wp::new(80, 25);
        wp.cwd = std::env::temp_dir();
        wp.input(Input::Func(Func::Shell));
        wp.input(Input::Char('2'));
        for c in "echo hallo-shell".chars() {
            wp.input(Input::Char(c));
        }
        wp.input(Input::Func(Func::Enter));
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let s: String = (0..25).map(|r| (0..80).map(|c| g.get(c, r).unwrap().ch).collect::<String>() + "\n").collect();
        assert!(s.contains("hallo-shell") && s.contains("Press any key to continue"), "{s}");
        wp.input(Input::Char('x'));
        assert!(!wp.has_screens());
    }
}
