//! Offline summary for qualitative judgments exported by the lab workbench.

use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;

const DIMENSIONS: [&str; 6] = [
    "overall",
    "correctness",
    "learnability",
    "compression",
    "image_relevance",
    "ui_fit",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewFile {
    version: u32,
    baseline_source: String,
    candidate_source: String,
    judgments: Vec<Judgment>,
}

#[derive(Debug, Deserialize)]
struct Judgment {
    key: String,
    dimensions: BTreeMap<String, String>,
    #[serde(default)]
    note: String,
}

pub(crate) fn render(path: &str) -> Result<String, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read review `{path}`: {error}"))?;
    let review: ReviewFile = serde_json::from_str(&json)
        .map_err(|error| format!("failed to parse review `{path}`: {error}"))?;
    validate(&review, path)?;

    let notes = review
        .judgments
        .iter()
        .filter(|judgment| !judgment.note.trim().is_empty())
        .count();
    let mut report = format!(
        "review: {path}\nbaseline: {}\ncandidate: {}\njudgments: {}\nnotes: {notes}\n",
        review.baseline_source,
        review.candidate_source,
        review.judgments.len()
    );
    for dimension in DIMENSIONS {
        let mut baseline = 0;
        let mut tie = 0;
        let mut candidate = 0;
        for judgment in &review.judgments {
            match judgment.dimensions.get(dimension).map(String::as_str) {
                Some("baseline") => baseline += 1,
                Some("tie") => tie += 1,
                Some("candidate") => candidate += 1,
                _ => {}
            }
        }
        let reviewed = baseline + tie + candidate;
        report.push_str(&format!(
            "{dimension}: baseline {baseline}, tie {tie}, candidate {candidate}, reviewed {reviewed}/{}\n",
            review.judgments.len()
        ));
    }
    Ok(report)
}

fn validate(review: &ReviewFile, path: &str) -> Result<(), String> {
    if review.version != 1 {
        return Err(format!(
            "review `{path}` uses unsupported version {}",
            review.version
        ));
    }
    if review.baseline_source.trim().is_empty() || review.candidate_source.trim().is_empty() {
        return Err(format!("review `{path}` has an empty run source"));
    }
    let mut keys = HashSet::new();
    for judgment in &review.judgments {
        if judgment.key.trim().is_empty() || !keys.insert(&judgment.key) {
            return Err(format!(
                "review `{path}` has an empty or duplicate judgment key `{}`",
                judgment.key
            ));
        }
        for (dimension, verdict) in &judgment.dimensions {
            if !DIMENSIONS.contains(&dimension.as_str()) {
                return Err(format!(
                    "review `{path}` judgment `{}` has unknown dimension `{dimension}`",
                    judgment.key
                ));
            }
            if !matches!(verdict.as_str(), "baseline" | "tie" | "candidate") {
                return Err(format!(
                    "review `{path}` judgment `{}` has invalid verdict `{verdict}`",
                    judgment.key
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_workbench_review_shape() {
        let path = std::env::temp_dir().join(format!(
            "denpie-lab-review-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(
            &path,
            r#"{
              "version":1,
              "baselineSource":"base/scorecard.json",
              "candidateSource":"candidate/scorecard.json",
              "updatedAt":"2026-08-24T00:00:00Z",
              "judgments":[
                {"key":"one","dimensions":{"overall":"candidate","correctness":"tie"},"note":"cleaner"},
                {"key":"two","dimensions":{"overall":"baseline"},"note":""}
              ]
            }"#,
        )
        .expect("review fixture writes");

        let report = render(path.to_str().expect("UTF-8 path")).expect("review is valid");
        assert!(report.contains("overall: baseline 1, tie 0, candidate 1, reviewed 2/2"));
        assert!(report.contains("correctness: baseline 0, tie 1, candidate 0, reviewed 1/2"));
        assert!(report.contains("notes: 1"));
        std::fs::remove_file(path).expect("review fixture removed");
    }
}
