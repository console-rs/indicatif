use std::io;
use std::mem;
use std::os::unix::io::{AsRawFd, RawFd};

use libc;

use term::Term;

pub const DEFAULT_WIDTH: u16 = 80;


#[inline(always)]
pub fn is_a_terminal(out: &Term) -> bool {
    unsafe {
        libc::isatty(out.as_raw_fd()) == 1
    }
}

pub fn terminal_size() -> Option<(u16, u16)> {
    unsafe {
        if libc::isatty(libc::STDOUT_FILENO) != 1 {
            return None;
        }

        let mut winsize: libc::winsize = mem::zeroed();
        libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut winsize);
        if winsize.ws_row > 0 && winsize.ws_col > 0 {
            Some((winsize.ws_row as u16, winsize.ws_col as u16))
        } else {
            None
        }
    }
}

pub fn move_cursor_down(out: &Term, n: usize) -> io::Result<()> {
    if n > 0 {
        out.write_str(&format!("\x1b[{}B", n))
    } else {
        Ok(())
    }
}

pub fn move_cursor_up(out: &Term, n: usize) -> io::Result<()> {
    if n > 0 {
        out.write_str(&format!("\x1b[{}A", n))
    } else {
        Ok(())
    }
}

pub fn clear_line(out: &Term) -> io::Result<()> {
    out.write_str(&format!("\r\x1b[2K"))
}
