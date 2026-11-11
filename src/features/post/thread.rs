use std::collections::HashSet;

/// Lemmy paths are `0.<root id>.<reply id>`. Numeric sorting gives a preorder.
pub fn path_ids(path: &str) -> Vec<i32> {
    path.split('.')
        .filter_map(|part| part.parse().ok())
        .filter(|id| *id != 0)
        .collect()
}

pub fn hidden(path: &[i32], collapsed: &HashSet<i32>) -> bool {
    path.iter()
        .take(path.len().saturating_sub(1))
        .any(|id| collapsed.contains(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thread_order() {
        let mut paths = [path_ids("0.10"), path_ids("0.2.20"), path_ids("0.2")];
        paths.sort();
        assert_eq!(paths, [vec![2], vec![2, 20], vec![10]]);
        let collapsed = HashSet::from([2]);
        assert!(!hidden(&[2], &collapsed));
        assert!(hidden(&[2, 20], &collapsed));
        assert!(!hidden(&[10], &collapsed));
    }
}
