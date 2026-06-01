use std::cell::Cell;
use std::io::{self, Write};

thread_local! {
    static LAST_SHOWN: Cell<(usize, usize, u8)> = const { Cell::new((usize::MAX, 0, 255)) };
}

fn percent(done: usize, total: usize) -> u8 {
    if total == 0 {
        100
    } else {
        ((done as u64 * 100) / total as u64).min(100) as u8
    }
}

/// In-place stderr progress for long audits (`trusted scan`).
pub fn print_audit_progress(done: usize, total: usize, phase: &str) {
    let done = done.min(total);
    let pct = percent(done, total);
    let should_draw = LAST_SHOWN.with(|last| {
        let prev = last.get();
        let draw = prev.0 != done || prev.1 != total || prev.2 != pct || done == total;
        if draw {
            last.set((done, total, pct));
        }
        draw
    });
    if !should_draw {
        return;
    }
    let mut err = io::stderr();
    let _ = write!(err, "\rtrusted: {done}/{total} packages ({pct}%) — {phase}");
    let _ = err.flush();
}

pub fn print_audit_progress_done() {
    let mut err = io::stderr();
    let _ = writeln!(err);
    let _ = err.flush();
}

#[cfg(test)]
mod tests {
    use super::percent;

    #[test]
    fn percent_rounds_down() {
        assert_eq!(percent(1, 4), 25);
        assert_eq!(percent(250, 250), 100);
        assert_eq!(percent(0, 0), 100);
    }
}
