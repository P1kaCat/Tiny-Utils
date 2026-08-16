use crate::network::NetworkManager;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::thread;
use windows_sys::Win32::System::Console::AllocConsole;

pub fn start_console_thread(network: Arc<NetworkManager>) {
    thread::spawn(move || {
        // Allocate a dedicated console window for the mod
        unsafe {
            AllocConsole();
        }

        // Print header
        println!("\x1b[32m==============================================================\x1b[0m");
        println!("\x1b[1;36m       GLADESYNC - TINY GLADE MULTIPLAYER MOD v0.1.0        \x1b[0m");
        println!("\x1b[32m==============================================================\x1b[0m");
        println!("\x1b[33mCommands:\x1b[0m");
        println!("  \x1b[1;37mhost [port]\x1b[0m     -> Host a multiplayer game (default port 7777)");
        println!("  \x1b[1;37mjoin <ip:port>\x1b[0m  -> Join a host player (e.g. join 127.0.0.1:7777)");
        println!("  \x1b[1;37mstatus\x1b[0m          -> Show current connection & sync status");
        println!("  \x1b[1;37mhelp\x1b[0m            -> Show this help message");
        println!("\x1b[32m==============================================================\x1b[0m\n");

        let stdin = io::stdin();
        let mut handle = stdin.lock();

        loop {
            print!("\x1b[1;32mGladeSync>\x1b[0m ");
            io::stdout().flush().ok();

            let mut line = String::new();
            if handle.read_line(&mut line).is_err() {
                break;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            match parts[0].to_lowercase().as_str() {
                "host" => {
                    let port = if parts.len() > 1 {
                        parts[1].parse::<u16>().unwrap_or(7777)
                    } else {
                        7777
                    };
                    if let Err(e) = network.start_host(port) {
                        println!("\x1b[31m[Error] Failed to host: {}\x1b[0m", e);
                    }
                }
                "join" | "connect" => {
                    if parts.len() < 2 {
                        println!("\x1b[31m[Error] Usage: join <ip:port> (e.g. join 127.0.0.1:7777)\x1b[0m");
                    } else {
                        let target = parts[1];
                        let addr = if target.contains(':') {
                            target.to_string()
                        } else {
                            format!("{}:7777", target)
                        };
                        if let Err(e) = network.connect_to_host(&addr) {
                            println!("\x1b[31m[Error] Failed to connect: {}\x1b[0m", e);
                        }
                    }
                }
                "status" => {
                    println!("\x1b[35m=== GladeSync Status ===\x1b[0m");
                    println!("Active: {}", network.is_active());
                }
                "help" => {
                    println!("\x1b[33mAvailable commands: host [port], join <ip:port>, status, help\x1b[0m");
                }
                _ => {
                    println!("\x1b[31mUnknown command. Type 'help' for options.\x1b[0m");
                }
            }
        }
    });
}
