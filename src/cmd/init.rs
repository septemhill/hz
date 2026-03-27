//! Init command - creates a new Lang project

use std::error::Error;
use std::fs;
use std::path::Path;

/// Initialize a new Lang project
pub fn init_project(name: &str) -> Result<(), Box<dyn Error>> {
    // Create project directory
    let project_path = Path::new(name);
    if project_path.exists() {
        return Err(format!("Directory '{}' already exists", name).into());
    }

    fs::create_dir_all(project_path)?;
    println!("Created project directory: {}", name);

    // Create deps.json file
    let deps_json_content = create_deps_json(name);
    let deps_json_path = project_path.join("deps.json");
    fs::write(&deps_json_path, deps_json_content)?;
    println!("Created deps.json");

    // Create main.lang file
    let main_content = create_main_file();
    let main_path = project_path.join("main.lang");
    fs::write(&main_path, main_content)?;
    println!("Created main.lang");

    // Create src directory
    let src_path = project_path.join("src");
    fs::create_dir_all(&src_path)?;
    println!("Created src/ directory");

    println!("\nProject '{}' initialized successfully!", name);
    println!("\nTo run your project:");
    println!("  cd {}", name);
    println!("  lang run main.lang");

    Ok(())
}

/// Create the package.json content
fn create_deps_json(name: &str) -> String {
    format!(
        r#"{{
  "name": "{}",
  "version": "0.1.0",
  "description": "A Lang project",
  "dependencies": {{
    "io": "*",
    "builtin": "*"
  }},
  "devDependencies": {{}},
  "type": "lang-package"
}}
"#,
        name
    )
}

/// Create the main.lang content
fn create_main_file() -> String {
    "// Main entry point for the project\n\nfn main() {\n    io.println(\"Hello, World!\");\n}\n"
        .to_string()
}
