// SPDX-License-Identifier: Apache-2.0
//! Written workflow structure; action references and run bodies are never executed.
use crate::{
    build_facts::Facts,
    structured_yaml::{self, Value, emit},
};
use anyhow::Result;

pub fn analyze(path: &str, text: &str) -> Result<crate::syntax::Analysis> {
    let view = structured_yaml::parse(text)?;
    let mut facts = Facts::new(path, text, "github-actions")?;
    view.diagnostics(&mut facts)?;
    let mut partial = !view.issues.is_empty();
    for document in &view.documents {
        let Some(entries) = document.mapping() else {
            shape(&mut facts, document, "workflow mapping")?;
            partial = true;
            continue;
        };
        for (key, value) in entries {
            match key.scalar() {
                Some("on") => {
                    if let Some(triggers) = value.mapping() {
                        for (trigger, _) in triggers {
                            partial |= !emit(&mut facts, trigger, "workflow_trigger")?;
                        }
                    } else {
                        partial |= !scalar_list(&mut facts, value, "workflow_trigger")?;
                    }
                }
                Some("jobs") => {
                    let Some(jobs) = value.mapping() else {
                        shape(&mut facts, value, "jobs mapping")?;
                        partial = true;
                        continue;
                    };
                    for (job, body) in jobs {
                        partial |= !emit(&mut facts, job, "workflow_job")?;
                        partial |= !job_body(&mut facts, body)?;
                    }
                }
                Some("permissions" | "env" | "defaults" | "concurrency") => {
                    partial |= !configuration(&mut facts, value, key.scalar().unwrap())?
                }
                _ => {}
            }
        }
    }
    Ok(facts.finish(partial))
}

fn job_body(facts: &mut Facts<'_>, body: &Value) -> Result<bool> {
    let Some(entries) = body.mapping() else {
        shape(facts, body, "job mapping")?;
        return Ok(false);
    };
    let mut complete = true;
    for (key, value) in entries {
        match key.scalar() {
            Some("needs") => complete &= scalar_list(facts, value, "workflow_needs")?,
            Some("uses") => complete &= emit(facts, value, "action_reference")?,
            Some("steps") => {
                let Some(steps) = value.sequence() else {
                    shape(facts, value, "steps sequence")?;
                    complete = false;
                    continue;
                };
                for step in steps {
                    let Some(entries) = step.mapping() else {
                        shape(facts, step, "step mapping")?;
                        complete = false;
                        continue;
                    };
                    for (key, value) in entries {
                        match key.scalar() {
                            Some("name" | "id") => complete &= emit(facts, value, "workflow_step")?,
                            Some("uses") => complete &= emit(facts, value, "action_reference")?,
                            Some("run") => complete &= emit(facts, value, "run_body")?,
                            Some("env" | "with" | "if" | "working-directory" | "shell") => {
                                complete &= configuration(facts, value, key.scalar().unwrap())?
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some(
                "permissions" | "env" | "environment" | "with" | "secrets" | "if" | "runs-on"
                | "strategy" | "concurrency",
            ) => complete &= configuration(facts, value, key.scalar().unwrap())?,
            _ => {}
        }
    }
    Ok(complete)
}

fn configuration(facts: &mut Facts<'_>, value: &Value, category: &str) -> Result<bool> {
    let kind = if category == "permissions" {
        "workflow_permission"
    } else if category == "env" {
        "environment_config"
    } else {
        "workflow_config"
    };
    if let Some(entries) = value.mapping() {
        let mut complete = true;
        for (key, value) in entries {
            complete &= emit(facts, key, kind)?;
            complete &= configuration(facts, value, category)?;
        }
        Ok(complete)
    } else {
        scalar_list(facts, value, kind)
    }
}

fn scalar_list(facts: &mut Facts<'_>, value: &Value, kind: &str) -> Result<bool> {
    if let Some(values) = value.sequence() {
        let mut complete = true;
        for value in values {
            complete &= emit(facts, value, kind)?;
        }
        Ok(complete)
    } else {
        emit(facts, value, kind)
    }
}
fn shape(facts: &mut Facts<'_>, value: &Value, expected: &str) -> Result<()> {
    facts.push(
        value.start,
        value.end,
        "extraction_diagnostic",
        "yaml_shape",
        &format!("Expected {expected}; unsupported structure omitted"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_original_run_keeps_exact_source_span() -> Result<()> {
        let command = format!("echo {}", "界".repeat(3000));
        let text = format!("jobs:\n  original:\n    steps:\n      - run: {command}\n");
        let result = analyze(".github/workflows/original.yml", &text)?;
        let run = result.facts.iter().find(|f| f.kind == "run_body").unwrap();
        assert!(run.name.len() <= 4096);
        let code = run.code.as_ref().unwrap();
        assert!(code.details_truncated);
        let span = code.span.as_ref().unwrap();
        assert_eq!(&text[span.start_byte..span.end_byte], command);
        Ok(())
    }
    #[test]
    fn original_workflow_preserves_expressions_and_opaque_run() -> Result<()> {
        let text = "on: [push, workflow_dispatch]\npermissions: {contents: read}\nenv: {MODE: '${{ vars.MODE }}'}\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: example/action@v1\n      - name: Build\n        run: |\n          echo 'jobs: fake'\n  test:\n    needs: [build]\n    uses: ./local.yml\n";
        let result = analyze(".github/workflows/check.yml", text)?;
        for kind in [
            "workflow_trigger",
            "workflow_permission",
            "environment_config",
            "workflow_job",
            "workflow_step",
            "workflow_needs",
            "action_reference",
            "run_body",
        ] {
            assert!(result.facts.iter().any(|f| f.kind == kind), "{kind}");
        }
        assert!(
            !result
                .facts
                .iter()
                .any(|f| f.kind == "workflow_job" && f.name == "fake")
        );
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.name.contains("${{ vars.MODE }}"))
        );
        Ok(())
    }
    #[test]
    fn wrong_shape_and_duplicate_jobs_are_partial() -> Result<()> {
        assert_eq!(
            analyze("a.yml", "jobs: [broken]\n")?.status,
            "partial-syntax"
        );
        let result = analyze("a.yml", "jobs:\n  same: {}\n  same: {}\n")?;
        assert_eq!(result.status, "partial-syntax");
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|f| f.kind == "workflow_job")
                .count(),
            2
        );
        Ok(())
    }
}
