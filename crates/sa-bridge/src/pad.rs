//! Controller-state injection for pad 0. Applied right after CPad::UpdatePads each in-game
//! frame, so it works without window focus and does not touch the real keyboard.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

use proto::{InputStep, PAD_FIELDS};

use crate::addr::PADS;

struct Queue {
    steps: VecDeque<InputStep>,
    started: Option<Instant>,
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue { steps: VecDeque::new(), started: None });

pub fn set(steps: Vec<InputStep>, append: bool) -> u32 {
    let mut q = QUEUE.lock().unwrap();
    if !append {
        q.steps.clear();
        q.started = None;
    }
    q.steps.extend(steps);
    q.steps.iter().map(|s| s.ms).sum()
}

pub fn clear() {
    let mut q = QUEUE.lock().unwrap();
    q.steps.clear();
    q.started = None;
}

pub fn pending_ms() -> u32 {
    let q = QUEUE.lock().unwrap();
    let elapsed = q.started.map(|s| s.elapsed().as_millis() as u32).unwrap_or(0);
    q.steps.iter().map(|s| s.ms).sum::<u32>().saturating_sub(elapsed)
}

/// Game thread, after CPad::UpdatePads.
pub unsafe fn apply() {
    let Ok(mut q) = QUEUE.try_lock() else { return };
    loop {
        let started = *q.started.get_or_insert_with(Instant::now);
        let Some(step) = q.steps.front() else {
            q.started = None;
            return;
        };
        if started.elapsed().as_millis() as u32 >= step.ms {
            q.steps.pop_front();
            q.started = None;
            continue;
        }
        let new_state = PADS as *mut i16;
        for &(field, value) in &step.fields {
            if (field as usize) < PAD_FIELDS {
                *new_state.add(field as usize) = value;
            }
        }
        return;
    }
}
