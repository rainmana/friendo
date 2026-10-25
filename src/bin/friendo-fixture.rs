//! Native fixture for portable process, session, output, and REA smoke tests.
use std::{
    env, fs,
    io::{self, BufRead, Write},
    process::{self, Command, Stdio},
    thread,
    time::Duration,
};

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn friendo_fixture_exported(value: u64) -> u64 {
    friendo_fixture_internal(value).wrapping_add(7)
}

#[inline(never)]
fn friendo_fixture_internal(value: u64) -> u64 {
    value.rotate_left(3) ^ 0x0046_5249_454e_444f
}

fn emit(stdout_bytes: usize, stderr_bytes: usize) -> io::Result<()> {
    let err = thread::spawn(move || fill(io::stderr().lock(), stderr_bytes, b'E'));
    let out = fill(io::stdout().lock(), stdout_bytes, b'O');
    err.join()
        .map_err(|_| io::Error::other("stderr writer panicked"))??;
    out
}

fn fill(mut writer: impl Write, mut bytes: usize, value: u8) -> io::Result<()> {
    let buffer = [value; 8192];
    while bytes > 0 {
        let count = bytes.min(buffer.len());
        writer.write_all(&buffer[..count])?;
        bytes -= count;
    }
    writer.flush()
}

fn spawn_child(pid_file: &str, milliseconds: u64) -> io::Result<process::Child> {
    let mut child = Command::new(env::current_exe()?)
        .arg("sleep")
        .arg(milliseconds.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    if let Err(error) = fs::write(pid_file, child.id().to_string()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    println!("child_pid={}", child.id());
    io::stdout().flush()?;
    Ok(child)
}

fn number(args: &[String], index: usize, default: u64) -> u64 {
    args.get(index)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(default)
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str).unwrap_or("symbols") {
        "emit" => emit(
            number(&args, 1, 1024) as usize,
            number(&args, 2, 1024) as usize,
        )?,
        "sleep" => thread::sleep(Duration::from_millis(number(&args, 1, 60_000))),
        "tree" => {
            let file = args
                .get(1)
                .ok_or_else(|| io::Error::other("tree requires pid file"))?;
            let mut child = spawn_child(file, number(&args, 2, 60_000))?;
            let _ = child.wait()?;
        }
        "orphan" => {
            let file = args
                .get(1)
                .ok_or_else(|| io::Error::other("orphan requires pid file"))?;
            let _child = spawn_child(file, number(&args, 2, 60_000))?;
        }
        "stdin" => {
            // Emit first so a parent that writes stdin before reading pipes deadlocks.
            emit(
                number(&args, 1, 262_144) as usize,
                number(&args, 2, 262_144) as usize,
            )?;
            let count = io::copy(&mut io::stdin().lock(), &mut io::sink())?;
            println!("\nstdin_bytes={count}");
        }
        "args" => {
            for arg in args.iter().skip(1) {
                println!("{arg}");
            }
        }
        "secret" => {
            let secret = env::var(
                args.get(1)
                    .map(String::as_str)
                    .unwrap_or("FRIENDO_FIXTURE_SECRET"),
            )
            .unwrap_or_default();
            let bytes = secret.as_bytes();
            print!("before:");
            io::stdout().write_all(&bytes[..bytes.len() / 2])?;
            io::stdout().flush()?;
            thread::sleep(Duration::from_millis(40));
            io::stdout().write_all(&bytes[bytes.len() / 2..])?;
            println!(":after");
            eprintln!("stderr:{secret}");
        }
        "escapes" => {
            println!("before\x1b[31mred\x1b[0m\x1b]52;c;injected\x07after\x1bPpayload\x1b\\end");
            io::stdout().flush()?;
        }
        "session" => {
            println!("ready pid={}", process::id());
            io::stdout().flush()?;
            let mut children = Vec::new();
            for line in io::stdin().lock().lines() {
                let line = line?;
                let (command, rest) = line.split_once(' ').unwrap_or((&line, ""));
                match command {
                    "echo" => println!("{rest}"),
                    "pwd" => println!("{}", env::current_dir()?.display()),
                    "env" => println!("{}", env::var(rest).unwrap_or_default()),
                    "sleep" => thread::sleep(Duration::from_millis(rest.parse().unwrap_or(100))),
                    "emit" => {
                        let amounts: Vec<_> = rest.split_whitespace().collect();
                        emit(
                            amounts.first().and_then(|n| n.parse().ok()).unwrap_or(0),
                            amounts.get(1).and_then(|n| n.parse().ok()).unwrap_or(0),
                        )?;
                    }
                    "spawn" => children.push(spawn_child(rest, 60_000)?),
                    "exit" => process::exit(rest.parse().unwrap_or(0)),
                    _ => println!("unknown command: {command}"),
                }
                io::stdout().flush()?;
            }
            for mut child in children {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        "symbols" => println!(
            "friendo fixture synthetic symbol result: {}",
            friendo_fixture_exported(std::hint::black_box(42))
        ),
        other => return Err(io::Error::other(format!("unknown fixture mode: {other}"))),
    }
    Ok(())
}
