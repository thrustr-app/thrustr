// SPDX-License-Identifier: GPL-3.0-or-later
//
// Adapted from gpui-kit,
// Copyright (C) Longbridge, licensed under Apache-2.0:
// https://github.com/longbridge/gpui-kit/blob/main/crates/base/src/input/base/blink_cursor.rs
//
// Modified and redistributed as part of Thrustr under GPL-3.0-or-later.

use gpui::{Context, Task};
use std::time::Duration;

const INTERVAL: Duration = Duration::from_millis(500);
const PAUSE_DELAY: Duration = Duration::from_millis(500);

pub struct Cursor {
    visible: bool,
    blink_task: Option<Task<()>>,
}

impl Cursor {
    pub fn new() -> Self {
        Self {
            visible: true,
            blink_task: None,
        }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn start(&mut self, cx: &mut Context<Self>) {
        self.show_and_blink_after(INTERVAL, cx);
    }

    pub fn stop(&mut self) {
        self.blink_task = None;
        self.visible = false;
    }

    pub fn pause(&mut self, cx: &mut Context<Self>) {
        if self.blink_task.is_some() {
            self.show_and_blink_after(PAUSE_DELAY, cx);
        }
    }

    fn show_and_blink_after(&mut self, delay: Duration, cx: &mut Context<Self>) {
        self.visible = true;
        cx.notify();

        self.blink_task = Some(cx.spawn(async move |this, cx| {
            let mut delay = delay;
            loop {
                cx.background_executor().timer(delay).await;
                let toggled = this.update(cx, |this, cx| {
                    this.visible = !this.visible;
                    cx.notify();
                });
                if toggled.is_err() {
                    break;
                }
                delay = INTERVAL;
            }
        }));
    }
}
