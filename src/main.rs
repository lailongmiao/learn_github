use std::process::Command;

mod scripting;
mod agent_config;

fn main() {

     let mut cmd = Command::new("C:\\Program Files\\nextrmm-agent\\runtime\\deno\\deno.exe");
    let deno_args = vec![
        "run".to_string(),
        "C:\\Users\\29693\\test.ts".to_string(),
    ];
    cmd.args(&deno_args);
    cmd.spawn().expect("TODO: panic message");
}

