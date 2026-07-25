// SPDX-License-Identifier: GPL-3.0-or-later
//
// Parts of this module are adapted from the gpui text-input example,
// Copyright (C) Zed Industries, Inc., licensed under Apache-2.0:
// https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/input.rs
// Modified and redistributed as part of Thrustr under GPL-3.0-or-later.

use gpui::SharedString;

pub struct InputEvent {
    pub value: SharedString,
}

pub struct ChangeEvent {
    pub value: SharedString,
}
