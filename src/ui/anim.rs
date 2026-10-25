//! Animation primitives: the spring used for the notch shape and paging, easing, and the
//! per-control hover/press animation.

use super::*;

pub(super) struct Spring {
    pub(super) v: f32,
    vel: f32,
}

impl Spring {
    pub(super) fn new(v: f32) -> Self {
        Spring { v, vel: 0.0 }
    }
    /// Slightly under-damped spring; returns true while still moving.
    pub(super) fn step(&mut self, target: f32, dt: f32) -> bool {
        let (k, c) = (230.0, 24.0);
        let mut t = 0.0;
        while t < dt {
            let h = (dt - t).min(1.0 / 240.0);
            let a = k * (target - self.v) - c * self.vel;
            self.vel += a * h;
            self.v += self.vel * h;
            t += h;
        }
        if (target - self.v).abs() < 0.05 && self.vel.abs() < 0.05 {
            self.v = target;
            self.vel = 0.0;
            return false;
        }
        true
    }
}

/// Blends two 0xRRGGBB colours (t = 0 gives `a`, 1 gives `b`).
pub(super) fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 255) as f32, ((b >> s) & 255) as f32);
        ((x + (y - x) * t).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

pub(super) fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}


/// Animation state of one control (hover highlight, press pop, animated on/off level).
struct Ctl {
    act: Act,
    hover: f32,
    press: f32,
    level: f32,
    target: f32,
}

/// Animations of the panel's controls, keyed by their action. Only controls that are
/// hovered, pressed or have an on/off level are kept, so the list stays tiny.
#[derive(Default)]
pub(super) struct Fx {
    ctl: RefCell<Vec<Ctl>>,
}

impl Fx {
    fn get<R>(&self, act: Act, f: impl FnOnce(&Ctl) -> R) -> Option<R> {
        self.ctl.borrow().iter().find(|c| c.act == act).map(f)
    }

    /// Eased hover amount 0..1.
    pub(super) fn hover(&self, act: Act) -> f32 {
        self.get(act, |c| ease(c.hover)).unwrap_or(0.0)
    }

    /// Press "pop": 0 at rest, rises and falls once after a click.
    pub(super) fn press(&self, act: Act) -> f32 {
        self.get(act, |c| (c.press * std::f32::consts::PI).sin()).unwrap_or(0.0)
    }

    pub(super) fn pulse(&self, act: Act) {
        self.touch(act).press = 1.0;
    }

    /// Smoothly follows `target` (e.g. 0/1 for a switch); starts at the first target seen.
    pub(super) fn level(&self, act: Act, target: f32) -> f32 {
        let mut v = self.ctl.borrow_mut();
        match v.iter_mut().find(|c| c.act == act) {
            Some(c) => {
                if c.level.is_nan() {
                    c.level = target;
                }
                c.target = target;
                c.level
            }
            None => {
                v.push(Ctl { act, hover: 0.0, press: 0.0, level: target, target });
                target
            }
        }
    }

    fn touch(&self, act: Act) -> std::cell::RefMut<'_, Ctl> {
        let mut v = self.ctl.borrow_mut();
        let i = match v.iter().position(|c| c.act == act) {
            Some(i) => i,
            None => {
                v.push(Ctl { act, hover: 0.0, press: 0.0, level: f32::NAN, target: f32::NAN });
                v.len() - 1
            }
        };
        std::cell::RefMut::map(v, |v| &mut v[i])
    }

    /// Advances all animations; returns true while any of them still moves.
    pub(super) fn step(&self, hot: Option<Act>, dt: f32) -> bool {
        if let Some(h) = hot {
            drop(self.touch(h));
        }
        let mut moving = false;
        let mut v = self.ctl.borrow_mut();
        for c in v.iter_mut() {
            let to = if Some(c.act) == hot { 1.0 } else { 0.0 };
            // ~140 ms in, ~220 ms out
            c.hover = if to > c.hover { (c.hover + dt / 0.14).min(1.0) } else { (c.hover - dt / 0.22).max(0.0) };
            c.press = (c.press - dt / 0.26).max(0.0);
            if !c.level.is_nan() {
                c.level += (c.target - c.level) * (1.0 - (-dt * 16.0).exp());
                if (c.target - c.level).abs() < 0.002 {
                    c.level = c.target;
                }
            }
            moving |= c.hover != to || c.press > 0.0 || c.level != c.target;
        }
        v.retain(|c| c.hover > 0.0 || c.press > 0.0 || !c.level.is_nan());
        moving
    }
}
