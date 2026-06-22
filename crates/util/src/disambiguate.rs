use std::{collections::HashMap, hash::Hash};

pub fn compute_disambiguation_details<T, D>(
    items: &[T],
    get_description: impl Fn(&T, usize) -> D,
) -> Vec<usize>
where
    D: Eq + Hash + Clone,
{
    let mut details = vec![0usize; items.len()];
    let mut descriptions: HashMap<D, Vec<usize>> = HashMap::default();
    let mut current_descriptions: Vec<D> =
        items.iter().map(|item| get_description(item, 0)).collect();

    loop {
        let mut any_collisions = false;

        for (index, ((item, &detail), current_description)) in items
            .iter()
            .zip(&details)
            .zip(&mut current_descriptions)
            .enumerate()
        {
            if detail > 0 {
                let new_description = get_description(item, detail);
                if new_description == *current_description {
                    continue;
                }
                *current_description = new_description;
            }

            descriptions
                .entry(current_description.clone())
                .or_default()
                .push(index);
        }

        for (_, indices) in descriptions.drain() {
            if indices.len() > 1 {
                any_collisions = true;
                for index in indices {
                    let detail = details
                        .get_mut(index)
                        .expect("collision index should be in bounds");
                    *detail += 1;
                }
            }
        }

        if !any_collisions {
            break;
        }
    }

    details
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_conflicts() {
        let items = ["alpha", "beta", "gamma"];
        let details = compute_disambiguation_details(&items, |item, _detail| item.to_string());
        assert_eq!(details, vec![0, 0, 0]);
    }

    #[test]
    fn test_simple_two_way_conflict() {
        let items = [("src/foo.rs", "foo.rs"), ("lib/foo.rs", "foo.rs")];
        let details = compute_disambiguation_details(&items, |item, detail| match detail {
            0 => item.1.to_string(),
            _ => item.0.to_string(),
        });
        assert_eq!(details, vec![1, 1]);
    }

    #[test]
    fn test_three_way_conflict() {
        let items = [
            ("foo.rs", "a/foo.rs"),
            ("foo.rs", "b/foo.rs"),
            ("foo.rs", "c/foo.rs"),
        ];
        let details = compute_disambiguation_details(&items, |item, detail| match detail {
            0 => item.0.to_string(),
            _ => item.1.to_string(),
        });
        assert_eq!(details, vec![1, 1, 1]);
    }

    #[test]
    fn test_deeper_conflict() {
        let items = [
            ["file.rs", "src/file.rs", "a/src/file.rs"],
            ["file.rs", "src/file.rs", "b/src/file.rs"],
            ["file.rs", "lib/file.rs", "x/lib/file.rs"],
        ];
        let details = compute_disambiguation_details(&items, |item, detail| {
            let clamped = detail.min(item.len() - 1);
            item.get(clamped).unwrap().to_string()
        });
        assert_eq!(details, vec![2, 2, 1]);
    }

    #[test]
    fn test_mixed_conflicting_and_unique() {
        let items = [
            ("src/foo.rs", "foo.rs"),
            ("lib/foo.rs", "foo.rs"),
            ("src/bar.rs", "bar.rs"),
        ];
        let details = compute_disambiguation_details(&items, |item, detail| match detail {
            0 => item.1.to_string(),
            _ => item.0.to_string(),
        });
        assert_eq!(details, vec![1, 1, 0]);
    }

    #[test]
    fn test_identical_items_terminates() {
        let items = ["same", "same", "same"];
        let details = compute_disambiguation_details(&items, |item, _detail| item.to_string());
        assert_eq!(details, vec![1, 1, 1]);
    }

    #[test]
    fn test_single_item() {
        let items = ["only"];
        let details = compute_disambiguation_details(&items, |item, _detail| item.to_string());
        assert_eq!(details, vec![0]);
    }

    #[test]
    fn test_empty_input() {
        let items: [&str; 0] = [];
        let details = compute_disambiguation_details(&items, |item, _detail| item.to_string());
        assert!(details.is_empty());
    }
}
