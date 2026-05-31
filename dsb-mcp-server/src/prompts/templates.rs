// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! MCP prompt templates
//!
//! This module defines prompt templates for DSB workflows.

use serde_json::json;

/// Get all available prompts
pub fn get_prompts_list() -> serde_json::Value {
    json!({
        "prompts": [
            {
                "name": "web_scraping_workflow",
                "description": "Step-by-step guidance for web scraping tasks using DSB. Provides best practices, tool selection guidance, and workflow steps for effective web scraping.",
                "arguments": [
                    {
                        "name": "target_url",
                        "description": "The URL to scrape",
                        "required": true
                    },
                    {
                        "name": "data_requirements",
                        "description": "Description of what data to extract",
                        "required": false
                    },
                    {
                        "name": "output_format",
                        "description": "Desired output format (markdown, json, csv)",
                        "required": false
                    }
                ]
            },
            {
                "name": "advanced_web_scraping",
                "description": "Handle complex web scraping scenarios including JavaScript rendering, pagination, authentication, and dynamic content loading.",
                "arguments": [
                    {
                        "name": "scenario",
                        "description": "Description of the complex scenario",
                        "required": true
                    },
                    {
                        "name": "target_url",
                        "description": "URL to scrape",
                        "required": true
                    }
                ]
            },
            {
                "name": "sandbox_management",
                "description": "Guide for creating, configuring, and managing DSB sandboxes. Includes recommendations for different task types and best practices.",
                "arguments": [
                    {
                        "name": "task_type",
                        "description": "Type of task (web_scraping, code_execution, browser_automation)",
                        "required": true
                    },
                    {
                        "name": "requirements",
                        "description": "Additional requirements or constraints",
                        "required": false
                    }
                ]
            },
            {
                "name": "sandbox_troubleshooting",
                "description": "Debug and resolve common sandbox issues including connection problems, execution failures, and performance bottlenecks.",
                "arguments": [
                    {
                        "name": "issue_type",
                        "description": "Type of issue (connection, execution, performance)",
                        "required": true
                    },
                    {
                        "name": "error_message",
                        "description": "Error details or message",
                        "required": false
                    }
                ]
            }
        ]
    })
}

/// Generate prompt messages for a specific prompt
pub async fn get_prompt_messages(
    name: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match name {
        "web_scraping_workflow" => generate_web_scraping_prompt(arguments),
        "advanced_web_scraping" => generate_advanced_scraping_prompt(arguments),
        "sandbox_management" => generate_sandbox_management_prompt(arguments),
        "sandbox_troubleshooting" => generate_troubleshooting_prompt(arguments),
        _ => Err(format!("Unknown prompt: {}", name)),
    }
}

fn generate_web_scraping_prompt(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let target_url = args
        .get("target_url")
        .and_then(|v| v.as_str())
        .ok_or("target_url is required")?;

    let data_requirements = args.get("data_requirements").and_then(|v| v.as_str());
    let output_format = args.get("output_format").and_then(|v| v.as_str());

    let mut guidance = format!("I need help scraping data from: {}\n\n", target_url);

    if let Some(requirements) = data_requirements {
        guidance.push_str(&format!("Data Requirements: {}\n\n", requirements));
    }

    if let Some(format) = output_format {
        guidance.push_str(&format!("Preferred Output Format: {}\n\n", format));
    }

    guidance.push_str(
        "Please guide me through:\n\
         1. The best approach for this web scraping task\n\
         2. Which tools to use (scrape_web, search_web, automate_browser)\n\
         3. Step-by-step workflow\n\
         4. Best practices and potential issues",
    );

    Ok(json!({
        "description": "Web scraping workflow guidance",
        "messages": [
            {
                "role": "user",
                "content": {
                    "type": "text",
                    "text": guidance
                }
            }
        ]
    }))
}

fn generate_advanced_scraping_prompt(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let scenario = args
        .get("scenario")
        .and_then(|v| v.as_str())
        .ok_or("scenario is required")?;

    let target_url = args
        .get("target_url")
        .and_then(|v| v.as_str())
        .ok_or("target_url is required")?;

    let guidance = format!(
        "I'm dealing with a complex web scraping scenario:\n\
         Target URL: {}\n\
         Scenario: {}\n\n\
         Please help me with:\n\
         1. Advanced strategies for handling this scenario\n\
         2. Whether to use browser automation (browser_automate) vs static scraping\n\
         3. Handling JavaScript rendering if needed\n\
         4. Dealing with pagination, authentication, or rate limiting\n\
         5. Alternative approaches if the primary method fails",
        target_url, scenario
    );

    Ok(json!({
        "description": "Advanced web scraping guidance",
        "messages": [
            {
                "role": "user",
                "content": {
                    "type": "text",
                    "text": guidance
                }
            }
        ]
    }))
}

fn generate_sandbox_management_prompt(
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let task_type = args
        .get("task_type")
        .and_then(|v| v.as_str())
        .ok_or("task_type is required")?;

    let requirements = args.get("requirements").and_then(|v| v.as_str());

    let mut guidance = format!("I need to set up a DSB sandbox for: {}\n\n", task_type);

    if let Some(reqs) = requirements {
        guidance.push_str(&format!("Additional Requirements: {}\n\n", reqs));
    }

    guidance.push_str(
        "Please provide:\n\
         1. Recommended sandbox image and configuration\n\
         2. Whether to use 'full' or 'slim' sandbox type\n\
         3. Setup steps and initial commands\n\
         4. Best practices for this task type\n\
         5. Performance and resource considerations",
    );

    Ok(json!({
        "description": "Sandbox management guidance",
        "messages": [
            {
                "role": "user",
                "content": {
                    "type": "text",
                    "text": guidance
                }
            }
        ]
    }))
}

fn generate_troubleshooting_prompt(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let issue_type = args
        .get("issue_type")
        .and_then(|v| v.as_str())
        .ok_or("issue_type is required")?;

    let error_message = args.get("error_message").and_then(|v| v.as_str());

    let mut guidance = format!("I'm experiencing a sandbox issue: {}\n\n", issue_type);

    if let Some(error) = error_message {
        guidance.push_str(&format!("Error Message: {}\n\n", error));
    }

    guidance.push_str(
        "Please help me:\n\
         1. Diagnose the root cause\n\
         2. Provide troubleshooting steps\n\
         3. Suggest diagnostic commands\n\
         4. Offer solutions or workarounds\n\
         5. Recommend preventive measures",
    );

    Ok(json!({
        "description": "Sandbox troubleshooting guidance",
        "messages": [
            {
                "role": "user",
                "content": {
                    "type": "text",
                    "text": guidance
                }
            }
        ]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_prompts_list() {
        let result = get_prompts_list();
        let prompts = result
            .get("prompts")
            .and_then(|v| v.as_array())
            .expect("prompts should be an array");

        assert_eq!(prompts.len(), 4);

        // Check web_scraping_workflow prompt
        let web_scraping = &prompts[0];
        assert_eq!(web_scraping["name"], "web_scraping_workflow");
        assert!(web_scraping["description"].as_str().unwrap().len() > 0);

        let arguments = web_scraping
            .get("arguments")
            .and_then(|v| v.as_array())
            .expect("arguments should be an array");
        assert_eq!(arguments.len(), 3);
    }

    #[tokio::test]
    async fn test_get_web_scraping_prompt() {
        let args = json!({
            "target_url": "https://example.com",
            "data_requirements": "Extract product names and prices",
            "output_format": "json"
        });

        let result = get_prompt_messages("web_scraping_workflow", args).await;
        assert!(result.is_ok());

        let prompt = result.unwrap();
        let messages = prompt
            .get("messages")
            .and_then(|v| v.as_array())
            .expect("messages should be an array");

        assert_eq!(messages.len(), 1);
        assert!(messages[0]["role"] == "user");
        assert!(messages[0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("example.com"));
    }

    #[tokio::test]
    async fn test_get_unknown_prompt() {
        let args = json!({});
        let result = get_prompt_messages("unknown_prompt", args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown prompt"));
    }

    #[tokio::test]
    async fn test_sandbox_management_prompt() {
        let args = json!({
            "task_type": "web_scraping",
            "requirements": "Need JavaScript support"
        });

        let result = get_prompt_messages("sandbox_management", args).await;
        assert!(result.is_ok());
    }
}
