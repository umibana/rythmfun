use crate::nav::Action;

/// One button: (report id, byte index in report data, bit mask, action).
pub type Binding = (u8, usize, u8, Action);

// Yuancon tassa (VID 0x5F73, PID 0x0010/0x0011, ids from yuancon.app).
// ponytail: empty until real input reports are captured in Settings → Controlador.
const TASSA: &[Binding] = &[];

pub fn profile(vendor_id: u16, product_id: u16) -> &'static [Binding] {
    match (vendor_id, product_id) {
        (0x5F73, 0x0010 | 0x0011) => TASSA,
        _ => &[],
    }
}

/// Actions whose button went from released (`prev`) to pressed (`now`) in report `report_id`.
pub fn pressed(bindings: &[Binding], report_id: u8, prev: &[u8], now: &[u8]) -> Vec<Action> {
    let down = |data: &[u8], i: usize, mask: u8| data.get(i).is_some_and(|b| b & mask != 0);
    bindings
        .iter()
        .filter(|(rid, i, mask, _)| *rid == report_id && down(now, *i, *mask) && !down(prev, *i, *mask))
        .map(|(_, _, _, a)| *a)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAD: &[Binding] = &[(1, 0, 0b01, Action::Left), (1, 0, 0b10, Action::Right), (1, 2, 0x80, Action::Confirm)];

    #[test]
    fn fires_only_on_press_edge() {
        assert_eq!(pressed(PAD, 1, &[0, 0, 0], &[0b01, 0, 0x80]), vec![Action::Left, Action::Confirm]);
        assert_eq!(pressed(PAD, 1, &[0b01, 0, 0x80], &[0b01, 0, 0x80]), vec![]);
    }

    #[test]
    fn ignores_other_reports_and_short_data() {
        assert_eq!(pressed(PAD, 2, &[], &[0b11, 0, 0x80]), vec![]);
        assert_eq!(pressed(PAD, 1, &[], &[0b10]), vec![Action::Right]);
    }

    #[test]
    fn unknown_device_has_no_bindings() {
        assert!(profile(0x1234, 0x5678).is_empty());
    }
}
