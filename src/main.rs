use std::{collections::HashMap, iter::Peekable, path::PathBuf};

use clap::Parser;
use serde::Deserialize;

type Result<T> = std::result::Result<T, ()>;

#[derive(Deserialize, Default, Debug)]
#[serde(deny_unknown_fields, default)]
struct Script {
    cmd: Vec<Cmd>,
    verbose: bool,
}

#[derive(Deserialize, Default, Debug)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case", default)]
struct Cmd {
    name: String,
    bind: Vec<String>,
    cmd: Vec<String>,
    cwd: String,
}

struct Env {
    binds: HashMap<String, Vec<String>>,
}

impl Env {
    fn parse_text<I: Iterator<Item = char>>(&self, peek: &mut Peekable<I>) -> Result<String> {
        let mut name = String::new();

        while let Some(c) = peek.next_if(|c| *c != '$') {
            name.push(c);
        }

        if name.is_empty() {
            return Err(());
        }

        Ok(name)
    }

    // $NAME.txt
    //  ^
    fn parse_bind<I: Iterator<Item = char>>(&self, peek: &mut Peekable<I>) -> Result<Vec<String>> {
        if let Some(&'$') = peek.peek() {
            peek.next();
            return Ok(vec!["$".to_string()]);
        }

        let mut name = String::new();

        while let Some(c) = peek.next_if(char::is_ascii_alphanumeric) {
            name.push(c);
        }

        if name.is_empty() {
            return Err(());
        }

        let bind = self
            .binds
            .get(&name)
            .ok_or_else(|| eprintln!("Missing bind for {name}"))?;

        Ok(bind.to_owned())
    }

    fn eval(&self, text: &str) -> Result<Vec<String>> {
        let mut parts: Vec<String> = vec![];
        let mut tmp: Vec<String> = vec![];

        let mut chars = text.chars().peekable();

        loop {
            match chars.peek() {
                Some(&'$') => {
                    chars.next();

                    let bind = self.parse_bind(&mut chars)?;

                    if parts.is_empty() {
                        parts = bind;
                    } else {
                        tmp.clear();
                        for part in parts {
                            for bind_each in &bind {
                                tmp.push(part.clone() + &bind_each);
                            }
                        }
                        parts = std::mem::take(&mut tmp);
                    }
                }
                Some(_) => {
                    let text = self.parse_text(&mut chars)?;

                    if parts.is_empty() {
                        parts.push(text);
                    } else {
                        tmp.clear();
                        for part in parts {
                            tmp.push(part.clone() + &text);
                        }

                        parts = std::mem::take(&mut tmp);
                    }
                }
                None => break,
            }
        }

        Ok(parts)
    }
}

#[test]
fn test_eval() {
    let env = Env {
        binds: [
            ("a".to_string(), vec![]),
            ("b".to_string(), vec!["x".to_string(), "y".to_string()]),
        ]
        .into_iter()
        .collect(),
    };

    assert_eq!(env.eval("$a").unwrap(), Vec::<String>::new());
    assert_eq!(env.eval("a").unwrap(), vec!["a"]);
    assert_eq!(env.eval("$$").unwrap(), vec!["$"]);
    assert_eq!(env.eval("$b").unwrap(), vec!["x", "y"]);
    assert_eq!(env.eval("$b.txt").unwrap(), vec!["x.txt", "y.txt"]);
    assert_eq!(env.eval("output/$b").unwrap(), vec!["output/x", "output/y"]);

    assert_eq!(
        env.eval("output/$b.o").unwrap(),
        vec!["output/x.o", "output/y.o"]
    );

    assert_eq!(
        env.eval("output/$b.o$$").unwrap(),
        vec!["output/x.o$", "output/y.o$"]
    );
}

impl Script {
    fn run(&self, env: &mut Env) -> Result<()> {
        for cmd in &self.cmd {
            if !cmd.name.is_empty() {
                env.binds.insert(cmd.name.to_owned(), cmd.bind.clone());
            }

            if !cmd.cmd.is_empty() {
                let cmd = &cmd.cmd;

                let mut cmd_eval = vec![];
                for arg in cmd {
                    cmd_eval.extend(env.eval(arg)?);
                }

                if cmd_eval.is_empty() {
                    eprintln!("Empty command: {:?}", cmd);
                    return Err(());
                }

                let mut cmd = std::process::Command::new(&cmd_eval[0]);

                cmd.args(&cmd_eval[1..]);

                if self.verbose {
                    eprintln!("tomlsh: => {:?}", cmd);
                }

                let status = cmd
                    .status()
                    .map_err(|err| eprintln!("Failed start command {}: {err}", cmd_eval[0]))?;

                if !status.success() {
                    eprintln!("Command {} failed with {:?}", cmd_eval[0], status.code());
                    return Err(());
                }
            }
        }

        Ok(())
    }
}

#[derive(Parser)]
struct CommandLine {
    /// A .toml file contains script of tomlsh.
    path: PathBuf,

    /// Overwrite .verbose of script
    #[clap(long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let cli = CommandLine::parse();

    let script = std::fs::read_to_string(&cli.path)
        .map_err(|err| eprintln!("Failed to load script {}: {err}", cli.path.display()))?;

    let mut script: Script = toml::from_str(&script)
        .map_err(|err| eprintln!("Failed to parse script {}: {err}", cli.path.display()))?;

    if cli.verbose {
        script.verbose = true;
    }

    let mut env = Env {
        binds: HashMap::new(),
    };

    script.run(&mut env)?;

    Ok(())
}
