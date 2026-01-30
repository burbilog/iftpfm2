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
