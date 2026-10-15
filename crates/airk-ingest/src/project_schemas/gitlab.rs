// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn extract(facts: &mut Facts<'_>, document: &Value) -> Result<bool> {
    if document.mapping().is_none() {
        return issue(facts, document, "GitLab CI requires a mapping");
    }
    let mut complete = true;
    for (key, value) in entries(document) {
        match key.scalar() {
            Some("stages") => complete &= written(facts, value, "pipeline_stage")?,
            Some("include") => complete &= written(facts, value, "pipeline_include")?,
            Some(
                "variables" | "default" | "workflow" | "image" | "services" | "before_script"
                | "after_script" | "cache" | "spec",
            ) => complete &= written(facts, value, "pipeline_configuration")?,
            Some(_) => {
                complete &= emit(facts, key, "pipeline_job")?;
                if value.mapping().is_none() {
                    complete &= issue(facts, value, "GitLab job must be a mapping")?;
                    continue;
                }
                for (key, value) in entries(value) {
                    let kind = match key.scalar() {
                        Some("stage") => "job_stage",
                        Some("needs") => "job_needs",
                        Some("dependencies") => "job_dependency",
                        Some("artifacts") => "job_artifact",
                        Some("script" | "before_script" | "after_script") => "job_script",
                        Some("extends") => "job_inheritance",
                        _ => "job_configuration",
                    };
                    complete &= written(facts, value, kind)?;
                }
            }
            None => complete &= issue(facts, key, "GitLab key must be scalar")?,
        }
    }
    Ok(complete)
}
