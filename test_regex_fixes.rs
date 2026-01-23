use regex::Regex;

fn main() {
    // Test the fixed file detection patterns
    let file_patterns = vec![
        Regex::new(r"(?:analiza|lee|revisa|muestra|ver|check|analyze|read|review|show)\s+([a-zA-Z0-9_./][a-zA-Z0-9_./-]{0,100}?\.[a-zA-Z0-9]{1,10})").unwrap(),
        Regex::new(r"archivo\s+([a-zA-Z0-9_./][a-zA-Z0-9_./-]{0,100}?\.[a-zA-Z0-9]{1,10})").unwrap(),
        Regex::new(r"file\s+([a-zA-Z0-9_./][a-zA-Z0-9_./-]{0,100}?\.[a-zA-Z0-9]{1,10})").unwrap(),
        Regex::new(r"([a-zA-Z0-9_][a-zA-Z0-9_/]{0,100}?\.rs)").unwrap(),
        Regex::new(r"([a-zA-Z0-9_][a-zA-Z0-9_/]{0,100}?\.py)").unwrap(),
        Regex::new(r"([a-zA-Z0-9_][a-zA-Z0-9_/]{0,100}?\.js)").unwrap(),
        Regex::new(r"([a-zA-Z0-9_][a-zA-Z0-9_/]{0,100}?\.ts)").unwrap(),
    ];

    // Test queries
    let test_queries = vec![
        "analiza src/main.rs",
        "lee Cargo.toml",
        "revisa ./config.json",
        "archivo src/lib.rs",
        "file test.py",
        "muestra README.md",
        "ver src/agent/router_orchestrator.rs",
    ];

    println!("Testing file detection patterns:");
    for query in test_queries {
        println!("Query: {}", query);
        for pattern in &file_patterns {
            for cap in pattern.captures_iter(query) {
                if let Some(file_match) = cap.get(1) {
                    println!("  Found: {}", file_match.as_str());
                }
            }
        }
    }

    // Test security scanner patterns
    let critical_patterns = vec![
        Regex::new(r"rm\s+(-[rf]+\s+)*/?$").unwrap(),
        Regex::new(r"rm\s+(-[rf]+\s+)*/\*").unwrap(),
        Regex::new(r"dd\s+if=[^\s]*\sof=/dev/[sh]d[a-z]").unwrap(),
        Regex::new(r">\s*/dev/[sh]d[a-z]").unwrap(),
    ];

    let test_commands = vec![
        "rm -rf /",
        "rm -rf /*",
        "dd if=/dev/zero of=/dev/sda",
        "> /dev/sda",
        "ls -la", // Safe command
    ];

    println!("\nTesting security patterns:");
    for cmd in test_commands {
        println!("Command: {}", cmd);
        let mut risk = "Safe";
        for pattern in &critical_patterns {
            if pattern.is_match(cmd) {
                risk = "Critical";
                break;
            }
        }
        println!("  Risk: {}", risk);
    }

    println!("\nAll patterns compiled and tested successfully!");
}