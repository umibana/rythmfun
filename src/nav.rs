#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Left,
    Right,
    Confirm,
    Back,
}

/// Up/down are aliases only while browsing the portrait game carousel.
pub fn keyboard_action(key: &str, vertical_home: bool) -> Option<Action> {
    match key {
        "ArrowLeft" => Some(Action::Left),
        "ArrowRight" => Some(Action::Right),
        "ArrowUp" if vertical_home => Some(Action::Left),
        "ArrowDown" if vertical_home => Some(Action::Right),
        "Enter" => Some(Action::Confirm),
        "Backspace" | "Escape" => Some(Action::Back),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Home,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Bar,
    Content,
}

/// What a step did, so the UI can play a sound or launch a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Moved,
    Confirmed,
    Back,
    Launch(usize),
    Nothing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nav {
    pub tab: Tab,
    pub zone: Zone,
    pub card: usize,
}

impl Default for Nav {
    fn default() -> Self {
        Nav { tab: Tab::Home, zone: Zone::Content, card: 0 }
    }
}

impl Nav {
    pub fn step(self, action: Action, games: usize) -> (Nav, Outcome) {
        let mut n = self;
        n.card = n.card.min(games.saturating_sub(1));
        let out = match (n.zone, n.tab, action) {
            (Zone::Bar, _, Action::Left | Action::Right) => {
                n.tab = match n.tab {
                    Tab::Home => Tab::Settings,
                    Tab::Settings => Tab::Home,
                };
                Outcome::Moved
            }
            (Zone::Bar, _, Action::Confirm) => {
                n.zone = Zone::Content;
                Outcome::Confirmed
            }
            (Zone::Bar, _, Action::Back) => Outcome::Nothing,
            (Zone::Content, _, Action::Back) => {
                n.zone = Zone::Bar;
                Outcome::Back
            }
            (Zone::Content, Tab::Home, Action::Left) if n.card > 0 => {
                n.card -= 1;
                Outcome::Moved
            }
            (Zone::Content, Tab::Home, Action::Right) if n.card + 1 < games => {
                n.card += 1;
                Outcome::Moved
            }
            (Zone::Content, Tab::Home, Action::Confirm) if games > 0 => Outcome::Launch(n.card),
            _ => Outcome::Nothing,
        };
        (n, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_arrows_select_previous_and_next_game() {
        assert_eq!(keyboard_action("ArrowUp", true), Some(Action::Left));
        assert_eq!(keyboard_action("ArrowDown", true), Some(Action::Right));
        assert_eq!(home(1).step(keyboard_action("ArrowUp", true).unwrap(), 3), (home(0), Outcome::Moved));
        assert_eq!(home(1).step(keyboard_action("ArrowDown", true).unwrap(), 3), (home(2), Outcome::Moved));
    }

    #[test]
    fn vertical_arrows_are_ignored_outside_vertical_home() {
        assert_eq!(keyboard_action("ArrowUp", false), None);
        assert_eq!(keyboard_action("ArrowDown", false), None);
        assert_eq!(keyboard_action("ArrowLeft", true), Some(Action::Left));
        assert_eq!(keyboard_action("ArrowRight", false), Some(Action::Right));
        assert_eq!(keyboard_action("Enter", true), Some(Action::Confirm));
    }

    fn home(card: usize) -> Nav {
        Nav { card, ..Nav::default() }
    }

    #[test]
    fn moves_between_cards_and_stops_at_edges() {
        assert_eq!(home(0).step(Action::Right, 3), (home(1), Outcome::Moved));
        assert_eq!(home(2).step(Action::Right, 3), (home(2), Outcome::Nothing));
        assert_eq!(home(0).step(Action::Left, 3), (home(0), Outcome::Nothing));
    }

    #[test]
    fn confirm_launches_selected_card() {
        assert_eq!(home(1).step(Action::Confirm, 3), (home(1), Outcome::Launch(1)));
        assert_eq!(home(0).step(Action::Confirm, 0), (home(0), Outcome::Nothing));
    }

    #[test]
    fn back_goes_to_bar_and_bar_switches_tabs() {
        let (bar, out) = home(1).step(Action::Back, 3);
        assert_eq!(out, Outcome::Back);
        assert_eq!(bar.zone, Zone::Bar);

        let (bar, out) = bar.step(Action::Right, 3);
        assert_eq!((bar.tab, out), (Tab::Settings, Outcome::Moved));
        assert_eq!(bar.step(Action::Back, 3), (bar, Outcome::Nothing));

        let (content, out) = bar.step(Action::Confirm, 3);
        assert_eq!((content.zone, out), (Zone::Content, Outcome::Confirmed));
    }

    #[test]
    fn settings_content_ignores_left_right_confirm() {
        let s = Nav { tab: Tab::Settings, ..Nav::default() };
        for a in [Action::Left, Action::Right, Action::Confirm] {
            assert_eq!(s.step(a, 3), (s, Outcome::Nothing));
        }
    }

    #[test]
    fn card_is_clamped_when_games_shrink() {
        assert_eq!(home(5).step(Action::Left, 2), (home(0), Outcome::Moved));
    }
}
