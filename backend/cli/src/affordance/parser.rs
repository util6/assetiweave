use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandAffordance {
    pub name: String,
    pub description: Option<String>,
    pub use_this_when: String,
    pub avoid_when: String,
    pub examples: Vec<String>,
    pub related_skills: Vec<String>,
    pub risk: String,
}

impl CommandAffordance {
    pub fn fallback_for(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: Some(format!("General operations for `{}`", name)),
            use_this_when: format!("- Performing operations related to `{}`.", name),
            avoid_when: "- Using without checking command options or required arguments."
                .to_string(),
            examples: vec![format!("aiwc {} --help", name)],
            related_skills: vec![],
            risk: "Standard CLI command execution.".to_string(),
        }
    }

    /// Renders the 5-dimension affordance guidance into a formatted string suitable for `after_long_help`.
    pub fn render_after_long_help(&self) -> String {
        let mut out = String::new();
        out.push_str("Affordance:\n");

        append_text_section(&mut out, "Use this when", &self.use_this_when);
        append_text_section(&mut out, "Avoid when", &self.avoid_when);
        append_list_section(&mut out, "Examples", &self.examples);
        append_list_section(&mut out, "Related skills", &self.related_skills);
        append_text_section(&mut out, "Risk", &self.risk);

        out.trim_end().to_string()
    }
}

fn append_text_section(out: &mut String, title: &str, content: &str) {
    let trimmed_content = content.trim();
    if !trimmed_content.is_empty() {
        out.push_str(&format!("  {title}:\n"));
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                out.push_str(&format!("    {trimmed}\n"));
            }
        }
        out.push('\n');
    }
}

fn append_list_section(out: &mut String, title: &str, items: &[String]) {
    if !items.is_empty() {
        out.push_str(&format!("  {title}:\n"));
        for item in items {
            let trimmed = item.trim();
            if !trimmed.is_empty() {
                if trimmed.starts_with('-') || trimmed.starts_with('*') {
                    out.push_str(&format!("    {trimmed}\n"));
                } else {
                    out.push_str(&format!("    - {trimmed}\n"));
                }
            }
        }
        out.push('\n');
    }
}

#[derive(Debug, Clone, Default)]
pub struct AffordanceRegistry {
    commands: HashMap<String, CommandAffordance>,
}

#[derive(Debug, PartialEq, Eq)]
enum CurrentSection {
    None,
    Description,
    UseThisWhen,
    AvoidWhen,
    Examples,
    RelatedSkills,
    Risk,
}

impl AffordanceRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn insert(&mut self, affordance: CommandAffordance) {
        self.commands
            .insert(affordance.name.trim().to_lowercase(), affordance);
    }

    pub fn get(&self, name: &str) -> Option<&CommandAffordance> {
        let key = name.trim().to_lowercase();
        self.commands.get(&key).or_else(|| {
            let space_key = key.replace('.', " ");
            self.commands.get(&space_key)
        })
    }

    pub fn get_or_fallback(&self, name: &str) -> CommandAffordance {
        if let Some(affordance) = self.get(name) {
            affordance.clone()
        } else {
            CommandAffordance::fallback_for(name)
        }
    }

    pub fn render_help(&self, name: &str) -> Option<String> {
        self.get(name).map(|a| a.render_after_long_help())
    }

    pub fn render_help_or_fallback(&self, name: &str) -> String {
        self.get_or_fallback(name).render_after_long_help()
    }

    pub fn all_commands(&self) -> &HashMap<String, CommandAffordance> {
        &self.commands
    }

    /// Parse markdown documentation into a structured AffordanceRegistry.
    ///
    /// Recognizes:
    /// - `# <command>` or `## <command>` as command boundaries
    /// - `### Use this when` (or `#### Use this when`)
    /// - `### Avoid when`
    /// - `### Examples`
    /// - `### Related skills`
    /// - `### Risk`
    pub fn parse(markdown: &str) -> Self {
        let mut registry = Self::new();
        let mut current_cmd: Option<CommandAffordance> = None;
        let mut current_sec = CurrentSection::None;

        for line in markdown.lines() {
            let trimmed = line.trim();

            // Check if this line is a command heading (# <cmd> or ## <cmd>)
            let is_level1 = trimmed.starts_with("# ") && !trimmed.starts_with("## ");
            let is_level2 = trimmed.starts_with("## ") && !trimmed.starts_with("### ");

            if is_level1 || is_level2 {
                // Save previous command if any
                if let Some(cmd) = current_cmd.take() {
                    if !cmd.name.is_empty() {
                        registry.insert(cmd);
                    }
                }

                let cmd_name = if is_level1 {
                    trimmed.strip_prefix("# ").unwrap_or("").trim()
                } else {
                    trimmed.strip_prefix("## ").unwrap_or("").trim()
                };

                current_cmd = Some(CommandAffordance {
                    name: cmd_name.to_string(),
                    ..Default::default()
                });
                current_sec = CurrentSection::Description;
                continue;
            }

            // Check if this line is a dimension heading (### or ####)
            if trimmed.starts_with("### ") || trimmed.starts_with("#### ") {
                let heading_text = trimmed.trim_start_matches('#').trim().to_lowercase();
                if heading_text.starts_with("use this when") {
                    current_sec = CurrentSection::UseThisWhen;
                } else if heading_text.starts_with("avoid when") {
                    current_sec = CurrentSection::AvoidWhen;
                } else if heading_text.starts_with("example") {
                    current_sec = CurrentSection::Examples;
                } else if heading_text.starts_with("related skill") {
                    current_sec = CurrentSection::RelatedSkills;
                } else if heading_text.starts_with("risk") {
                    current_sec = CurrentSection::Risk;
                } else {
                    current_sec = CurrentSection::None;
                }
                continue;
            }

            // Otherwise, append line content to the current active section
            if let Some(cmd) = current_cmd.as_mut() {
                match current_sec {
                    CurrentSection::Description => {
                        if !trimmed.is_empty() {
                            let desc = cmd.description.get_or_insert_with(String::new);
                            if !desc.is_empty() {
                                desc.push(' ');
                            }
                            desc.push_str(trimmed);
                        }
                    }
                    CurrentSection::UseThisWhen => {
                        if !trimmed.is_empty() {
                            if !cmd.use_this_when.is_empty() {
                                cmd.use_this_when.push('\n');
                            }
                            cmd.use_this_when.push_str(line);
                        }
                    }
                    CurrentSection::AvoidWhen => {
                        if !trimmed.is_empty() {
                            if !cmd.avoid_when.is_empty() {
                                cmd.avoid_when.push('\n');
                            }
                            cmd.avoid_when.push_str(line);
                        }
                    }
                    CurrentSection::Examples => {
                        if !trimmed.is_empty() {
                            cmd.examples.push(trimmed.to_string());
                        }
                    }
                    CurrentSection::RelatedSkills => {
                        if !trimmed.is_empty() {
                            cmd.related_skills.push(trimmed.to_string());
                        }
                    }
                    CurrentSection::Risk => {
                        if !trimmed.is_empty() {
                            if !cmd.risk.is_empty() {
                                cmd.risk.push('\n');
                            }
                            cmd.risk.push_str(line);
                        }
                    }
                    CurrentSection::None => {}
                }
            }
        }

        // Save last command
        if let Some(cmd) = current_cmd.take() {
            if !cmd.name.is_empty() {
                registry.insert(cmd);
            }
        }

        registry
    }
}
