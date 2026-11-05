//! Macro execution on the input thread. Restarting one button replaces only
//! that button's run; other buttons stay independent. Time is passed in, so
//! scheduling and cancellation are tested without sleeping.

use super::actions::{KeyStroke, MacroStep};
use super::validation::{self, ProfileDataError};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Text is sent in small pieces so a long snippet never holds up input.
pub const TEXT_CHUNK: usize = 16;

pub enum TextResult {
    Sent,
    /// The text went to a helper process; continue when `resume(token)` runs.
    Waiting(u64),
}

pub trait MacroOutput {
    fn send_key(&mut self, stroke: &KeyStroke);
    fn send_text(&mut self, text: &str) -> TextResult;
}

struct Run {
    steps: Vec<MacroStep>,
    index: usize,
    text: Vec<char>,
    text_offset: usize,
    due: Instant,
    waiting: Option<u64>,
}

#[derive(Default)]
pub struct MacroRunner {
    runs: BTreeMap<i32, Run>,
}

impl MacroRunner {
    pub fn active_buttons(&self) -> Vec<i32> {
        self.runs.keys().copied().collect()
    }

    pub fn is_idle(&self) -> bool {
        self.runs.is_empty()
    }

    pub fn start(&mut self, steps: &[MacroStep], button: i32, now: Instant) -> Result<(), ProfileDataError> {
        validation::validate_macro(steps)?;
        self.runs.insert(
            button,
            Run { steps: steps.to_vec(), index: 0, text: Vec::new(), text_offset: 0, due: now, waiting: None },
        );
        Ok(())
    }

    /// Returns true when something was running.
    pub fn cancel_all(&mut self) -> bool {
        let had = !self.runs.is_empty();
        self.runs.clear();
        had
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.runs.values().filter(|r| r.waiting.is_none()).map(|r| r.due).min()
    }

    pub fn resume(&mut self, token: u64, now: Instant) {
        for run in self.runs.values_mut() {
            if run.waiting == Some(token) {
                run.waiting = None;
                run.due = now;
            }
        }
    }

    /// Advances every due run by one step. Returns true when a run ended.
    pub fn tick(&mut self, now: Instant, output: &mut dyn MacroOutput) -> bool {
        let due: Vec<i32> = self.runs.iter().filter(|(_, r)| r.waiting.is_none() && r.due <= now).map(|(b, _)| *b).collect();
        let mut finished = false;
        for button in due {
            let Some(run) = self.runs.get_mut(&button) else { continue };
            if run.index >= run.steps.len() {
                self.runs.remove(&button);
                finished = true;
                continue;
            }
            let step = run.steps[run.index].clone();
            match step.kind.as_str() {
                "text" => {
                    if run.text.is_empty() && run.text_offset == 0 {
                        run.text = step.text.as_deref().unwrap_or("").chars().collect();
                    }
                    let end = (run.text_offset + TEXT_CHUNK).min(run.text.len());
                    let mut waiting = None;
                    if end > run.text_offset {
                        let chunk: String = run.text[run.text_offset..end].iter().collect();
                        if let TextResult::Waiting(token) = output.send_text(&chunk) {
                            waiting = Some(token);
                        }
                    }
                    run.text_offset = end;
                    if end >= run.text.len() {
                        run.text.clear();
                        run.text_offset = 0;
                        run.index += 1;
                    }
                    run.waiting = waiting;
                    run.due = now;
                }
                "key" => {
                    run.index += 1;
                    if let Some(key) = &step.key_stroke {
                        output.send_key(key);
                    }
                    run.due = now;
                }
                _ => {
                    run.index += 1;
                    let delay = step.delay_ms.unwrap_or(0).max(0) as u64;
                    run.due = now + Duration::from_millis(delay);
                }
            }
        }
        finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::keys::mac;

    #[derive(Default)]
    struct Recorder {
        log: Vec<String>,
        wait: bool,
    }

    impl MacroOutput for Recorder {
        fn send_key(&mut self, stroke: &KeyStroke) {
            self.log.push(format!("key {}", stroke.key));
        }
        fn send_text(&mut self, text: &str) -> TextResult {
            self.log.push(format!("text {text}"));
            if self.wait {
                TextResult::Waiting(7)
            } else {
                TextResult::Sent
            }
        }
    }

    fn drain(runner: &mut MacroRunner, out: &mut Recorder, now: Instant) {
        for _ in 0..1000 {
            if runner.next_deadline().map(|d| d > now).unwrap_or(true) {
                break;
            }
            runner.tick(now, out);
        }
    }

    #[test]
    fn runs_steps_in_order_with_delays() {
        let t0 = Instant::now();
        let mut runner = MacroRunner::default();
        let mut out = Recorder::default();
        let steps = vec![
            MacroStep::key(KeyStroke::new("a", &[], Some(mac::A))),
            MacroStep::delay(100),
            MacroStep::text("hi"),
        ];
        runner.start(&steps, 1, t0).unwrap();
        drain(&mut runner, &mut out, t0);
        assert_eq!(out.log, vec!["key a"]);
        assert_eq!(runner.next_deadline(), Some(t0 + Duration::from_millis(100)));
        drain(&mut runner, &mut out, t0 + Duration::from_millis(99));
        assert_eq!(out.log.len(), 1);
        drain(&mut runner, &mut out, t0 + Duration::from_millis(100));
        assert_eq!(out.log, vec!["key a", "text hi"]);
        assert!(runner.is_idle());
    }

    #[test]
    fn restart_replaces_only_that_button() {
        let t0 = Instant::now();
        let mut runner = MacroRunner::default();
        let mut out = Recorder::default();
        let long = vec![MacroStep::delay(1000), MacroStep::text("late")];
        runner.start(&long, 1, t0).unwrap();
        runner.start(&long, 2, t0).unwrap();
        drain(&mut runner, &mut out, t0);
        runner.start(&[MacroStep::text("again")], 1, t0).unwrap();
        drain(&mut runner, &mut out, t0 + Duration::from_millis(1000));
        assert_eq!(out.log, vec!["text again", "text late"]);
        assert!(runner.is_idle());
    }

    #[test]
    fn cancel_stops_everything() {
        let t0 = Instant::now();
        let mut runner = MacroRunner::default();
        let mut out = Recorder::default();
        runner.start(&[MacroStep::delay(10), MacroStep::text("never")], 3, t0).unwrap();
        drain(&mut runner, &mut out, t0);
        assert!(runner.cancel_all());
        assert!(!runner.cancel_all());
        drain(&mut runner, &mut out, t0 + Duration::from_secs(1));
        assert!(out.log.is_empty());
    }

    #[test]
    fn long_text_is_chunked_without_splitting_characters() {
        let t0 = Instant::now();
        let mut runner = MacroRunner::default();
        let mut out = Recorder::default();
        let text: String = "😀".repeat(20) + "abc";
        runner.start(&[MacroStep::text(&text)], 1, t0).unwrap();
        runner.tick(t0, &mut out);
        assert_eq!(out.log.len(), 1, "one chunk per tick");
        drain(&mut runner, &mut out, t0);
        let joined: String = out.log.iter().map(|l| l.trim_start_matches("text ")).collect();
        assert_eq!(joined, text);
        assert_eq!(out.log.len(), 2);
    }

    #[test]
    fn waiting_on_a_helper_pauses_the_run() {
        let t0 = Instant::now();
        let mut runner = MacroRunner::default();
        let mut out = Recorder { wait: true, ..Default::default() };
        runner.start(&[MacroStep::text("x"), MacroStep::key(KeyStroke::new("b", &[], Some(mac::B)))], 1, t0).unwrap();
        drain(&mut runner, &mut out, t0);
        assert_eq!(out.log, vec!["text x"]);
        assert_eq!(runner.next_deadline(), None);
        runner.resume(7, t0);
        drain(&mut runner, &mut out, t0);
        assert_eq!(out.log, vec!["text x", "key b"]);
    }

    #[test]
    fn invalid_macros_do_not_start() {
        let mut runner = MacroRunner::default();
        assert!(runner.start(&[MacroStep::delay(-5)], 1, Instant::now()).is_err());
        assert!(runner.is_idle());
    }
}
