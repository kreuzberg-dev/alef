use crate::cli::pipeline::helpers::{check_precondition, run_before, run_command_streamed};
use crate::core::config::{Language, ResolvedCrateConfig};
use rayon::prelude::*;

/// Clean build artifacts for each language.
pub fn clean(config: &ResolvedCrateConfig, languages: &[Language]) -> anyhow::Result<()> {
    let results: Vec<(Language, anyhow::Result<()>)> = languages
        .par_iter()
        .map(|lang| {
            let label = lang.to_string();
            let clean_cfg = config.clean_config_for_language(*lang);
            if !check_precondition(*lang, clean_cfg.precondition.as_deref()) {
                return (*lang, Ok(()));
            }
            if let Err(e) = run_before(*lang, clean_cfg.before.as_ref()) {
                return (*lang, Err(e));
            }
            if let Some(cmd_list) = &clean_cfg.clean {
                for cmd in cmd_list.commands() {
                    if let Err(e) = run_command_streamed(cmd, Some(&label)) {
                        return (*lang, Err(e));
                    }
                }
            }
            (*lang, Ok(()))
        })
        .collect();

    let mut first_error: Option<anyhow::Error> = None;
    for (lang, result) in results {
        if let Err(e) = result {
            eprintln!("✗ clean failed: {lang} — {e}");
            if first_error.is_none() {
                first_error = Some(e);
            }
        }
    }
    if let Some(e) = first_error {
        return Err(e);
    }

    Ok(())
}
