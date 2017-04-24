use std::io;
use std::mem;

use libc;

use term::Term;

pub fn terminal_size() -> Option<(u16, u16)> {
    let is_tty = unsafe {
        libc::isatty(libc::STDOUT_FILENO) == 1
    };

    if !is_tty {
        return None;
    }

    unsafe {
        let mut winsize: libc::winsize = mem::zeroed();
        libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut winsize);
        if winsize.ws_row > 0 && winsize.ws_col > 0 {
            return Some((winsize.ws_row as u16, winsize.ws_col as u16));
        }
    }

    None
}

pub fn move_cursor_down(out: &Term, n: usize) -> io::Result<()> {
    if n > 0 {
        out.write_bytes(format!("\x1b[{}B", n).as_bytes())
    } else {
        Ok(())
    }
}

pub fn move_cursor_up(out: &Term, n: usize) -> io::Result<()> {
    if n > 0 {
        out.write_bytes(format!("\x1b[{}A", n).as_bytes())
    } else {
        Ok(())
    }
}

pub fn clear_line(out: &Term) -> io::Result<()> {
    out.write_bytes(format!("\r\x1b[2K").as_bytes())
}

pub fn hide_cursor(out: &Term) -> io::Result<()> {
    out.write_bytes(b"\x1b[?25l")
}

pub fn show_cursor(out: &Term) -> io::Result<()> {
    out.write_bytes(b"\x1b[?25h")
}
