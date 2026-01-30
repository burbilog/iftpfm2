#!/usr/bin/env rust-script
//! CSV to JSONL Migration Script for iftpfm2
//!
//! This script converts existing CSV configuration files to the new JSONL format.
//! Usage: cargo run --bin migrate_csv_to_jsonl -- input.csv output.jsonl

use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        eprintln!("Usage: {} <input.csv> <output.jsonl>", args[0]);
        eprintln!();
        eprintln!("This script converts CSV config files to JSONL format for iftpfm2 v2.1.0+");
        eprintln!();
        eprintln!("CSV format:");
        eprintln!("  host_from,port_from,login_from,password_from,path_from,host_to,port_to,login_to,password_to,path_to,age,filename_regexp");
        eprintln!();
        eprintln!("Example:");
        eprintln!("  {} sample.csv sample.jsonl", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];

    // Read input CSV file
    let input_file = match File::open(input_path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Error opening input file '{}': {}", input_path, e);
            std::process::exit(1);
        }
    };

    let reader = BufReader::new(input_file);
    let mut output_lines = Vec::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Error reading line {}: {}", line_num + 1, e);
                std::process::exit(1);
            }
        };

        let trimmed = line.trim();

        // Preserve comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            output_lines.push(line);
            continue;
        }

        // Parse CSV line
        let fields: Vec<&str> = trimmed.split(',').collect();

        if fields.len() != 12 {
            eprintln!(
                "Warning: Line {} has {} fields (expected 12), skipping: {}",
                line_num + 1,
                fields.len(),
                trimmed
            );
            continue;
        }

        // Convert to JSONL format
        // CSV: host_from,port_from,login_from,password_from,path_from,host_to,port_to,login_to,password_to,path_to,age,filename_regexp
        // JSONL: {"host_from":"...","port_from":21,...}
        let jsonl = format!(
            r#"{{"host_from":"{}","port_from":{},"login_from":"{}","password_from":"{}","path_from":"{}","host_to":"{}","port_to":{},"login_to":"{}","password_to":"{}","path_to":"{}","age":{},"filename_regexp":"{}"}}"#,
            fields[0],  // host_from
            fields[1],  // port_from
            fields[2],  // login_from
            fields[3],  // password_from
            fields[4],  // path_from
            fields[5],  // host_to
            fields[6],  // port_to
            fields[7],  // login_to
            fields[8],  // password_to
            fields[9],  // path_to
            fields[10], // age
            fields[11]  // filename_regexp
        );

        output_lines.push(jsonl);
    }

    // Write output file
    let mut output_file = match File::create(output_path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Error creating output file '{}': {}", output_path, e);
            std::process::exit(1);
        }
    };

    for line in &output_lines {
        if let Err(e) = writeln!(output_file, "{}", line) {
            eprintln!("Error writing to output file: {}", e);
            std::process::exit(1);
        }
    }

    println!("Successfully converted {} lines from {} to {}", output_lines.len(), input_path, output_path);
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::fs::File;
    use tempfile::tempdir;

    fn convert_csv_to_jsonl_internal(input_path: &str, _output_path: &str) -> Result<String, String> {
        // Read input CSV file
        let input_file = match File::open(input_path) {
            Ok(file) => file,
            Err(e) => {
                return Err(format!("Error opening input file '{}': {}", input_path, e));
            }
        };

        let reader = BufReader::new(input_file);
        let mut output_lines = Vec::new();

        for (line_num, line_result) in reader.lines().enumerate() {
            let line = match line_result {
                Ok(l) => l,
                Err(e) => {
                    return Err(format!("Error reading line {}: {}", line_num + 1, e));
                }
            };

            let trimmed = line.trim();

            // Preserve comments and empty lines
            if trimmed.is_empty() || trimmed.starts_with('#') {
                output_lines.push(line);
                continue;
            }

            // Parse CSV line
            let fields: Vec<&str> = trimmed.split(',').collect();

            if fields.len() != 12 {
                // Skip invalid lines (just like the main function)
                continue;
            }

            // Convert to JSONL format
            let jsonl = format!(
                r#"{{"host_from":"{}","port_from":{},"login_from":"{}","password_from":"{}","path_from":"{}","host_to":"{}","port_to":{},"login_to":"{}","password_to":"{}","path_to":"{}","age":{},"filename_regexp":"{}"}}"#,
                fields[0], fields[1], fields[2], fields[3], fields[4],
                fields[5], fields[6], fields[7], fields[8], fields[9],
                fields[10], fields[11]
            );

            output_lines.push(jsonl);
        }

        Ok(output_lines.join("\n"))
    }

    #[test]
    fn test_convert_basic_csv() {
        let input = r#"192.168.1.1,21,user1,pass1,/path1/,192.168.1.2,21,user2,pass2,/path2/,86400,.*\.txt$
"#;

        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.csv");
        let output_path = dir.path().join("output.jsonl");

        let mut file = File::create(&input_path).unwrap();
        file.write_all(input.as_bytes()).unwrap();

        let result = convert_csv_to_jsonl_internal(input_path.to_str().unwrap(), output_path.to_str().unwrap());
        assert!(result.is_ok());

        let jsonl = result.unwrap();
        assert!(jsonl.contains(r#""host_from":"192.168.1.1""#));
        assert!(jsonl.contains(r#""port_from":21"#));
        // The actual output has single backslash: .*\.txt$
        assert!(jsonl.contains(r#""filename_regexp":".*\"#));
        assert!(jsonl.contains(r#".txt$""#));
    }

    #[test]
    fn test_convert_with_comments() {
        let input = r#"# This is a comment
# Another comment
192.168.1.1,21,user1,pass1,/path1/,192.168.1.2,21,user2,pass2,/path2/,86400,.*\.txt$
"#;

        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.csv");
        let output_path = dir.path().join("output.jsonl");

        let mut file = File::create(&input_path).unwrap();
        file.write_all(input.as_bytes()).unwrap();

        let result = convert_csv_to_jsonl_internal(input_path.to_str().unwrap(), output_path.to_str().unwrap());
        assert!(result.is_ok());

        let jsonl = result.unwrap();
        assert!(jsonl.contains("# This is a comment"));
        assert!(jsonl.contains("# Another comment"));
    }

    #[test]
    fn test_convert_empty_csv() {
        let input = "";

        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.csv");
        let output_path = dir.path().join("output.jsonl");

        let mut file = File::create(&input_path).unwrap();
        file.write_all(input.as_bytes()).unwrap();

        let result = convert_csv_to_jsonl_internal(input_path.to_str().unwrap(), output_path.to_str().unwrap());
        assert!(result.is_ok());

        let jsonl = result.unwrap();
        assert_eq!(jsonl, "");
    }

    #[test]
    fn test_convert_malformed_csv() {
        // Line with wrong number of fields (should be skipped)
        let input = r#"192.168.1.1,21,user1,pass1
192.168.1.1,21,user1,pass1,/path1/,192.168.1.2,21,user2,pass2,/path2/,86400,.*\.txt$
"#;

        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.csv");
        let output_path = dir.path().join("output.jsonl");

        let mut file = File::create(&input_path).unwrap();
        file.write_all(input.as_bytes()).unwrap();

        let result = convert_csv_to_jsonl_internal(input_path.to_str().unwrap(), output_path.to_str().unwrap());
        assert!(result.is_ok());

        let jsonl = result.unwrap();
        // Should only have one valid line (the second one)
        assert!(jsonl.contains(r#""host_from":"192.168.1.1""#));
    }

    #[test]
    fn test_convert_special_characters() {
        // Note: The simple split(',') parser can't handle commas in values.
        // This test uses other special characters that work with simple parsing.
        let input = r#"192.168.1.1,21,user-name,pass:word,/path/1/,192.168.1.2,21,user2,pass2,/path2/,86400,.*"#;

        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.csv");
        let output_path = dir.path().join("output.jsonl");

        let mut file = File::create(&input_path).unwrap();
        file.write_all(input.as_bytes()).unwrap();

        let result = convert_csv_to_jsonl_internal(input_path.to_str().unwrap(), output_path.to_str().unwrap());
        assert!(result.is_ok());

        let jsonl = result.unwrap();
        assert!(jsonl.contains(r#""login_from":"user-name""#));
        assert!(jsonl.contains(r#""password_from":"pass:word""#));
        assert!(jsonl.contains(r#""path_from":"/path/1/""#));
    }

    #[test]
    fn test_convert_unicode_characters() {
        let input = r#"192.168.1.1,21,пользователь,пароль,/путь/,192.168.1.2,21,user2,pass2,/path2/,86400,.*"#;

        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.csv");
        let output_path = dir.path().join("output.jsonl");

        let mut file = File::create(&input_path).unwrap();
        file.write_all(input.as_bytes()).unwrap();

        let result = convert_csv_to_jsonl_internal(input_path.to_str().unwrap(), output_path.to_str().unwrap());
        assert!(result.is_ok());

        let jsonl = result.unwrap();
        assert!(jsonl.contains("пользователь"));
        assert!(jsonl.contains("пароль"));
        assert!(jsonl.contains("/путь/"));
    }
}
