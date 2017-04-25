use std::io;
use std::mem;
use std::os::windows::io::AsRawHandle;

use winapi::{HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE,
             CONSOLE_SCREEN_BUFFER_INFO, COORD, SMALL_RECT};
use kernel32::{GetStdHandle, GetConsoleScreenBufferInfo,
               SetConsoleCursorPosition};

use term::Term;

pub const DEFAULT_WIDTH: u16 = 79;


pub fn is_a_terminal() -> bool {
    unsafe {
        let handle = kernel32::GetStdHandle(STD_OUTPUT_HANDLE);
        let mut out = 0;
        kernel32::GetConsoleMode(handle, &mut out) != 0
    }
}

pub fn terminal_size() -> Option<(u16, u16)> {
    if let Some((_, csbi)) = get_console_screen_buffer_info(TermOutput::Stdout) {
        Some(((csbi.srWindow.Right - csbi.srWindow.Left) as u16,
              (csbi.srWindow.Bottom - csbi.srWindow.Top) as u16))
    } else {
        None
    }
}

pub fn move_cursor_up(out: &Term, n: usize) -> io::Result<()> {
    if let Some((hand, csbi)) = get_console_screen_buffer_info(out) {
        unsafe {
            SetConsoleCursorPosition(hand, COORD {
                X: 0,
                Y: csbi.dwCursorPosition.Y - n as i16,
            });
        }
    }
    Ok(())
}

pub fn move_cursor_down(out: &Term, n: usize) -> io::Result<()> {
    if let Some((hand, csbi)) = get_console_screen_buffer_info(out) {
        unsafe {
            SetConsoleCursorPosition(hand, COORD {
                X: 0,
                Y: csbi.dwCursorPosition.Y + n as i16,
            });
        }
    }
    Ok(())
}

pub fn clear_line(out: &Term) -> io::Result<()> {
    if let Some((hand, csbi)) = get_console_screen_buffer_info(out) {
        out.write_bytes(format!("\r{0:width$}\r", "", width=
            csbi.srWindow.Right - csbi.srWindow.Left).as_bytes())
    }
    Ok(())
}

fn get_console_screen_buffer_info(out: &Term)
    -> Option<(HANDLE, CONSOLE_SCREEN_BUFFER_INFO)>
{
    let hand: HANDLE = unsafe { GetStdHandle(out.as_raw_handle()) };
    let mut csbi: CONSOLE_SCREEN_BUFFER_INFO = unsafe { mem::zeroed() };
    match unsafe { GetConsoleScreenBufferInfo(hand, &mut csbi) } {
        0 => None,
        _ => Some((hand, csbi)),
    }
}
