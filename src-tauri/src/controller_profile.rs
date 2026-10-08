//! TASOLLER PLUS input: interrupt endpoint 0x84, DBT + buttons + 32 pressures.
//! Physical order documented by chuniio-rs: top/bottom, right to left.
//! https://gitea.tendokyu.moe/beerpsi/chuniio-rs/src/branch/trunk/src/backends/tasoller_plus.rs

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action { Left, Right, Back, Confirm }

// Same pressure scale as the controller's game IO (factory game threshold: 20).
const PRESS_THRESHOLD: u8 = 20;

// Left / Right / Back / Confirm, shared with the colored physical-order hints.
const ZONE_COLORS: [[u8; 3]; 4] = [[255, 255, 0], [255, 128, 32], [160, 64, 176], [255, 160, 192]];

/// DL v2: 31 alternating pad/divider RGB pixels, right to left, then towers.
pub fn tasoller_lights(zones: [bool; 4]) -> [u8; 114] {
    let mut packet = [0; 114];
    packet[..3].copy_from_slice(b"DL\x02");
    for pixel in 0..31 {
        let column = 15 - pixel / 2;
        // Leave the divider between each pair of quarters dark.
        if pixel % 2 == 1 && column % 4 == 0 { continue; }
        let zone = column / 4;
        let brightness = if zones[zone] { 2 } else { 1 };
        let rgb = ZONE_COLORS[zone].map(|c| (c as u16 * brightness / 3) as u8);
        packet[3 + pixel * 3..6 + pixel * 3].copy_from_slice(&rgb);
    }
    packet
}

/// MI_02 mirrors DBT game state inside a 65-byte HID report (report ID 0).
pub fn hid_fn_buttons(report: &[u8]) -> Option<u8> {
    if report.len() != 65 || report[0] != 0 || &report[1..4] != b"DBT" { return None; }
    Some(report[4] & 0xc0)
}

#[derive(Default)]
pub struct StopCombo { armed: bool }

impl StopCombo {
    pub fn update(&mut self, buttons: u8) -> bool {
        let buttons = buttons & 0xc0;
        if buttons == 0 { self.armed = true; }
        if buttons == 0xc0 && self.armed {
            self.armed = false;
            return true;
        }
        false
    }
}

pub fn tasoller_zones(packet: &[u8]) -> Option<[bool; 4]> {
    if packet.len() != 36 || &packet[..3] != b"DBT" { return None; }
    let mut zones = [false; 4];
    for (sensor, pressure) in packet[4..].iter().enumerate() {
        zones[3 - sensor / 8] |= *pressure >= PRESS_THRESHOLD;
    }
    Some(zones)
}

#[derive(Default)]
pub struct ZoneEdges {
    previous: [bool; 4],
    armed: bool,
}

impl ZoneEdges {
    pub fn update(&mut self, zones: [bool; 4]) -> Option<Action> {
        let previous = std::mem::replace(&mut self.previous, zones);
        let active = zones.iter().filter(|&&z| z).count();
        if active == 0 { self.armed = true; return None; }
        if active > 1 { self.armed = false; return None; }
        if !self.armed { return None; }
        let index = zones.iter().position(|&z| z)?;
        if previous[index] { return None; }
        self.armed = false;
        Some([Action::Left, Action::Right, Action::Back, Action::Confirm][index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet() -> [u8; 36] {
        let mut p = [0; 36];
        p[..3].copy_from_slice(b"DBT");
        p
    }

    #[test]
    fn lighting_matches_physical_quarters_and_reacts_only_to_touched_zone() {
        let idle = tasoller_lights([false; 4]);
        assert_eq!(&idle[..3], b"DL\x02");
        assert_eq!(idle.len(), 114);
        for column in 0..16 {
            let pixel = 3 + (30 - column * 2) * 3;
            let expected = ZONE_COLORS[column / 4].map(|c| c / 3);
            assert_eq!(&idle[pixel..pixel + 3], &expected);
        }
        // Three dividers between quarters stay dark; towers are unassigned.
        for pixel in [7, 15, 23] { assert_eq!(&idle[3 + pixel * 3..6 + pixel * 3], &[0; 3]); }
        assert_eq!(&idle[96..], &[0; 18]);
        let touched = tasoller_lights([true, false, false, false]);
        assert_eq!(&touched[3..75], &idle[3..75]);
        assert_eq!(&touched[93..96], &[170, 170, 0]);
    }

    #[test]
    fn hid_stop_ignores_diagnostics_air_and_single_fn() {
        let mut report = [0; 65];
        report[1..4].copy_from_slice(b"DBT");
        let mut combo = StopCombo::default();
        assert!(!combo.update(hid_fn_buttons(&report).unwrap()));
        for buttons in [0x3f, 0x80, 0x40, 0] {
            report[4] = buttons;
            assert!(!combo.update(hid_fn_buttons(&report).unwrap()));
        }
        report[4] = 0xc0;
        assert!(combo.update(hid_fn_buttons(&report).unwrap()));
        assert!(!combo.update(hid_fn_buttons(&report).unwrap()));
        report[1] = 0xe0;
        assert_eq!(hid_fn_buttons(&report), None);
        assert_eq!(hid_fn_buttons(&report[..36]), None);
    }

    #[test]
    fn stop_combo_needs_release_on_start_and_after_each_stop() {
        let mut combo = StopCombo::default();
        assert!(!combo.update(0xc0));
        assert!(!combo.update(0));
        assert!(!combo.update(0x80));
        assert!(combo.update(0xc0));
        assert!(!combo.update(0x40));
        assert!(!combo.update(0xc0));
        assert!(!combo.update(0));
        assert!(combo.update(0xc0));
    }

    #[test]
    fn both_rows_map_to_the_four_physical_quarters() {
        for sensor in 0..32 {
            let mut p = packet();
            p[4 + sensor] = 255;
            let mut expected = [false; 4];
            expected[3 - sensor / 8] = true;
            assert_eq!(tasoller_zones(&p), Some(expected), "sensor {sensor}");
        }
    }

    #[test]
    fn rejects_invalid_frames_and_ignores_air_and_fn() {
        assert_eq!(tasoller_zones(&[0; 36]), None);
        assert_eq!(tasoller_zones(b"DBT"), None);
        let mut p = packet();
        p[3] = 255;
        assert_eq!(tasoller_zones(&p), Some([false; 4]));
        assert_eq!(tasoller_zones(&[0; 37]), None);
    }

    #[test]
    fn pressure_threshold_filters_noise() {
        let mut p = packet();
        p[4] = 19;
        assert_eq!(tasoller_zones(&p), Some([false; 4]));
        p[4] = 20;
        assert_eq!(tasoller_zones(&p), Some([false, false, false, true]));
    }

    #[test]
    fn needs_neutral_then_fires_once_per_touch() {
        let mut edges = ZoneEdges::default();
        assert_eq!(edges.update([true, false, false, false]), None);
        assert_eq!(edges.update([false; 4]), None);
        assert_eq!(edges.update([true, false, false, false]), Some(Action::Left));
        assert_eq!(edges.update([true, false, false, false]), None);
        assert_eq!(edges.update([false; 4]), None);
        assert_eq!(edges.update([true, false, false, false]), Some(Action::Left));
    }

    #[test]
    fn ambiguous_contact_requires_release() {
        let mut edges = ZoneEdges::default();
        edges.update([false; 4]);
        assert_eq!(edges.update([false, true, false, true]), None);
        assert_eq!(edges.update([false, false, false, true]), None);
        edges.update([false; 4]);
        assert_eq!(edges.update([false, false, false, true]), Some(Action::Confirm));
    }

    #[test]
    fn all_four_actions_after_release() {
        let actions = [Action::Left, Action::Right, Action::Back, Action::Confirm];
        let mut edges = ZoneEdges::default();
        edges.update([false; 4]);
        for (zone, action) in actions.into_iter().enumerate() {
            edges.update([false; 4]);
            let mut state = [false; 4];
            state[zone] = true;
            assert_eq!(edges.update(state), Some(action));
        }
    }

    #[test]
    fn dragging_across_zones_does_not_confirm_until_released() {
        let mut edges = ZoneEdges::default();
        edges.update([false; 4]);
        assert_eq!(edges.update([true, false, false, false]), Some(Action::Left));
        assert_eq!(edges.update([false, true, false, false]), None);
        assert_eq!(edges.update([false, false, true, false]), None);
        assert_eq!(edges.update([false, false, false, true]), None);
        edges.update([false; 4]);
        assert_eq!(edges.update([false, false, false, true]), Some(Action::Confirm));
    }
}
