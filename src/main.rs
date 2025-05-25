use clap::{Parser, Subcommand};
use glob::glob;
use std::path::PathBuf;
use std::process::exit;
use translate_dir_lib::{project, project_config::ProjectConfig, Language}; // Add this import

#[derive(Parser, Debug)]
#[clap(author = "Paris Innovation Laboratory", version, about = "CLI for document/directory translation", long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    // ... Init command ...
    Init {
        name: String,
        #[clap(short, long, value_parser, default_value = ".")]
        path: PathBuf,
    },
    #[clap(alias = "p")]
    Project {
        #[clap(subcommand)]
        action: ProjectAction,
        #[clap(short, long, value_parser, global = true, default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum ProjectAction {
    // ... SetSource, AddTargetLang, RemoveTargetLang, Sync ...
    SetSource {
        dir_name: String,
        #[clap(value_enum)]
        language: Language,
    },
    AddTargetLang {
        #[clap(value_enum)]
        language: Language,
    },
    RemoveTargetLang {
        #[clap(value_enum)]
        language: Language,
    },
    Sync,

    /// Marks one or more files/patterns in the source directory as translatable.
    /// Accepts multiple file paths or glob patterns (e.g., "*.txt", "docs/*.md").
    /// Note: Shells might expand globs; quote them if needed: "src/*.rs"
    MarkTranslatable {
        /// Paths or glob patterns of files to mark as translatable.
        #[clap(required = true, num_args = 1..)]
        file_patterns: Vec<String>,
    },
    /// Marks one or more files/patterns in the source directory as untranslatable.
    /// Accepts multiple file paths or glob patterns (e.g., "*.log", "images/*").
    /// Note: Shells might expand globs; quote them if needed: "config/*.json"
    MarkUntranslatable {
        /// Paths or glob patterns of files to mark as untranslatable.
        #[clap(required = true, num_args = 1..)]
        file_patterns: Vec<String>,
    },
    ListTranslatable,
    TranslateFile {
        file_path: PathBuf,
        #[clap(value_enum)]
        target_language: Language,
    },
    TranslateAll {
        #[clap(value_enum)]
        target_language: Language,
    },
    Info,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name, path } => {
            handle_init(&name, path);
        }
        Commands::Project { action, path } => match project::load(path.clone()) {
            Ok(mut proj) => {
                handle_project_action(&mut proj, action, &path);
            }
            Err(e) => {
                eprintln!("Error loading project: {}. Ensure you are in a project directory or specify a valid path.", e);
                exit(1);
            }
        },
    }
}

fn handle_init(name: &str, path: PathBuf) {
    match project::init(name, path.clone()) {
        Ok(_) => println!(
            "Successfully initialized project '{}' in '{}'",
            name,
            path.canonicalize().unwrap_or(path).display()
        ),
        Err(e) => {
            eprintln!("Error initializing project: {}", e);
            exit(1);
        }
    }
}

fn process_file_patterns<F>(
    proj: &mut project::Project,
    file_patterns: Vec<String>,
    action_fn: F,
    action_name: &str,
) where
    F: Fn(
        &mut project::Project,
        PathBuf,
    ) -> Result<(), translate_dir_lib::errors::project_errors::AddTranslatableFileError>,
{
    let mut success_count = 0;
    let mut error_count = 0;
    let mut no_match_patterns = Vec::new();

    for pattern_str in file_patterns {
        let mut pattern_matched_at_least_one_file = false;
        // Try glob expansion first
        match glob(&pattern_str) {
            Ok(paths) => {
                for entry in paths {
                    match entry {
                        Ok(path) => {
                            pattern_matched_at_least_one_file = true;
                            match action_fn(proj, path.clone()) {
                                Ok(_) => {
                                    println!(
                                        "Successfully marked '{}' as {}.",
                                        path.display(),
                                        action_name
                                    );
                                    success_count += 1;
                                }
                                Err(e) => {
                                    eprintln!(
                                        "Error marking '{}' as {}: {}",
                                        path.display(),
                                        action_name,
                                        e
                                    );
                                    error_count += 1;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "Error processing glob entry for pattern '{}': {}",
                                pattern_str, e
                            );
                            error_count += 1;
                        }
                    }
                }
            }
            Err(e) => {
                // This means the pattern itself is invalid, not that it didn't match.
                eprintln!("Invalid glob pattern '{}': {}", pattern_str, e);
                error_count += 1;
                continue; // Skip to next pattern
            }
        }

        // If glob didn't match anything AND the pattern doesn't look like a typical glob,
        // try treating it as a literal path.
        if !pattern_matched_at_least_one_file
            && !pattern_str.contains('*')
            && !pattern_str.contains('?')
            && !pattern_str.contains('[')
            && !pattern_str.contains('{')
        {
            let path = PathBuf::from(&pattern_str);
            match action_fn(proj, path.clone()) {
                Ok(_) => {
                    println!(
                        "Successfully marked '{}' as {}.",
                        path.display(),
                        action_name
                    );
                    success_count += 1;
                }
                Err(e) => {
                    // If this also fails, it's likely a "NoFile" error, which is fine to report
                    // as "no match" if it was intended as a literal.
                    eprintln!(
                        "Error marking literal path '{}' as {}: {}",
                        path.display(),
                        action_name,
                        e
                    );
                    error_count += 1; // Count as error if specific file not found
                                      // Or, if we want to be more lenient for literal paths that don't exist:
                                      // no_match_patterns.push(pattern_str);
                }
            }
        } else if !pattern_matched_at_least_one_file {
            no_match_patterns.push(pattern_str);
        }
    }

    for pattern_str in no_match_patterns {
        println!(
            "Warning: No files matched the pattern or literal path '{}'.",
            pattern_str
        );
    }

    println!(
        "Action '{}' summary: {} successful, {} errors.",
        action_name, success_count, error_count
    );
    if error_count > 0 {
        exit(1);
    }
}

fn handle_project_action(
    proj: &mut project::Project,
    action: ProjectAction,
    _cwd_or_proj_path: &PathBuf,
) {
    if matches!(
        action,
        ProjectAction::TranslateFile { .. } | ProjectAction::TranslateAll { .. }
    ) {
        if std::env::var("GOOGLE_API_KEY").is_err() {
            eprintln!("Error: The GOOGLE_API_KEY environment variable must be set to use translation features.");
            exit(1);
        }
    }

    match action {
        ProjectAction::SetSource { dir_name, language } => {
            match proj.set_source_dir(&dir_name, language) {
                Ok(_) => println!(
                    "Successfully set source directory to '{}' with language {:?}",
                    dir_name, language
                ),
                Err(e) => {
                    eprintln!("Error setting source directory: {}", e);
                    exit(1);
                }
            }
        }
        ProjectAction::AddTargetLang { language } => match proj.add_lang(language) {
            Ok(_) => println!("Successfully added target language {:?}", language),
            Err(e) => {
                eprintln!("Error adding target language: {}", e);
                exit(1);
            }
        },
        ProjectAction::RemoveTargetLang { language } => match proj.remove_lang(language) {
            Ok(_) => println!("Successfully removed target language {:?}", language),
            Err(e) => {
                eprintln!("Error removing target language: {}", e);
                exit(1);
            }
        },
        ProjectAction::Sync => match proj.sync_files() {
            Ok(_) => println!("Successfully synced untranslatable files."),
            Err(e) => {
                eprintln!("Error syncing files: {}", e);
                exit(1);
            }
        },
        ProjectAction::MarkTranslatable { file_patterns } => {
            process_file_patterns(
                proj,
                file_patterns,
                |p, path| p.make_translatable_file(path),
                "translatable",
            );
        }
        ProjectAction::MarkUntranslatable { file_patterns } => {
            process_file_patterns(
                proj,
                file_patterns,
                |p, path| p.make_untranslatable_file(path),
                "untranslatable",
            );
        }
        ProjectAction::ListTranslatable => match proj.get_translatable_files() {
            Ok(files) => {
                if files.is_empty() {
                    println!("No translatable files found.");
                } else {
                    println!("Translatable files:");
                    for file in files {
                        println!("  {}", file.display());
                    }
                }
            }
            Err(e) => {
                eprintln!("Error listing translatable files: {}", e);
                exit(1);
            }
        },
        ProjectAction::TranslateFile {
            file_path,
            target_language,
        } => {
            let lang_for_print = target_language;
            println!(
                "Translating '{}' to {:?}...",
                file_path.display(),
                lang_for_print
            );
            match proj.translate_file(file_path.clone(), target_language) {
                Ok(_) => println!(
                    "Successfully submitted '{}' for translation to {:?}.",
                    file_path.display(),
                    lang_for_print
                ),
                Err(e) => {
                    eprintln!("Error translating file: {}", e);
                    exit(1);
                }
            }
        }
        ProjectAction::TranslateAll { target_language } => {
            let lang_for_print = target_language;
            println!("Translating all files to {:?}...", lang_for_print);
            match proj.translate_all(target_language) {
                Ok(_) => println!(
                    "Successfully submitted all translatable files for translation to {:?}.",
                    lang_for_print
                ),
                Err(e) => {
                    eprintln!("Error translating all files: {}", e);
                    exit(1);
                }
            }
        }
        ProjectAction::Info => {
            display_project_info(proj.get_config_as_ref(), proj.get_root_path());
        }
    }
}

fn display_project_info(config: &ProjectConfig, root_path: PathBuf) {
    println!("Project Information:");
    println!("  Root Path: {}", root_path.display());
    println!("  Project Name: {}", config.get_name());

    if let Some(src_dir_lang) = config.get_src_dir_as_ref() {
        println!("  Source Language: {:?}", src_dir_lang.get_lang());
        if let Some(src_path) = config.get_src_dir_path() {
            println!("  Source Directory: {}", src_path.display());
        } else {
            println!("  Source Directory: Not set or path invalid");
        }
    } else {
        println!("  Source Language: Not set");
    }

    let target_langs = config.get_lang_dirs_as_ref();
    if target_langs.is_empty() {
        println!("  Target Languages: None");
    } else {
        println!("  Target Languages:");
        for lang_dir in target_langs {
            println!(
                "    - {:?}: {}",
                lang_dir.get_lang(),
                lang_dir.get_dir_as_ref().get_path().display()
            );
        }
    }
}
