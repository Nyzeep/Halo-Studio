use std::io::{self, BufRead, Write};

use halo_core::EventBus;
use halo_ipc::{decode_command, encode_error, encode_event, encode_snapshot};
use halo_protocol::{RuntimeCommand, RuntimeEvent};

const EVENT_CAPACITY: usize = 256;

fn main() -> io::Result<()> {
    run_stdio(io::stdin().lock(), io::stdout().lock())
}

fn run_stdio<R, W>(reader: R, mut writer: W) -> io::Result<()>
where
    R: BufRead,
    W: Write,
{
    let mut bus = EventBus::new(EVENT_CAPACITY);

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let command = match decode_command(&line) {
            Ok(command) => command,
            Err(error) => {
                writeln!(writer, "{}", encode_error(&error.to_string()))?;
                writer.flush()?;
                continue;
            }
        };

        match command {
            RuntimeCommand::CreateRun {
                run_id,
                agent_id,
                prompt,
            } => {
                if bus.snapshot(&run_id).is_some() {
                    writeln!(
                        writer,
                        "{}",
                        encode_error(&format!("duplicate run id: {run_id}"))
                    )?;
                } else {
                    emit_fake_run(&mut bus, &mut writer, &run_id, &agent_id, &prompt)?;
                }
            }
            RuntimeCommand::GetSnapshot { run_id } => match bus.snapshot(&run_id) {
                Some(snapshot) => writeln!(writer, "{}", encode_snapshot(&snapshot))?,
                None => writeln!(
                    writer,
                    "{}",
                    encode_error(&format!("snapshot not found: {run_id}"))
                )?,
            },
            RuntimeCommand::Shutdown => break,
        }

        writer.flush()?;
    }

    Ok(())
}

fn emit_fake_run<W: Write>(
    bus: &mut EventBus,
    writer: &mut W,
    run_id: &str,
    agent_id: &str,
    prompt: &str,
) -> io::Result<()> {
    let events = [
        RuntimeEvent::new(run_id, agent_id, 1, "run.state", "running"),
        RuntimeEvent::new(
            run_id,
            agent_id,
            2,
            "message.delta",
            format!("Prompt accepted: {prompt}"),
        ),
        RuntimeEvent::new(
            run_id,
            agent_id,
            3,
            "tool.call",
            "fake-runtime.prepare_workspace",
        ),
        RuntimeEvent::new(run_id, agent_id, 4, "run.state", "completed"),
    ];

    for event in events {
        match bus.append(event.clone()) {
            Ok(()) => writeln!(writer, "{}", encode_event(&event))?,
            Err(error) => writeln!(writer, "{}", encode_error(&error.to_string()))?,
        }
    }

    Ok(())
}
