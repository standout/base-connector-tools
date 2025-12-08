use super::error::GenerateActionError;
use std::collections::HashMap;
use std::fs;

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
    pub fn load_template(&mut self, name: &str, path: &str) -> Result<(), GenerateActionError> {
        let content = fs::read_to_string(path).map_err(|e| {
            GenerateActionError::TemplateError(format!("Failed to read template '{}': {}", name, e))
        })?;

        self.templates.insert(name.to_string(), content);
        Ok(())
    }

    /// Load all standard templates
    pub fn load_standard_templates(&mut self) -> Result<(), GenerateActionError> {
        // Find templates relative to the package root (CARGO_MANIFEST_DIR)
        // This works even when the binary is run from a different directory
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let templates_dir = format!("{}/build_templates", manifest_dir);

        self.load_template(
            "get_executor",
            &format!("{}/get_executor.rs.template", templates_dir),
        )?;
        self.load_template(
            "delete_executor",
            &format!("{}/delete_executor.rs.template", templates_dir),
        )?;
        self.load_template(
            "post_patch_executor",
            &format!("{}/post_patch_executor.rs.template", templates_dir),
        )?;
        self.load_template(
            "path_parameter_functions",
            &format!("{}/path_parameter_functions.rs.template", templates_dir),
        )?;
        self.load_template(
            "query_parameter_functions",
            &format!("{}/query_parameter_functions.rs.template", templates_dir),
        )?;
        Ok(())
    }

    /// Render a template with variables
    pub fn render(
        &self,
        template_name: &str,
        variables: &HashMap<String, String>,
    ) -> Result<String, GenerateActionError> {
        let template = self.templates.get(template_name).ok_or_else(|| {
            GenerateActionError::TemplateError(format!("Template '{}' not found", template_name))
        })?;

        let mut result = template.clone();

        for (key, value) in variables {
            let placeholder = format!("{{{}}}", key);
            result = result.replace(&placeholder, value);
        }

        // Check for any remaining placeholders (but be more careful about false positives)
        // Only report as error if we find a placeholder that looks like our format: {WORD}
        if let Some(start) = result.find('{')
            && let Some(end) = result[start..].find('}')
        {
            let placeholder = &result[start..start + end + 1];
            // Only report as error if it looks like our template placeholder format (uppercase letters and underscores)
            if placeholder
                .chars()
                .skip(1)
                .take_while(|&c| c != '}')
                .all(|c| c.is_uppercase() || c == '_')
            {
                return Err(GenerateActionError::TemplateError(format!(
                    "Unresolved placeholder: {}",
                    placeholder
                )));
            }
        }

        Ok(result)
    }

    /// Get a template by name
    #[allow(dead_code)]
    pub fn get_template(&self, name: &str) -> Option<&String> {
        self.templates.get(name)
    }

    /// Check if a template exists
    #[allow(dead_code)]
    pub fn has_template(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }

    /// List all loaded template names
    #[allow(dead_code)]
    pub fn list_templates(&self) -> Vec<&String> {
        self.templates.keys().collect()
    }
}

impl Default for TemplateManager {
    fn default() -> Self {
        Self::new()
    }
}
