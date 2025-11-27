use super::error::GenerateTriggerError;
use std::collections::HashMap;
use std::fs;

// Note: TemplateManager duplicates structure from generate_action::template_manager
// Both modules need to stay in sync for shared functionality

/// Template manager for handling code generation templates
pub struct TemplateManager {
    templates: HashMap<String, String>,
}

impl TemplateManager {
    /// Create a new template manager
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// Load a template from file
    pub fn load_template(&mut self, name: &str, path: &str) -> Result<(), GenerateTriggerError> {
        let content = fs::read_to_string(path).map_err(|e| {
            GenerateTriggerError::TemplateError(format!(
                "Failed to read template '{}': {}",
                name, e
            ))
        })?;
        self.templates.insert(name.to_string(), content);
        Ok(())
    }

    /// Load all standard templates
    pub fn load_standard_templates(&mut self) -> Result<(), GenerateTriggerError> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let templates_dir = format!("{}/build_templates", manifest_dir);
        // Only load the trigger executor template - helper functions are embedded in it
        self.load_template(
            "trigger_executor",
            &format!("{}/trigger_executor.rs.template", templates_dir),
        )?;
        Ok(())
    }

    /// Render a template with variables
    pub fn render(
        &self,
        template_name: &str,
        variables: &HashMap<String, String>,
    ) -> Result<String, GenerateTriggerError> {
        let template = self.templates.get(template_name).ok_or_else(|| {
            GenerateTriggerError::TemplateError(format!("Template '{}' not found", template_name))
        })?;
        let mut result = template.clone();
        for (key, value) in variables {
            let placeholder = format!("{{{}}}", key);
            result = result.replace(&placeholder, value);
        }
        if let Some(start) = result.find('{')
            && let Some(end) = result[start..].find('}')
        {
            let placeholder = &result[start..start + end + 1];
            if placeholder
                .chars()
                .skip(1)
                .take_while(|&c| c != '}')
                .all(|c| c.is_uppercase() || c == '_')
            {
                return Err(GenerateTriggerError::TemplateError(format!(
                    "Unresolved placeholder: {}",
                    placeholder
                )));
            }
        }
        Ok(result)
    }
}

impl Default for TemplateManager {
    fn default() -> Self {
        Self::new()
    }
}
