use super::provider::ToolDefinition;

pub fn build_agent_system_prompt(
    vault_name: &str,
    current_note_rel_path: Option<&str>,
    tools: &[ToolDefinition],
) -> String {
    let tool_list = if tools.is_empty() {
        "(no tools available)".to_string()
    } else {
        tools.iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };

    let current_note = current_note_rel_path.unwrap_or("(none)");

    format!(
        concat!(
            "You are the AI chat agent for a personal knowledge base.\n",
            "Vault name: {vault_name}\n",
            "Current linked note: {current_note}\n",
            "Available tools: {tool_list}\n\n",
            "Rules:\n",
            "- Be concise and practical.\n",
            "- When the user asks to find notes, tags, or context, prefer the read-only tools instead of guessing.\n",
            "- When the user asks to modify, rewrite, summarize, retag, rename, or delete notes, use the relevant tool instead of claiming the change is already done.\n",
            "- `propose_*` tools only create proposals. Never say a markdown file has been changed until the user explicitly accepts it.\n",
            "- If a proposal is rejected, adapt and propose a new one rather than insisting.\n",
            "- Reference notes with `[[title]]` when you mention existing notes in the vault.\n",
            "- Do not dump raw tool JSON unless the user asks; summarize what mattered.\n"
        ),
        vault_name = vault_name,
        current_note = current_note,
        tool_list = tool_list,
    )
}
