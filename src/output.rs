//! Lines printed to the owner. Tests can capture them instead of printing.

use std::cell::RefCell;

thread_local! {
    static CAPTURE: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// Print one line on stdout, or record it while [`capture`] runs.
pub fn line(text: impl Into<String>) {
    let text = text.into();
    let kept = CAPTURE.with(|c| match c.borrow_mut().as_mut() {
        Some(lines) => {
            lines.extend(text.lines().map(str::to_string));
            true
        }
        None => false,
    });
    if !kept {
        println!("{text}");
    }
}

/// Run `f`, returning what it printed through [`line`] instead of printing it.
pub fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    let previous = CAPTURE.with(|c| c.borrow_mut().replace(Vec::new()));
    let result = f();
    let lines = CAPTURE.with(|c| std::mem::replace(&mut *c.borrow_mut(), previous));
    (result, lines.unwrap_or_default())
}

/// `say!("ok: {name}")`: [`line`] with `format!` arguments.
#[macro_export]
macro_rules! say {
    ($($arg:tt)*) => {
        $crate::output::line(format!($($arg)*))
    };
}
