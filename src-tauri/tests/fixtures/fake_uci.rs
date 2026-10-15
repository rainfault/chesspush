//! Deterministic child process used only by the UCI supervisor tests.
use std::io::{self,BufRead,Write};
fn main() {
    let mut mode=String::new();
    for line in io::stdin().lock().lines() {
        let line=line.unwrap();
        if line=="uci" {
            println!("id name Stockfish fixture");println!("id author ChessPush tests");
            for (name,kind,default) in [("Threads","spin","1"),("Hash","spin","128"),("MultiPV","spin","1"),("UCI_ShowWDL","check","false"),("Clear Hash","button","")] {
                println!("option name {name} type {kind} default {default}");
            }
            println!("uciok");
        } else if line=="isready" {println!("readyok");}
        else if let Some(value)=line.strip_prefix("setoption name TestMode value "){mode=value.to_owned();}
        else if line=="stopread" {std::thread::sleep(std::time::Duration::from_secs(30));}
        else if line=="emitlong" {println!("{}","x".repeat(70000));}
        else if line.starts_with("go ") {
            match mode.as_str() {
                "hang"=>(),
                "crash"=>std::process::exit(7),
                "no-score"=>println!("bestmove e2e4"),
                _=>{
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    let played=line.split("searchmoves ").nth(1).and_then(|s|s.split_whitespace().next()).unwrap_or("e2e4");
                    println!("info depth 12 seldepth 17 score cp 36 wdl 130 800 70 nodes 3000 pv {played}");
                    println!("bestmove {played}");
                }
            }
        } else if line=="stop" {println!("bestmove e2e4");}
        else if line=="quit" {return;}
        io::stdout().flush().unwrap();
    }
}


