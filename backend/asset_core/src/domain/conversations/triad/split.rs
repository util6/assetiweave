use std::collections::HashMap;

use super::super::{ConversationPartKind, ConversationPartRole, NormalizedConversationPart};
use super::types::{
    extract_auxiliary_from_metadata, format_code_block, is_answer_candidate, is_command_part,
    is_result_part, is_successful_status, is_user_part, SplitTurnTriad, TriadPart, TriadRawPart,
    TurnProcess, TurnProcessItem,
};

/// Splits a raw list of parts for a turn into the 3-part triad (prompt, process, response)
/// and intermediate parts for database storage.
pub fn split_turn_triad(
    adapter_id: &str,
    prompt: &str,
    raw_parts: &[TriadRawPart<'_>],
) -> SplitTurnTriad {
    // 1. 过滤掉重复 user 零件
    let indexed_parts: Vec<(usize, &TriadRawPart<'_>)> = raw_parts
        .iter()
        .enumerate()
        .filter(|(_, p)| !is_user_part(p))
        .collect();

    // 2. 识别所有命令或工具执行的最大原始索引
    let mut last_command_or_tool_idx: Option<usize> = None;
    for (idx, part) in &indexed_parts {
        if is_command_part(part) || is_result_part(part) {
            last_command_or_tool_idx = Some(*idx);
        }
    }

    // 3. 确定最终响应（response）：位于所有命令/工具执行之后（或末尾）的最后一条 answer 候选
    let mut final_response_idx: Option<usize> = None;
    for (idx, part) in indexed_parts.iter().rev() {
        if is_answer_candidate(adapter_id, part) {
            if let Some(last_tool) = last_command_or_tool_idx {
                if *idx > last_tool {
                    final_response_idx = Some(*idx);
                    break;
                }
            } else {
                final_response_idx = Some(*idx);
                break;
            }
        }
    }

    let response_card = None;

    // 4. 第一遍扫描：收集命令，建立 source_execution_id -> cmd_seq 映射
    let mut command_items = Vec::new();
    let mut exec_to_cmd_seq: HashMap<String, i64> = HashMap::new();
    let mut last_cmd_seq: Option<i64> = None;

    for (orig_idx, part) in &indexed_parts {
        if is_command_part(part) {
            let seq = *orig_idx as i64;
            last_cmd_seq = Some(seq);

            if let Some(exec_id) = part.source_execution_id {
                let trimmed = exec_id.trim();
                if !trimmed.is_empty() {
                    exec_to_cmd_seq.insert(trimmed.to_string(), seq);
                }
            }

            let kind = part.custom_kind.as_deref().unwrap_or("").trim();
            let final_kind = if kind.is_empty() {
                format!("{adapter_id}.command")
            } else {
                kind.to_string()
            };

            let (signal, _, _) = extract_auxiliary_from_metadata(
                part.metadata_json,
                part.signal,
                part.tool_name,
                part.file_path,
            );

            let exit_code = match part.exit_code {
                Some(0) => None,
                other => other,
            };

            let status = part.status.and_then(|s| {
                if is_successful_status(s) {
                    None
                } else {
                    Some(s.to_string())
                }
            });

            let (content, summary) = if part.command.is_some() {
                (
                    part.command.map(|s| s.to_string()),
                    part.text.map(|s| s.to_string()),
                )
            } else {
                (None, part.text.map(|s| s.to_string()))
            };

            command_items.push(TurnProcessItem {
                seq,
                kind: final_kind,
                label: part.command_label.map(|s| s.to_string()),
                content,
                signal,
                cwd: part.cwd.map(|s| s.to_string()),
                exit_code,
                summary,
                status,
                translated_text: part.translated_text.map(|s| s.to_string()),
            });
        }
    }

    // 5. 第二遍扫描：处理结果与中途思考
    let mut empty_results_to_absorb = Vec::new();
    let mut retained_parts = Vec::new();
    let mut response_text: Option<String> = None;
    let mut response_seq: Option<i64> = None;
    let mut response_translated_text: Option<String> = None;
    let mut response_card_kind: Option<String> = None;
    let mut response_signal: Option<String> = None;

    for (orig_idx, part) in &indexed_parts {
        let seq = *orig_idx as i64;

        if Some(*orig_idx) == final_response_idx {
            // 提取为最终回答
            let is_web = adapter_id == "web" || adapter_id.ends_with("-web");
            response_text = if is_web && part.kind == ConversationPartKind::CodeBlock {
                Some(format_code_block(part.text, part.language))
            } else {
                part.text.or(part.command).map(|s| s.to_string())
            };
            response_seq = Some(seq);
            response_translated_text = part.translated_text.map(|s| s.to_string());
            response_card_kind = part.custom_kind.as_deref().map(|s| s.to_string());
            let (sig, _, _) = extract_auxiliary_from_metadata(
                part.metadata_json,
                part.signal,
                part.tool_name,
                part.file_path,
            );
            response_signal = sig;
            continue;
        }

        if is_command_part(part) {
            // 命令已进入 process.items，不再留存于 parts
            continue;
        }

        if is_result_part(part) {
            let body = part.text.or(part.command).unwrap_or("").trim();
            if body.is_empty() {
                // 空壳 .result：准备吸收并丢弃
                let matched_seq = part
                    .source_execution_id
                    .and_then(|id| exec_to_cmd_seq.get(id.trim()).copied())
                    .or(last_cmd_seq);
                empty_results_to_absorb.push((matched_seq, part.exit_code, part.status));
                continue;
            }

            // 非空 .result：保留在 parts 中，通过 cmd_seq 关联
            let matched_seq = part
                .source_execution_id
                .and_then(|id| exec_to_cmd_seq.get(id.trim()).copied())
                .or(last_cmd_seq);

            let mut attrs_map = serde_json::Map::new();
            if let Some(cmd_seq) = matched_seq {
                attrs_map.insert("cmd_seq".to_string(), serde_json::json!(cmd_seq));
            }
            if let Some(code) = part.exit_code {
                if code != 0 {
                    attrs_map.insert("exit_code".to_string(), serde_json::json!(code));
                }
            }
            if let Some(status) = part.status {
                if !is_successful_status(status) {
                    attrs_map.insert("status".to_string(), serde_json::json!(status));
                }
            }

            let attrs_json = if attrs_map.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&attrs_map).unwrap_or_default())
            };

            let kind = part.custom_kind.as_deref().unwrap_or("").trim();
            let final_kind = if adapter_id == "web" {
                "process".to_string()
            } else if kind.is_empty() {
                format!("{adapter_id}.result")
            } else {
                kind.to_string()
            };

            retained_parts.push(TriadPart {
                part_index: seq,
                kind: final_kind,
                content: Some(body.to_string()),
                attrs_json,
                translated_text: part.translated_text.map(|s| s.to_string()),
            });
            continue;
        }

        // 其余零件（中途思考播报、文件修改、代码块、子代理等）
        let (signal, tool_name, file_path) = extract_auxiliary_from_metadata(
            part.metadata_json,
            part.signal,
            part.tool_name,
            part.file_path,
        );

        let mut attrs_map = serde_json::Map::new();
        if let Some(lang) = part.language {
            if !lang.trim().is_empty() {
                attrs_map.insert("language".to_string(), serde_json::json!(lang));
            }
        }
        if let Some(s) = signal {
            attrs_map.insert("signal".to_string(), serde_json::json!(s));
        }
        if let Some(t) = tool_name {
            attrs_map.insert("tool_name".to_string(), serde_json::json!(t));
        }
        if let Some(f) = file_path {
            attrs_map.insert("file_path".to_string(), serde_json::json!(f));
        }
        if let Some(lbl) = part.command_label {
            if !lbl.trim().is_empty() {
                attrs_map.insert("command_label".to_string(), serde_json::json!(lbl));
            }
        }
        if let Some(ref r) = part.semantic_role {
            attrs_map.insert("semantic_role".to_string(), serde_json::json!(r));
        }
        if let Some(ref r) = part.renderer {
            attrs_map.insert("renderer".to_string(), serde_json::json!(r));
        }

        let attrs_json = if attrs_map.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&attrs_map).unwrap_or_default())
        };

        // 中途伪 answer 纠偏
        let is_web = adapter_id == "web" || adapter_id.ends_with("-web");
        let kind = part.custom_kind.as_deref().unwrap_or("").trim();
        let final_kind = if is_web {
            if kind == "answer"
                || kind.ends_with(".answer")
                || part.kind == ConversationPartKind::CodeBlock
            {
                "answer".to_string()
            } else {
                "process".to_string()
            }
        } else if kind == "answer"
            || kind.ends_with(".answer")
            || is_answer_candidate(adapter_id, part)
        {
            format!("{adapter_id}.reasoning")
        } else if !kind.is_empty() {
            kind.to_string()
        } else {
            match part.kind {
                ConversationPartKind::Text => format!("{adapter_id}.reasoning"),
                ConversationPartKind::CodeBlock => format!("{adapter_id}.code"),
                ConversationPartKind::FileChange => format!("{adapter_id}.file-change"),
                ConversationPartKind::Subagent => format!("{adapter_id}.subagent"),
                ConversationPartKind::Metadata => format!("{adapter_id}.skill"),
                _ => format!("{adapter_id}.tool"),
            }
        };

        let content = if is_web && part.kind == ConversationPartKind::CodeBlock {
            Some(format_code_block(part.text, part.language))
        } else {
            part.text.or(part.command).map(|s| s.to_string())
        };

        retained_parts.push(TriadPart {
            part_index: seq,
            kind: final_kind,
            content,
            attrs_json,
            translated_text: part.translated_text.map(|s| s.to_string()),
        });
    }

    // 6. 回填空壳 .result 的非零退出码与失败状态至对应命令
    for (target_seq, exit_code, status) in empty_results_to_absorb {
        if let Some(seq) = target_seq {
            if let Some(cmd) = command_items.iter_mut().find(|c| c.seq == seq) {
                if let Some(code) = exit_code {
                    if code != 0 && cmd.exit_code.is_none() {
                        cmd.exit_code = Some(code);
                    }
                }
                if let Some(st) = status {
                    if !is_successful_status(st) && cmd.status.is_none() {
                        cmd.status = Some(st.to_string());
                    }
                }
            }
        }
    }

    // 7. 提取公共 cwd 至顶层，单条命令与顶层相同时省略
    let mut common_cwd: Option<String> = None;
    for cmd in &command_items {
        if let Some(ref cwd) = cmd.cwd {
            if !cwd.trim().is_empty() {
                if common_cwd.is_none() {
                    common_cwd = Some(cwd.clone());
                } else if common_cwd.as_ref() != Some(cwd) {
                    common_cwd = None;
                    break;
                }
            }
        }
    }

    if let Some(ref top_cwd) = common_cwd {
        for cmd in &mut command_items {
            if cmd.cwd.as_ref() == Some(top_cwd) {
                cmd.cwd = None;
            }
        }
    }

    let process = if command_items.is_empty()
        && common_cwd.is_none()
        && response_seq.is_none()
        && response_translated_text.is_none()
        && response_card.is_none()
        && response_card_kind.is_none()
        && response_signal.is_none()
    {
        None
    } else {
        Some(TurnProcess {
            cwd: common_cwd,
            response_seq,
            response_translated_text,
            response_card,
            response_card_kind,
            response_signal,
            items: command_items,
        })
    };

    SplitTurnTriad {
        prompt: prompt.to_string(),
        process,
        response: response_text,
        parts: retained_parts,
    }
}

/// Convenience wrapper for NormalizedConversationPart slices.
pub fn split_normalized_turn(
    adapter_id: &str,
    prompt: &str,
    parts: &[NormalizedConversationPart],
) -> SplitTurnTriad {
    let raw_parts: Vec<TriadRawPart<'_>> = parts.iter().map(TriadRawPart::from).collect();
    split_turn_triad(adapter_id, prompt, &raw_parts)
}
