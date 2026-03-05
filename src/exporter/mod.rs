use color_eyre::Result;
use std::path::{Path, PathBuf};

use crate::collector::transcript::{ConversationTurn, TranscriptData};

mod markdown;

/// Export a session to a Markdown file.
///
/// # Parameters
/// - `session_path`: Original path to the session file (used to extract project name)
/// - `data`: Session data
/// - `conversation`: Conversation history (optional)
///
/// # Returns
/// - Ok(PathBuf): Full path to the exported file
/// - Err: Export failure error
pub fn export_session(
    session_path: &Path,
    data: &TranscriptData,
    conversation: Option<&[ConversationTurn]>,
) -> Result<PathBuf> {
    // 1. Determine export directory
    let export_dir = get_export_dir()?;

    // 2. Generate filename
    let filename = generate_filename(data)?;
    let output_path = export_dir.join(filename);

    // 3. Generate Markdown content
    let markdown_content = markdown::generate_markdown(session_path, data, conversation)?;

    // 4. Write file
    std::fs::write(&output_path, markdown_content)?;

    Ok(output_path)
}

/// Get the export directory path, creating it if it doesn't exist.
///
/// Export location priority:
/// 1. Current working directory (./aink-exports/) - if writable
/// 2. User's Documents folder (~/Documents/aink-exports/) - fallback
/// 3. Application data directory (~/Library/Application Support/aink/exports/) - last resort
fn get_export_dir() -> Result<PathBuf> {
    // Try current working directory first (most discoverable for CLI tools)
    if let Ok(cwd) = std::env::current_dir() {
        let export_dir = cwd.join("aink-exports");

        // Check if we can create/write to this directory
        if export_dir.exists() || std::fs::create_dir_all(&export_dir).is_ok() {
            // Verify we can actually write to it
            if export_dir
                .metadata()
                .map(|m| !m.permissions().readonly())
                .unwrap_or(false)
            {
                return Ok(export_dir);
            }
        }
    }

    // Fallback to Documents folder (more discoverable than Application Support)
    if let Some(docs_dir) = dirs::document_dir() {
        let export_dir = docs_dir.join("aink-exports");

        if export_dir.exists() || std::fs::create_dir_all(&export_dir).is_ok() {
            return Ok(export_dir);
        }
    }

    // Last resort: Application data directory
    let base_dir = dirs::data_dir()
        .ok_or_else(|| color_eyre::eyre::eyre!("Unable to determine data directory"))?;

    let export_dir = base_dir.join("aink").join("exports");

    if !export_dir.exists() {
        std::fs::create_dir_all(&export_dir)?;
    }

    Ok(export_dir)
}

/// Generate export filename: aink-export-{project-name}-{timestamp}.md
fn generate_filename(data: &TranscriptData) -> Result<String> {
    use chrono::Local;

    let project_name = data
        .project_name
        .as_deref()
        .filter(|s| !s.is_empty()) // Filter out empty strings
        .unwrap_or("unknown")
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-");

    let timestamp = Local::now().format("%Y%m%d-%H%M%S");

    Ok(format!("aink-export-{}-{}.md", project_name, timestamp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::source::SourceKind;
    use std::collections::{HashMap, HashSet};
    use tempfile::TempDir;

    /// Helper function to create a minimal TranscriptData for testing
    fn create_test_transcript(project_name: Option<&str>) -> TranscriptData {
        TranscriptData {
            source: SourceKind::Kiro,
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 100,
            cache_read_tokens: 200,
            models: HashSet::new(),
            tool_call_total: 10,
            tool_call_by_type: HashMap::new(),
            files_touched: vec![],
            first_user_message: Some("Test message".to_string()),
            summary: None,
            per_model: HashMap::new(),
            duration_ms: 60000,
            turn_count: 5,
            user_message_count: 3,
            assistant_message_count: 2,
            start_time: Some("2025-01-15T10:00:00Z".to_string()),
            end_time: Some("2025-01-15T10:01:00Z".to_string()),
            agent_version: Some("1.0.0".to_string()),
            git_branch: Some("main".to_string()),
            slug: None,
            project_name: project_name.map(|s| s.to_string()),
            estimated_cost_usd: 0.05,
        }
    }

    #[test]
    fn test_get_export_dir_creates_directory() {
        // This test verifies that get_export_dir creates the directory structure
        // We can't easily test the actual directory creation without mocking,
        // but we can verify the function returns a valid path
        let result = get_export_dir();
        assert!(result.is_ok(), "get_export_dir should succeed");

        let path = result.unwrap();
        assert!(
            path.to_string_lossy().contains("aink"),
            "Path should contain 'aink'"
        );
        assert!(
            path.to_string_lossy().contains("exports"),
            "Path should contain 'exports'"
        );
    }

    #[test]
    fn test_generate_filename_with_project_name() {
        let data = create_test_transcript(Some("my-project"));
        let result = generate_filename(&data);

        assert!(result.is_ok(), "generate_filename should succeed");
        let filename = result.unwrap();

        // Verify format: aink-export-{project-name}-{timestamp}.md
        assert!(
            filename.starts_with("aink-export-my-project-"),
            "Filename should start with 'aink-export-my-project-', got: {}",
            filename
        );
        assert!(
            filename.ends_with(".md"),
            "Filename should end with '.md', got: {}",
            filename
        );

        // Verify timestamp format: YYYYMMDD-HHMMSS
        let parts: Vec<&str> = filename.split('-').collect();
        assert!(
            parts.len() >= 5,
            "Filename should have at least 5 parts separated by '-'"
        );

        // Extract timestamp parts (last two parts before .md)
        let timestamp_date = parts[parts.len() - 2];
        let timestamp_time = parts[parts.len() - 1].trim_end_matches(".md");

        assert_eq!(
            timestamp_date.len(),
            8,
            "Date part should be 8 digits (YYYYMMDD)"
        );
        assert_eq!(
            timestamp_time.len(),
            6,
            "Time part should be 6 digits (HHMMSS)"
        );

        // Verify all characters are digits
        assert!(
            timestamp_date.chars().all(|c| c.is_ascii_digit()),
            "Date should be all digits"
        );
        assert!(
            timestamp_time.chars().all(|c| c.is_ascii_digit()),
            "Time should be all digits"
        );
    }

    #[test]
    fn test_generate_filename_without_project_name() {
        let data = create_test_transcript(None);
        let result = generate_filename(&data);

        assert!(result.is_ok(), "generate_filename should succeed");
        let filename = result.unwrap();

        // Should use "unknown" as default project name
        assert!(
            filename.starts_with("aink-export-unknown-"),
            "Filename should start with 'aink-export-unknown-', got: {}",
            filename
        );
        assert!(filename.ends_with(".md"), "Filename should end with '.md'");
    }

    #[test]
    fn test_generate_filename_replaces_special_characters() {
        // Test various special characters that should be replaced with hyphens
        let special_chars = vec![
            ("project/name", "project-name"),
            ("project\\name", "project-name"),
            ("project:name", "project-name"),
            ("project*name", "project-name"),
            ("project?name", "project-name"),
            ("project\"name", "project-name"),
            ("project<name", "project-name"),
            ("project>name", "project-name"),
            ("project|name", "project-name"),
            ("project/\\:*?\"<>|name", "project---------name"),
        ];

        for (input, expected_sanitized) in special_chars {
            let data = create_test_transcript(Some(input));
            let result = generate_filename(&data);

            assert!(
                result.is_ok(),
                "generate_filename should succeed for input: {}",
                input
            );
            let filename = result.unwrap();

            // Check that the sanitized project name is in the filename
            assert!(
                filename.contains(expected_sanitized),
                "Filename should contain '{}', got: {}",
                expected_sanitized,
                filename
            );

            // Verify it starts with the expected prefix
            let expected_prefix = format!("aink-export-{}-", expected_sanitized);
            assert!(
                filename.starts_with(&expected_prefix),
                "Filename should start with '{}', got: {}",
                expected_prefix,
                filename
            );
        }
    }

    #[test]
    fn test_generate_filename_multiple_calls_different_timestamps() {
        let data = create_test_transcript(Some("test-project"));

        let filename1 = generate_filename(&data).unwrap();

        // Sleep briefly to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(1100));

        let filename2 = generate_filename(&data).unwrap();

        // Filenames should be different due to different timestamps
        assert_ne!(
            filename1, filename2,
            "Consecutive calls should generate different filenames"
        );
    }

    #[test]
    fn test_export_session_creates_file() {
        // Use a temporary directory for testing
        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("test-session.json");

        let data = create_test_transcript(Some("test-export"));

        // Note: This test will create a file in the actual export directory
        // In a real scenario, we'd want to mock the file system or use dependency injection
        let result = export_session(&session_path, &data, None);

        // We expect this to succeed
        assert!(result.is_ok(), "export_session should succeed");

        let output_path = result.unwrap();

        // Verify the file was created
        assert!(
            output_path.exists(),
            "Export file should exist at: {:?}",
            output_path
        );

        // Verify the file has content
        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(!content.is_empty(), "Export file should not be empty");

        // Note: The markdown module is not fully implemented yet (task 4.1)
        // For now, we just verify the file was created with some content
        // When task 4.1 is complete, uncomment these assertions:
        // assert!(content.contains("# AI Coding Session:"),
        //         "Export should contain session title");
        // assert!(content.contains("**Source:**"),
        //         "Export should contain source metadata");

        // Clean up the created file
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn test_export_session_with_conversation() {
        use crate::collector::transcript::{ConversationRole, ConversationTurn, ToolCallDetail};

        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("test-session.json");

        let data = create_test_transcript(Some("test-with-conversation"));

        let conversation = vec![
            ConversationTurn {
                role: ConversationRole::User,
                content: "Hello, can you help me?".to_string(),
                tool_calls: vec![],
                created_at: Some("2025-01-15T10:00:00Z".to_string()),
            },
            ConversationTurn {
                role: ConversationRole::Assistant,
                content: "Of course! What do you need?".to_string(),
                tool_calls: vec![ToolCallDetail {
                    name: "readFile".to_string(),
                    summary: "src/main.rs".to_string(),
                }],
                created_at: Some("2025-01-15T10:00:05Z".to_string()),
            },
        ];

        let result = export_session(&session_path, &data, Some(&conversation));

        assert!(
            result.is_ok(),
            "export_session with conversation should succeed"
        );

        let output_path = result.unwrap();
        let content = std::fs::read_to_string(&output_path).unwrap();

        // Verify file was created with content
        assert!(!content.is_empty(), "Export file should not be empty");

        // Note: The markdown module is not fully implemented yet (task 4.1)
        // When task 4.1 is complete, uncomment these assertions:
        // assert!(content.contains("## Conversation"),
        //         "Export should contain conversation section");
        // assert!(content.contains("Hello, can you help me?"),
        //         "Export should contain user message");
        // assert!(content.contains("Of course! What do you need?"),
        //         "Export should contain assistant message");
        // assert!(content.contains("**Tool Calls:**"),
        //         "Export should contain tool calls section");
        // assert!(content.contains("`readFile`"),
        //         "Export should contain tool name");

        // Clean up
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn test_generate_filename_regex_pattern() {
        // Verify the filename matches the expected pattern without using regex crate
        let data = create_test_transcript(Some("my-project"));
        let filename = generate_filename(&data).unwrap();

        // Pattern: ^aink-export-.+-\d{8}-\d{6}\.md$
        // Manual verification:
        assert!(
            filename.starts_with("aink-export-"),
            "Should start with 'aink-export-'"
        );
        assert!(filename.ends_with(".md"), "Should end with '.md'");

        // Extract the parts
        let without_prefix = filename.strip_prefix("aink-export-").unwrap();
        let without_suffix = without_prefix.strip_suffix(".md").unwrap();
        let parts: Vec<&str> = without_suffix.rsplitn(3, '-').collect();

        // Should have at least 3 parts: time, date, project-name
        assert!(parts.len() >= 3, "Should have at least 3 parts");

        let time_part = parts[0];
        let date_part = parts[1];

        // Verify date and time format
        assert_eq!(date_part.len(), 8, "Date should be 8 digits");
        assert_eq!(time_part.len(), 6, "Time should be 6 digits");
        assert!(
            date_part.chars().all(|c| c.is_ascii_digit()),
            "Date should be all digits"
        );
        assert!(
            time_part.chars().all(|c| c.is_ascii_digit()),
            "Time should be all digits"
        );
    }

    #[test]
    fn test_generate_filename_empty_project_name() {
        let data = create_test_transcript(Some(""));
        let filename = generate_filename(&data).unwrap();

        // Empty string should be treated as "unknown"
        assert!(
            filename.starts_with("aink-export--") || filename.starts_with("aink-export-unknown-"),
            "Empty project name should result in empty or 'unknown', got: {}",
            filename
        );
    }

    #[test]
    fn test_export_session_utf8_encoding() {
        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("test-session.json");

        // Create data with Unicode characters
        let mut data = create_test_transcript(Some("测试项目"));
        data.first_user_message = Some("你好世界 🌍".to_string());
        data.summary = Some("Unicode test: café, naïve, 日本語".to_string());

        let result = export_session(&session_path, &data, None);
        assert!(result.is_ok(), "export_session with Unicode should succeed");

        let output_path = result.unwrap();

        // Read the file and verify UTF-8 content is preserved
        let content = std::fs::read_to_string(&output_path);
        assert!(content.is_ok(), "Should be able to read file as UTF-8");

        let content = content.unwrap();
        assert!(!content.is_empty(), "Export file should not be empty");

        // Note: The markdown module is not fully implemented yet (task 4.1)
        // When task 4.1 is complete, uncomment this assertion to verify Unicode preservation:
        // assert!(content.contains("测试项目") || content.contains("你好世界") || content.contains("🌍"),
        //         "Export should preserve Unicode characters");

        // Clean up
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn test_get_export_dir_idempotent() {
        // Calling get_export_dir multiple times should return the same path
        let dir1 = get_export_dir().unwrap();
        let dir2 = get_export_dir().unwrap();

        assert_eq!(dir1, dir2, "get_export_dir should return consistent path");
        assert!(
            dir1.exists(),
            "Export directory should exist after first call"
        );
    }

    #[test]
    fn test_generate_filename_long_project_name() {
        // Test with a very long project name
        let long_name = "a".repeat(200);
        let data = create_test_transcript(Some(&long_name));
        let result = generate_filename(&data);

        assert!(result.is_ok(), "Should handle long project names");
        let filename = result.unwrap();

        // Verify it still has the correct format
        assert!(filename.starts_with("aink-export-"));
        assert!(filename.ends_with(".md"));
    }

    #[test]
    fn test_generate_filename_only_special_characters() {
        // Test with project name containing only special characters
        let data = create_test_transcript(Some("/\\:*?\"<>|"));
        let result = generate_filename(&data);

        assert!(
            result.is_ok(),
            "Should handle project name with only special chars"
        );
        let filename = result.unwrap();

        // All special chars should be replaced with hyphens
        assert!(
            filename.contains("---------"),
            "Special chars should be replaced"
        );
    }

    #[test]
    fn test_export_session_with_empty_conversation() {
        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("test-session.json");

        let data = create_test_transcript(Some("test-empty-conversation"));

        // Export with empty conversation array
        let result = export_session(&session_path, &data, Some(&[]));

        assert!(
            result.is_ok(),
            "export_session with empty conversation should succeed"
        );

        let output_path = result.unwrap();
        assert!(output_path.exists(), "Export file should exist");

        // Clean up
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn test_export_session_path_contains_project_info() {
        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("my-awesome-project-session.json");

        let data = create_test_transcript(Some("my-awesome-project"));

        let result = export_session(&session_path, &data, None);
        assert!(result.is_ok(), "export_session should succeed");

        let output_path = result.unwrap();
        let filename = output_path.file_name().unwrap().to_string_lossy();

        // Verify the filename contains the project name
        assert!(
            filename.contains("my-awesome-project"),
            "Filename should contain project name: {}",
            filename
        );

        // Clean up
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn test_generate_filename_with_whitespace() {
        // Test project names with various whitespace
        let test_cases = vec![
            "project name",
            "  project  ",
            "project\tname",
            "project\nname",
        ];

        for project_name in test_cases {
            let data = create_test_transcript(Some(project_name));
            let result = generate_filename(&data);

            assert!(
                result.is_ok(),
                "Should handle whitespace in project name: {:?}",
                project_name
            );
            let filename = result.unwrap();

            // Verify basic format is maintained
            assert!(filename.starts_with("aink-export-"));
            assert!(filename.ends_with(".md"));
        }
    }

    #[test]
    fn test_export_session_creates_valid_path() {
        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("test.json");
        let data = create_test_transcript(Some("test"));

        let result = export_session(&session_path, &data, None);
        assert!(result.is_ok());

        let output_path = result.unwrap();

        // Verify the path is absolute
        assert!(output_path.is_absolute(), "Output path should be absolute");

        // Verify the path has a parent directory
        assert!(
            output_path.parent().is_some(),
            "Output path should have a parent directory"
        );

        // Verify the filename is valid
        assert!(
            output_path.file_name().is_some(),
            "Output path should have a filename"
        );

        // Clean up
        let _ = std::fs::remove_file(&output_path);
    }

    // Property-based tests
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        // Feature: export-functionality, Property 8: Filename Format
        // **Validates: Requirements 3.3, 3.4**
        proptest! {
            #[test]
            fn prop_filename_format(
                project_name in prop::option::of(
                    prop::string::string_regex("[a-zA-Z0-9 _./\\\\:*?\"<>|\\-]{0,100}").unwrap()
                )
            ) {
                // Create test data with the generated project name
                let data = create_test_transcript(project_name.as_deref());

                // Generate filename
                let result = generate_filename(&data);
                prop_assert!(result.is_ok(), "generate_filename should always succeed");

                let filename = result.unwrap();

                // Verify the filename matches the expected pattern:
                // ^aink-export-.+-\d{8}-\d{6}\.md$

                // 1. Must start with "aink-export-"
                prop_assert!(
                    filename.starts_with("aink-export-"),
                    "Filename must start with 'aink-export-', got: {}", filename
                );

                // 2. Must end with ".md"
                prop_assert!(
                    filename.ends_with(".md"),
                    "Filename must end with '.md', got: {}", filename
                );

                // 3. Extract the middle part (project-name-YYYYMMDD-HHMMSS)
                let without_prefix = filename.strip_prefix("aink-export-").unwrap();
                let without_suffix = without_prefix.strip_suffix(".md").unwrap();

                // 4. Split from the right to get timestamp parts
                let parts: Vec<&str> = without_suffix.rsplitn(3, '-').collect();

                // Should have at least 3 parts: time, date, and project name (which may contain hyphens)
                prop_assert!(
                    parts.len() >= 3,
                    "Filename should have at least 3 parts (time, date, project-name), got: {} parts in '{}'",
                    parts.len(), filename
                );

                let time_part = parts[0];
                let date_part = parts[1];

                // 5. Verify date format: 8 digits (YYYYMMDD)
                prop_assert_eq!(
                    date_part.len(), 8,
                    "Date part should be 8 digits (YYYYMMDD), got: {} in '{}'",
                    date_part, filename
                );
                prop_assert!(
                    date_part.chars().all(|c| c.is_ascii_digit()),
                    "Date part should be all digits, got: {} in '{}'",
                    date_part, filename
                );

                // 6. Verify time format: 6 digits (HHMMSS)
                prop_assert_eq!(
                    time_part.len(), 6,
                    "Time part should be 6 digits (HHMMSS), got: {} in '{}'",
                    time_part, filename
                );
                prop_assert!(
                    time_part.chars().all(|c| c.is_ascii_digit()),
                    "Time part should be all digits, got: {} in '{}'",
                    time_part, filename
                );

                // 7. Verify project name part exists (at least one character)
                let project_part = parts[2..].join("-");
                prop_assert!(
                    !project_part.is_empty(),
                    "Project name part should not be empty in '{}'", filename
                );

                // 8. Verify special characters are replaced
                // The project name should not contain any of these characters: / \ : * ? " < > |
                let forbidden_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
                for ch in forbidden_chars {
                    prop_assert!(
                        !project_part.contains(ch),
                        "Project name part should not contain '{}', got: {} in '{}'",
                        ch, project_part, filename
                    );
                }
            }
        }
    }
}
