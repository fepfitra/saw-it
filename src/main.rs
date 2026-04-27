use clap::{CommandFactory, Parser};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::channel;
use std::time::Duration;

fn load_gitignore_patterns(watch_path: &Path) -> Vec<String> {
    let gitignore_path = watch_path.join(".gitignore");
    if !gitignore_path.exists() {
        return Vec::new();
    }

    let content = match fs::read_to_string(&gitignore_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .map(|line| line.trim().to_string())
        .collect()
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, override_usage = "saw --it <PATH> --do <COMMAND>")]
struct Args {
    #[arg(long = "it")]
    path: Option<String>,

    #[arg(long = "do")]
    command: Option<String>,

    #[arg(short = 'c', long = "clear")]
    clear: bool,

    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    #[arg(short = 'r', long = "restart")]
    restart: bool,

    #[arg(short = 'e', long = "exclude", value_delimiter = ' ', num_args = 1..)]
    exclude: Vec<String>,
}

fn main() -> notify::Result<()> {
    let args = Args::parse();

    let (path_str, command_str) = match (args.path, args.command) {
        (Some(p), Some(c)) => (p, c),
        _ => {
            let _ = Args::command().print_help();
            return Ok(());
        }
    };

    let raw_path = Path::new(&path_str);
    let canonical_path = raw_path
        .canonicalize()
        .unwrap_or_else(|_| raw_path.to_path_buf());

    let (watch_path, target_file) = if canonical_path.is_file() {
        (
            canonical_path.parent().unwrap().to_path_buf(),
            Some(canonical_path.clone()),
        )
    } else {
        (canonical_path.clone(), None)
    };

    let clear_screen = args.clear;
    let verbose = args.verbose;
    let restart = args.restart;

    let gitignore_patterns = load_gitignore_patterns(&watch_path);
    let mut exclude_patterns: Vec<&str> = gitignore_patterns.iter().map(|s| s.as_str()).collect();
    exclude_patterns.extend(args.exclude.iter().map(|s| s.as_str()));

    if verbose {
        println!("Watching path: {:?}", watch_path);
        if let Some(ref target) = target_file {
            println!("Targeting specific file: {:?}", target);
        }
        println!("Command to run: '{}'", command_str);
        println!("Restart on change: {}", restart);
        if !exclude_patterns.is_empty() {
            println!("Excluding: {:?}", exclude_patterns);
        }
        println!("Waiting for changes...");
    }

    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&watch_path, RecursiveMode::Recursive)?;

    let mut current_child: Option<std::process::Child> = None;

    if verbose {
        println!("--- Executing (Initial Run): {} ---", command_str);
    }

    let initial_cmd = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", &command_str]).spawn()
    } else {
        Command::new("sh").arg("-c").arg(&command_str).spawn()
    };

    match initial_cmd {
        Ok(child) => {
            current_child = Some(child);
        }
        Err(e) => eprintln!("Failed to start initial command: {}", e),
    }

    loop {
        if let Some(mut child) = current_child.take() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if verbose {
                        if status.success() {
                            println!("--- Success ---");
                        } else {
                            println!("--- Failed ({}) ---", status);
                        }
                    }
                    current_child = None;
                }
                Ok(None) => {
                    current_child = Some(child);
                }
                Err(e) => {
                    println!("Error waiting for process: {}", e);
                    current_child = None;
                }
            }
        }

        let event_result = if current_child.is_some() {
            rx.recv_timeout(Duration::from_millis(100))
        } else {
            rx.recv()
                .map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected)
        };

        match event_result {
            Ok(Ok(event)) => {
                if let Some(ref target) = target_file {
                    let hits_target = event
                        .paths
                        .iter()
                        .any(|p| p.canonicalize().ok().as_ref() == Some(target) || p == target);

                    if !hits_target {
                        continue;
                    }
                }

                let should_exclude = event.paths.iter().any(|p| {
                    let path_str = p.to_string_lossy();
                    exclude_patterns.iter().any(|pattern| {
                        if pattern.ends_with('/') {
                            path_str.contains(pattern)
                        } else {
                            path_str.ends_with(pattern)
                                || path_str.contains(&format!("/{}/", pattern))
                        }
                    })
                });

                if should_exclude {
                    continue;
                }

                use notify::event::ModifyKind;
                if !matches!(
                    event.kind,
                    EventKind::Modify(ModifyKind::Data(_))
                        | EventKind::Modify(ModifyKind::Any)
                        | EventKind::Modify(ModifyKind::Metadata(_))
                        | EventKind::Create(_)
                        | EventKind::Remove(_)
                ) {
                    continue;
                }

                let debounce_duration = Duration::from_millis(100);
                while rx.recv_timeout(debounce_duration).is_ok() {}

                if verbose {
                    println!("Change detected: {:?}", event.kind);
                }

                if let Some(mut child) = current_child.take() {
                    if restart {
                        if verbose {
                            println!("--- Terminating previous process ---");
                        }
                        let _ = child.kill();
                        let _ = child.wait();
                    } else {
                        if verbose {
                            println!("--- Waiting for previous process to finish ---");
                        }
                        let status = child.wait();
                        if verbose {
                            match status {
                                Ok(s) => {
                                    if s.success() {
                                        println!("--- Success ---");
                                    } else {
                                        println!("--- Failed ({}) ---", s);
                                    }
                                }
                                Err(e) => println!("Error waiting: {}", e),
                            }
                        }
                    }
                }

                if clear_screen {
                    print!("\x1B[2J\x1B[1;1H");
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }

                if verbose {
                    println!("--- Executing: {} ---", command_str);
                }

                let cmd_result = if cfg!(target_os = "windows") {
                    Command::new("cmd").args(["/C", &command_str]).spawn()
                } else {
                    Command::new("sh").arg("-c").arg(&command_str).spawn()
                };

                match cmd_result {
                    Ok(child) => {
                        current_child = Some(child);
                    }
                    Err(e) => eprintln!("Failed to start command: {}", e),
                }
            }
            Ok(Err(e)) => println!("Watch error: {:?}", e),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                println!("Channel disconnected");
                break;
            }
        }
    }

    Ok(())
}
