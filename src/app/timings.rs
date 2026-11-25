use std::time::{Duration, Instant};

pub(super) struct Timings {
    start: Option<Instant>,
    phases: Vec<(&'static str, Duration)>,
}

impl Timings {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            start: enabled.then(Instant::now),
            phases: Vec::new(),
        }
    }

    pub(super) fn measure<T>(&mut self, phase: &'static str, run: impl FnOnce() -> T) -> T {
        let start = self.start.map(|_| Instant::now());
        let result = run();
        if let Some(start) = start {
            self.phases.push((phase, start.elapsed()));
        }
        result
    }
}

impl Drop for Timings {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let phases = self
                .phases
                .iter()
                .map(|(phase, elapsed)| format!("{phase}={}", elapsed.as_millis()))
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!(
                "Timings (ms): {phases} total={}",
                start.elapsed().as_millis()
            );
        }
    }
}
