/// Return display indices without changing the saved/manual game order.
pub fn order(bindings: &[Option<&str>], active: &[String]) -> Vec<usize> {
    let matches = |i: &usize| bindings[*i].is_some_and(|id| active.iter().any(|a| a == id));
    let (mut preferred, others): (Vec<_>, Vec<_>) = (0..bindings.len()).partition(matches);
    preferred.extend(others);
    preferred
}

pub fn needs_confirmation(bindings: &[Option<&str>], active: &[String], selected: usize) -> bool {
    let matches = |binding: Option<&str>| binding.is_some_and(|id| active.iter().any(|a| a == id));
    bindings.iter().copied().any(matches) && !bindings.get(selected).copied().is_some_and(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connected_games_move_first_without_changing_relative_order() {
        let bindings = [None, Some("tasoller"), Some("yuancon"), Some("tasoller")];
        assert_eq!(order(&bindings, &["tasoller".into()]), vec![1, 3, 0, 2]);
        assert_eq!(order(&bindings, &[]), vec![0, 1, 2, 3]);
        assert_eq!(order(&bindings, &["yuancon".into(), "tasoller".into()]), vec![1, 2, 3, 0]);
    }
    #[test]
    fn only_another_controllers_game_or_an_unassigned_game_needs_confirmation() {
        let bindings = [None, Some("tasoller"), Some("yuancon"), Some("tasoller")];
        let active = vec!["tasoller".into()];
        assert!(needs_confirmation(&bindings, &active, 0));
        assert!(!needs_confirmation(&bindings, &active, 1));
        assert!(needs_confirmation(&bindings, &active, 2));
        assert!(!needs_confirmation(&bindings, &active, 3));
        assert!(!needs_confirmation(&bindings, &[], 0));
        assert!(!needs_confirmation(&bindings, &["unknown".into()], 0));
    }
}
