//! Offline scorecard comparison for repeatable lab experiments.

use std::collections::BTreeMap;

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScorecardKind {
    Images,
    Prompts,
}

impl ScorecardKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Prompts => "prompts",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    mechanical: BTreeMap<&'static str, String>,
    metrics: BTreeMap<&'static str, Option<i64>>,
}

#[derive(Debug)]
struct Scorecard {
    kind: ScorecardKind,
    rows: BTreeMap<String, Row>,
}

pub(crate) fn render(baseline_path: &str, candidate_path: &str) -> Result<String, String> {
    let baseline = load(baseline_path)?;
    let candidate = load(candidate_path)?;
    if baseline.kind != candidate.kind {
        return Err(format!(
            "cannot compare {} and {} scorecards",
            baseline.kind.as_str(),
            candidate.kind.as_str()
        ));
    }

    let baseline_keys = baseline.rows.keys().collect::<Vec<_>>();
    let candidate_keys = candidate.rows.keys().collect::<Vec<_>>();
    let added = candidate_keys
        .iter()
        .filter(|key| !baseline.rows.contains_key(**key))
        .copied()
        .collect::<Vec<_>>();
    let removed = baseline_keys
        .iter()
        .filter(|key| !candidate.rows.contains_key(**key))
        .copied()
        .collect::<Vec<_>>();
    let matching = baseline_keys
        .iter()
        .filter(|key| candidate.rows.contains_key(**key))
        .copied()
        .collect::<Vec<_>>();

    let mut outcome_changes = Vec::new();
    let mut metric_changes = Vec::new();
    for key in &matching {
        let baseline_row = &baseline.rows[*key];
        let candidate_row = &candidate.rows[*key];
        for (field, baseline_value) in &baseline_row.mechanical {
            let candidate_value = candidate_row
                .mechanical
                .get(field)
                .expect("validated scorecards share a schema");
            if baseline_value != candidate_value {
                outcome_changes.push(format!(
                    "  {key} {field}: {baseline_value} -> {candidate_value}"
                ));
            }
        }
        for (field, baseline_value) in &baseline_row.metrics {
            let candidate_value = candidate_row
                .metrics
                .get(field)
                .expect("validated scorecards share a schema");
            if baseline_value != candidate_value {
                metric_changes.push(format_metric_change(
                    key,
                    field,
                    *baseline_value,
                    *candidate_value,
                ));
            }
        }
    }

    let mut report = format!(
        "compare: {}\nbaseline: {baseline_path}\ncandidate: {candidate_path}\nmatching: {}\nadded: {}\nremoved: {}\noutcome changes: {}\nmetric changes: {}\n",
        baseline.kind.as_str(),
        matching.len(),
        added.len(),
        removed.len(),
        outcome_changes.len(),
        metric_changes.len(),
    );
    append_keys(&mut report, "added", &added);
    append_keys(&mut report, "removed", &removed);
    append_lines(&mut report, "outcomes", &outcome_changes);
    append_lines(&mut report, "metrics", &metric_changes);
    Ok(report)
}

fn append_keys(report: &mut String, label: &str, keys: &[&String]) {
    if !keys.is_empty() {
        report.push_str(&format!("{label} cases:\n"));
        for key in keys {
            report.push_str(&format!("  {key}\n"));
        }
    }
}

fn append_lines(report: &mut String, label: &str, lines: &[String]) {
    if !lines.is_empty() {
        report.push_str(&format!("{label}:\n"));
        for line in lines {
            report.push_str(line);
            report.push('\n');
        }
    }
}

fn format_metric_change(
    key: &str,
    field: &str,
    baseline: Option<i64>,
    candidate: Option<i64>,
) -> String {
    match (baseline, candidate) {
        (Some(baseline), Some(candidate)) => {
            let delta = i128::from(candidate) - i128::from(baseline);
            format!("  {key} {field}: {baseline} -> {candidate} ({delta:+})")
        }
        (None, Some(candidate)) => format!("  {key} {field}: none -> {candidate}"),
        (Some(baseline), None) => format!("  {key} {field}: {baseline} -> none"),
        (None, None) => unreachable!("equal optional metrics are skipped"),
    }
}

fn load(path: &str) -> Result<Scorecard, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read scorecard `{path}`: {error}"))?;
    let value: Value = serde_json::from_str(&json)
        .map_err(|error| format!("failed to parse scorecard `{path}`: {error}"))?;
    let rows = value
        .as_array()
        .ok_or_else(|| format!("scorecard `{path}` must be a JSON array"))?;
    let Some(first) = rows.first() else {
        return Err(format!(
            "scorecard `{path}` is empty; its bench type is ambiguous"
        ));
    };
    let kind = if first.get("strategy").is_some() {
        ScorecardKind::Images
    } else if first.get("mode").is_some() {
        ScorecardKind::Prompts
    } else {
        return Err(format!(
            "scorecard `{path}` is neither an image nor prompt scorecard"
        ));
    };

    let mut parsed = BTreeMap::new();
    for (index, value) in rows.iter().enumerate() {
        let (key, row) = match kind {
            ScorecardKind::Images => parse_image_row(value, path, index)?,
            ScorecardKind::Prompts => parse_prompt_row(value, path, index)?,
        };
        if parsed.insert(key.clone(), row).is_some() {
            return Err(format!("scorecard `{path}` has duplicate row `{key}`"));
        }
    }
    Ok(Scorecard { kind, rows: parsed })
}

fn parse_image_row(value: &Value, path: &str, index: usize) -> Result<(String, Row), String> {
    let case_id = unsigned_number(value, "case_id", path, index)?;
    let strategy = string(value, "strategy", path, index)?;
    let key = format!("{case_id}/{strategy}");
    let mechanical = BTreeMap::from([
        (
            "search_or_download",
            string(value, "search_or_download", path, index)?,
        ),
        ("kind", string(value, "kind", path, index)?),
    ]);
    let metrics = BTreeMap::from([
        ("bytes", Some(number(value, "bytes", path, index)?)),
        (
            "elapsed_ms",
            Some(number(value, "elapsed_ms", path, index)?),
        ),
    ]);
    Ok((
        key,
        Row {
            mechanical,
            metrics,
        },
    ))
}

fn parse_prompt_row(value: &Value, path: &str, index: usize) -> Result<(String, Row), String> {
    let case_id = string(value, "case_id", path, index)?;
    let mechanical = BTreeMap::from([
        (
            "assembled",
            boolean(value, "assembled", path, index)?.to_string(),
        ),
        ("generated", string(value, "generated", path, index)?),
        ("kind", string(value, "kind", path, index)?),
        (
            "use_image",
            boolean(value, "use_image", path, index)?.to_string(),
        ),
    ]);
    let metrics = BTreeMap::from([
        (
            "elapsed_ms",
            Some(number(value, "elapsed_ms", path, index)?),
        ),
        (
            "prompt_tokens",
            Some(number(value, "prompt_tokens", path, index)?),
        ),
        (
            "title_words",
            optional_metric(value, "title_words", path, index)?,
        ),
        (
            "full_content_words",
            optional_metric(value, "full_content_words", path, index)?,
        ),
        (
            "compressed_content_words",
            optional_metric(value, "compressed_content_words", path, index)?,
        ),
        (
            "completion_tokens",
            optional_metric(value, "completion_tokens", path, index)?,
        ),
        (
            "total_tokens",
            optional_metric(value, "total_tokens", path, index)?,
        ),
    ]);
    Ok((
        case_id,
        Row {
            mechanical,
            metrics,
        },
    ))
}

fn string(value: &Value, field: &str, path: &str, index: usize) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("scorecard `{path}` row {index} has no string `{field}`"))
}

fn number(value: &Value, field: &str, path: &str, index: usize) -> Result<i64, String> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("scorecard `{path}` row {index} has no integer `{field}`"))
}

fn unsigned_number(value: &Value, field: &str, path: &str, index: usize) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("scorecard `{path}` row {index} has no unsigned integer `{field}`"))
}

fn optional_metric(
    value: &Value,
    field: &str,
    path: &str,
    index: usize,
) -> Result<Option<i64>, String> {
    match value.get(field) {
        Some(Value::Null) => Ok(None),
        Some(value) => value.as_i64().map(Some).ok_or_else(|| {
            format!("scorecard `{path}` row {index} has no integer or null `{field}`")
        }),
        None => Ok(None),
    }
}

fn boolean(value: &Value, field: &str, path: &str, index: usize) -> Result<bool, String> {
    value
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("scorecard `{path}` row {index} has no boolean `{field}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp(name: &str, content: &str) -> String {
        let path = std::env::temp_dir().join(format!(
            "denpie-lab-compare-{name}-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(&path, content).expect("write scorecard fixture");
        path.display().to_string()
    }

    #[test]
    fn compares_image_outcomes_metrics_and_case_sets() {
        let baseline = write_temp(
            "image-baseline",
            r#"[
          {"case_id":1,"strategy":"bing_html","search_or_download":"miss","kind":"none","bytes":0,"elapsed_ms":10},
          {"case_id":2,"strategy":"bing_html","search_or_download":"hit","kind":"prepared","bytes":8,"elapsed_ms":20}
        ]"#,
        );
        let candidate = write_temp(
            "image-candidate",
            r#"[
          {"case_id":1,"strategy":"bing_html","search_or_download":"hit","kind":"prepared","bytes":7,"elapsed_ms":15},
          {"case_id":3,"strategy":"ddgs_text_og","search_or_download":"hit","kind":"prepared","bytes":9,"elapsed_ms":30}
        ]"#,
        );

        let report = render(&baseline, &candidate).expect("valid comparison");
        assert!(report.contains("compare: images"));
        assert!(report.contains("matching: 1"));
        assert!(report.contains("added cases:\n  3/ddgs_text_og"));
        assert!(report.contains("removed cases:\n  2/bing_html"));
        assert!(report.contains("1/bing_html search_or_download: miss -> hit"));
        assert!(report.contains("1/bing_html elapsed_ms: 10 -> 15 (+5)"));
        assert!(report.contains("1/bing_html bytes: 0 -> 7 (+7)"));
    }

    #[test]
    fn compares_prompt_word_and_token_metrics() {
        let baseline = write_temp(
            "prompt-baseline",
            r#"[
          {"case_id":"grammar","mode":"one_shot","assembled":true,"generated":"hit","kind":"generated","title_words":2,"use_image":false,"prompt_tokens":10,"elapsed_ms":20}
        ]"#,
        );
        let candidate = write_temp(
            "prompt-candidate",
            r#"[
          {"case_id":"grammar","mode":"one_shot","assembled":true,"generated":"hit","kind":"generated","title_words":3,"full_content_words":12,"compressed_content_words":6,"use_image":true,"prompt_tokens":12,"completion_tokens":4,"total_tokens":16,"elapsed_ms":18}
        ]"#,
        );

        let report = render(&baseline, &candidate).expect("valid comparison");
        assert!(report.contains("compare: prompts"));
        assert!(report.contains("grammar use_image: false -> true"));
        assert!(report.contains("grammar prompt_tokens: 10 -> 12 (+2)"));
        assert!(report.contains("grammar title_words: 2 -> 3 (+1)"));
        assert!(report.contains("grammar elapsed_ms: 20 -> 18 (-2)"));
        assert!(report.contains("grammar completion_tokens: none -> 4"));
        assert!(report.contains("grammar full_content_words: none -> 12"));
    }

    #[test]
    fn rejects_mixed_or_duplicate_scorecards() {
        let images = write_temp(
            "mixed-images",
            r#"[
          {"case_id":1,"strategy":"bing_html","search_or_download":"hit","kind":"prepared","bytes":1,"elapsed_ms":1}
        ]"#,
        );
        let prompts = write_temp(
            "mixed-prompts",
            r#"[
          {"case_id":"one","mode":"one_shot","assembled":true,"generated":"hit","kind":"generated","title_words":1,"use_image":true,"prompt_tokens":1,"elapsed_ms":1}
        ]"#,
        );
        assert!(
            render(&images, &prompts)
                .unwrap_err()
                .contains("cannot compare")
        );

        let duplicate = write_temp(
            "duplicate",
            r#"[
          {"case_id":1,"strategy":"bing_html","search_or_download":"hit","kind":"prepared","bytes":1,"elapsed_ms":1},
          {"case_id":1,"strategy":"bing_html","search_or_download":"hit","kind":"prepared","bytes":1,"elapsed_ms":1}
        ]"#,
        );
        assert!(load(&duplicate).unwrap_err().contains("duplicate row"));
    }
}
