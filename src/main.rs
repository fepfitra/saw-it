use clap::{Parser, CommandFactory};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::channel;
use std::time::Duration;

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
    let canonical_path = raw_path.canonicalize().unwrap_or_else(|_| raw_path.to_path_buf());
    
    let (watch_path, target_file) = if canonical_path.is_file() {
        (canonical_path.parent().unwrap().to_path_buf(), Some(canonical_path.clone()))
    } else {
        (canonical_path.clone(), None)
    };

    let clear_screen = args.clear;
    let verbose = args.verbose;
    let restart = args.restart;

    if verbose {
        println!("Watching path: {:?}", watch_path);
        if let Some(ref target) = target_file {
            println!("Targeting specific file: {:?}", target);
        }
        println!("Command to run: '{}'", command_str);
        println!("Restart on change: {}", restart);
        println!("Waiting for changes...");
    }

    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&watch_path, RecursiveMode::Recursive)?;

    let mut current_child: Option<std::process::Child> = None;

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
            rx.recv().map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected)
        };

        match event_result {
            Ok(Ok(event)) => {
                if let Some(ref target) = target_file {
                    let hits_target = event.paths.iter().any(|p| {
                        p.canonicalize().ok().as_ref() == Some(target) || p == target
                    });
                    
                    if !hits_target {
                        continue;
                    }
                }

                use notify::event::ModifyKind;
                if !matches!(event.kind, EventKind::Modify(ModifyKind::Metadata(_))) {
                    continue;
                }

                let debounce_duration = Duration::from_millis(100);
                while let Ok(_) = rx.recv_timeout(debounce_duration) {}

                if verbose {
                    println!("Change detected: {:?}", event.kind);
                }

                if let Some(mut child) = current_child.take() {
                    if restart {
                        if verbose { println!("--- Terminating previous process ---"); }
                        let _ = child.kill();
                        let _ = child.wait();
                    } else {
                        if verbose { println!("--- Waiting for previous process to finish ---"); }
                        let status = child.wait();
                         if verbose {
                            match status {
                                Ok(s) => {
                                    if s.success() { println!("--- Success ---"); } 
                                    else { println!("--- Failed ({}) ---", s); }
                                },
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
                    Command::new("cmd")
                        .args(["/C", &command_str])
                        .spawn()
                } else {
                    Command::new("sh")
                        .arg("-c")
                        .arg(&command_str)
                        .spawn()
                };

                match cmd_result {
                    Ok(child) => {
                        current_child = Some(child);
                    },
                    Err(e) => eprintln!("Failed to start command: {}", e),
                }
            },
            Ok(Err(e)) => println!("Watch error: {:?}", e),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                continue;
            },
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                println!("Channel disconnected");
                break;
            }
        }
    }

    Ok(())
}
