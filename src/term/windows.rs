use std::io;
use std::mem;
use std::os::windows::io::{RawHandle, AsRawHandle};

use winapi::{CHAR, DWORD, HANDLE, STD_OUTPUT_HANDLE,
             CONSOLE_SCREEN_BUFFER_INFO, COORD};
use kernel32::{GetStdHandle, GetConsoleScreenBufferInfo,
               GetConsoleMode, SetConsoleCursorPosition,
               FillConsoleOutputCharacterA};

use term::Term;

pub const DEFAULT_WIDTH: u16 = 79;


pub fn is_a_terminal() -> bool {
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut out = 0;
        GetConsoleMode(handle, &mut out) != 0
    }
}

pub fn terminal_size() -> Option<(u16, u16)> {
    let hand = STD_OUTPUT_HANDLE as RawHandle;
    if let Some((_, csbi)) = get_console_screen_buffer_info(hand) {
        Some(((csbi.srWindow.Right - csbi.srWindow.Left) as u16,
              (csbi.srWindow.Bottom - csbi.srWindow.Top) as u16))
    } else {
        None
    }
}

pub fn move_cursor_up(out: &Term, n: usize) -> io::Result<()> {
    if let Some((hand, csbi)) = get_console_screen_buffer_info(out.as_raw_handle()) {
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
    if let Some((hand, csbi)) = get_console_screen_buffer_info(out.as_raw_handle()) {
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
    if let Some((hand, csbi)) = get_console_screen_buffer_info(out.as_raw_handle()) {
        unsafe {
            let width = csbi.srWindow.Right - csbi.srWindow.Left;
            let pos = COORD {
                X: 0,
                Y: csbi.dwCursorPosition.Y,
            };
            let mut written = 0;
            FillConsoleOutputCharacterA(hand, b' ' as CHAR,
                                        width as DWORD, pos, &mut written);
        }
    }
    Ok(())
}

fn get_console_screen_buffer_info(hand: HANDLE)
    -> Option<(HANDLE, CONSOLE_SCREEN_BUFFER_INFO)>
{
    let mut csbi: CONSOLE_SCREEN_BUFFER_INFO = unsafe { mem::zeroed() };
    match unsafe { GetConsoleScreenBufferInfo(hand, &mut csbi) } {
        0 => None,
        _ => Some((hand, csbi)),
    }
}
