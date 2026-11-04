//! Diff unifié d'une ressource, construit uniquement depuis les valeurs structurées
//! `before` / `after` du plan. Rien n'est inventé : les lignes viennent du JSON mis en
//! forme, et seules les plages identiques longues sont repliées.

use serde_json::Value;

/// Lignes inchangées conservées autour de chaque changement.
pub const CONTEXT: usize = 3;
/// Au-delà de cette taille de table LCS, on renonce à l'alignement fin.
const MAX_CELLS: usize = 4_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Context,
    Removed,
    Added,
    Fold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    pub text: String,
    /// Plage d'octets de `text` qui diffère de la ligne appariée (début, fin).
    pub highlight: Option<(usize, usize)>,
}

impl Row {
    fn new(kind: RowKind, text: String) -> Self {
        Self {
            kind,
            text,
            highlight: None,
        }
    }
}

/// JSON mis en forme, ligne par ligne ; `null` ne produit aucune ligne.
pub fn pretty_lines(value: &Value) -> Vec<String> {
    if value.is_null() {
        return Vec::new();
    }
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| "[JSON illisible]".into())
        .lines()
        .map(str::to_owned)
        .collect()
}

pub fn unified(before: &Value, after: &Value) -> Vec<Row> {
    let old = pretty_lines(before);
    let new = pretty_lines(after);
    let edits = edits(&old, &new);
    let mut rows = fold(&edits, &old, &new);
    pair_highlights(&mut rows);
    rows
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit {
    Equal(usize),
    Remove(usize),
    Insert(usize),
}

fn edits(old: &[String], new: &[String]) -> Vec<Edit> {
    let n = old.len();
    let m = new.len();
    if n.saturating_mul(m) > MAX_CELLS {
        return (0..n)
            .map(Edit::Remove)
            .chain((0..m).map(Edit::Insert))
            .collect();
    }
    let width = m + 1;
    let mut table = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * width + j] = if old[i] == new[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let mut out = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old[i] == new[j] {
            out.push(Edit::Equal(i));
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            out.push(Edit::Remove(i));
            i += 1;
        } else {
            out.push(Edit::Insert(j));
            j += 1;
        }
    }
    out.extend((i..n).map(Edit::Remove));
    out.extend((j..m).map(Edit::Insert));
    out
}

fn fold(edits: &[Edit], old: &[String], new: &[String]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    let mut seen_change = false;
    for edit in edits {
        match *edit {
            Edit::Equal(index) => run.push(index),
            Edit::Remove(index) => {
                emit_equal_run(&mut rows, old, &run, !seen_change, false);
                run.clear();
                seen_change = true;
                rows.push(Row::new(RowKind::Removed, old[index].clone()));
            }
            Edit::Insert(index) => {
                emit_equal_run(&mut rows, old, &run, !seen_change, false);
                run.clear();
                seen_change = true;
                rows.push(Row::new(RowKind::Added, new[index].clone()));
            }
        }
    }
    emit_equal_run(&mut rows, old, &run, !seen_change, true);
    rows
}

fn emit_equal_run(
    rows: &mut Vec<Row>,
    old: &[String],
    run: &[usize],
    leading: bool,
    trailing: bool,
) {
    match (leading, trailing) {
        (true, true) => push_context(rows, old, run),
        (true, false) => {
            let keep = run.len().saturating_sub(CONTEXT);
            push_fold(rows, old, &run[..keep]);
            push_context(rows, old, &run[keep..]);
        }
        (false, true) => {
            let keep = run.len().min(CONTEXT);
            push_context(rows, old, &run[..keep]);
            push_fold(rows, old, &run[keep..]);
        }
        (false, false) => {
            if run.len() > 2 * CONTEXT {
                push_context(rows, old, &run[..CONTEXT]);
                push_fold(rows, old, &run[CONTEXT..run.len() - CONTEXT]);
                push_context(rows, old, &run[run.len() - CONTEXT..]);
            } else {
                push_context(rows, old, run);
            }
        }
    }
}

fn push_context(rows: &mut Vec<Row>, old: &[String], indices: &[usize]) {
    rows.extend(
        indices
            .iter()
            .map(|index| Row::new(RowKind::Context, old[*index].clone())),
    );
}

/// Replie une plage identique, sauf si elle est trop courte pour que ça vaille la peine.
fn push_fold(rows: &mut Vec<Row>, old: &[String], indices: &[usize]) {
    if indices.len() < 2 {
        push_context(rows, old, indices);
    } else {
        rows.push(Row::new(
            RowKind::Fold,
            format!("⋯ {} lignes identiques ⋯", indices.len()),
        ));
    }
}

/// Apparie chaque bloc de lignes retirées au bloc de lignes ajoutées qui le suit et
/// surligne, dans chaque paire, la portion qui change réellement.
fn pair_highlights(rows: &mut [Row]) {
    let mut index = 0;
    while index < rows.len() {
        if rows[index].kind != RowKind::Removed {
            index += 1;
            continue;
        }
        let removed_start = index;
        while index < rows.len() && rows[index].kind == RowKind::Removed {
            index += 1;
        }
        let added_start = index;
        while index < rows.len() && rows[index].kind == RowKind::Added {
            index += 1;
        }
        let pairs = (added_start - removed_start).min(index - added_start);
        for offset in 0..pairs {
            let (removed, added) = (removed_start + offset, added_start + offset);
            if let Some((left, right)) = highlight_pair(&rows[removed].text, &rows[added].text) {
                rows[removed].highlight = Some(left);
                rows[added].highlight = Some(right);
            }
        }
    }
}

fn highlight_pair(a: &str, b: &str) -> Option<((usize, usize), (usize, usize))> {
    // Même clé JSON des deux côtés : la valeur entière est ce qui change.
    if let (Some(separator), Some(other)) = (a.find("\": "), b.find("\": "))
        && separator == other
        && a[..separator] == b[..other]
    {
        let start = separator + 3;
        let trailing = usize::from(a.ends_with(',') && b.ends_with(','));
        let left = (start, a.len() - trailing);
        let right = (start, b.len() - trailing);
        if a[left.0..left.1] == b[right.0..right.1] {
            return None;
        }
        return Some((left, right));
    }
    let prefix = a
        .chars()
        .zip(b.chars())
        .take_while(|(left, right)| left == right)
        .map(|(left, _)| left.len_utf8())
        .sum::<usize>();
    let suffix = a[prefix..]
        .chars()
        .rev()
        .zip(b[prefix..].chars().rev())
        .take_while(|(left, right)| left == right)
        .map(|(left, _)| left.len_utf8())
        .sum::<usize>();
    if prefix == 0 && suffix == 0 {
        return None;
    }
    let left = (prefix, a.len() - suffix);
    let right = (prefix, b.len() - suffix);
    if left.0 == left.1 && right.0 == right.1 {
        return None;
    }
    Some((left, right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn render(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| {
                let marker = match row.kind {
                    RowKind::Context => ' ',
                    RowKind::Removed => '-',
                    RowKind::Added => '+',
                    RowKind::Fold => '…',
                };
                format!("{marker}{}", row.text)
            })
            .collect()
    }

    #[test]
    fn changed_value_is_surrounded_by_context_and_highlighted() {
        let before =
            json!({"a": 1, "b": 2, "c": 3, "d": 4, "e": 5, "f": 6, "g": 7, "h": 8, "i": 9});
        let after =
            json!({"a": 1, "b": 2, "c": 3, "d": 4, "e": 50, "f": 6, "g": 7, "h": 8, "i": 9});
        let rows = unified(&before, &after);
        let text = render(&rows);
        assert_eq!(
            text,
            [
                "…⋯ 2 lignes identiques ⋯",
                "   \"b\": 2,",
                "   \"c\": 3,",
                "   \"d\": 4,",
                "-  \"e\": 5,",
                "+  \"e\": 50,",
                "   \"f\": 6,",
                "   \"g\": 7,",
                "   \"h\": 8,",
                "…⋯ 2 lignes identiques ⋯",
            ]
        );
        let removed = rows
            .iter()
            .find(|row| row.kind == RowKind::Removed)
            .unwrap();
        let added = rows.iter().find(|row| row.kind == RowKind::Added).unwrap();
        let (start, end) = removed.highlight.unwrap();
        assert_eq!(&removed.text[start..end], "5");
        let (start, end) = added.highlight.unwrap();
        assert_eq!(&added.text[start..end], "50");
    }

    #[test]
    fn creation_is_all_added_and_deletion_is_all_removed() {
        let value = json!({"name": "value"});
        let created = unified(&Value::Null, &value);
        assert!(created.iter().all(|row| row.kind == RowKind::Added));
        assert_eq!(created.len(), 3);
        let deleted = unified(&value, &Value::Null);
        assert!(deleted.iter().all(|row| row.kind == RowKind::Removed));
        assert!(created.iter().all(|row| row.highlight.is_none()));
    }

    #[test]
    fn short_identical_runs_are_not_folded() {
        let before = json!({"a": 1, "b": 2});
        let after = json!({"a": 1, "b": 3});
        let text = render(&unified(&before, &after));
        assert_eq!(
            text,
            [" {", "   \"a\": 1,", "-  \"b\": 2", "+  \"b\": 3", " }"]
        );
    }

    #[test]
    fn identical_values_produce_only_context() {
        let value = json!({"a": [1, 2]});
        let rows = unified(&value, &value);
        assert!(rows.iter().all(|row| row.kind == RowKind::Context));
        assert!(unified(&Value::Null, &Value::Null).is_empty());
    }

    #[test]
    fn unrelated_lines_get_no_highlight() {
        assert_eq!(highlight_pair("abc", "xyz"), None);
        assert_eq!(highlight_pair("same", "same"), None);
        assert_eq!(
            highlight_pair("  \"port\": 8080,", "  \"port\": 8443,"),
            Some(((10, 14), (10, 14)))
        );
        assert_eq!(highlight_pair("abc", "abcd"), Some(((3, 3), (3, 4))));
    }
}
