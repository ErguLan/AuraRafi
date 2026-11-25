//! World plan cancellation.

use super::super::query::text_argument;
use super::plan::{append_journal, load_planned_world, plan_cache, update_persisted_plan};
use super::*;

pub(crate) fn execute_cancel(request: &ParsedCommand) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let plan_id = text_argument(request, "plan_id")
            .or_else(|| text_argument(request, "id"))
            .ok_or_else(|| "game.cancel requires plan_id.".to_string())?;
        if !plan_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        {
            return Err("plan_id contains unsupported characters.".to_string());
        }
        let mut plan = load_planned_world(&plan_id)?;
        if plan.status == "committed" {
            return Err(format!(
                "Plan '{plan_id}' is already committed and cannot be cancelled. Use Undo instead."
            ));
        }
        if plan.status == "cancelled" {
            return Ok(CommandOutput::info(
                "World plan already cancelled",
                vec![format!("plan: {plan_id}")],
                serde_json::json!({"ok": true, "plan_id": plan_id, "cancelled": true, "replayed": true}),
            ));
        }
        plan.status = "cancelled".to_string();
        update_persisted_plan(&plan);
        if let Ok(mut cache) = plan_cache().lock() {
            cache.insert(plan.id.clone(), plan.clone());
        }
        if let Some(root) = active_project_root() {
            append_journal(
                &root,
                serde_json::json!({"event": "plan_cancelled", "plan_id": plan.id}),
            );
        }
        Ok(CommandOutput::info(
            "World plan cancelled",
            vec![format!("plan: {plan_id}")],
            serde_json::json!({
                "ok": true,
                "plan_id": plan.id,
                "plan_hash": plan.hash,
                "cancelled": true,
                "changed": false,
            }),
        ))
    })();
    result.unwrap_or_else(|error| CommandOutput::error("World cancel", error))
}
