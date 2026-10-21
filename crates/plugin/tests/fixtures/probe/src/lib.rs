//! A plugin that exercises every function of the plugin API, for the tests.

wit_bindgen::generate!({
    path: "../../../wit",
    world: "plugin",
});

use birchpad::plugin::host;
use birchpad::plugin::types::{ButtonControl, CheckControl, Control, TextControl};

struct Probe;

fn command(id: &str, title: &str) -> Command {
    Command {
        id: id.into(),
        title: title.into(),
        context_menu: id == "upper",
    }
}

impl Guest for Probe {
    fn commands() -> Vec<Command> {
        vec![
            command("upper", "Upper Case"),
            command("hello", "Say Hello"),
            command("select-all", "Select All"),
            command("describe", "Describe"),
            command("panel", "Show Panel"),
            command("fail", "Fail"),
            command("bad-range", "Bad Range"),
            command("spin", "Spin"),
            command("trap", "Trap"),
            command("grow", "Grow"),
            command("settings", "Settings"),
            command("files", "Files"),
            command("controls", "Controls"),
            command("tasks", "Tasks"),
        ]
    }

    fn run(command: String) -> Result<(), String> {
        match command.as_str() {
            "upper" => {
                let mut selections = host::selections();
                selections.sort_by_key(|s| std::cmp::Reverse(s.anchor.min(s.head)));
                for s in selections {
                    let (start, end) = (s.anchor.min(s.head), s.anchor.max(s.head));
                    let text = host::text_range(start, end)?;
                    host::replace(start, end, &text.to_uppercase())?;
                }
                Ok(())
            }
            "hello" => {
                host::insert("Hello from probe");
                host::message("said hello");
                Ok(())
            }
            "select-all" => host::run_command("edit.select-all", ""),
            "describe" => {
                let path = host::path().unwrap_or_else(|| "(none)".into());
                host::message(&format!(
                    "{} bytes, {}, {path}, {:?}",
                    host::length(),
                    host::language(),
                    host::text()
                ));
                host::set_selections(&[host::Selection { anchor: 0, head: host::length() }])
            }
            "panel" => {
                host::show_panel("Probe", &["one".to_string(), "two".to_string()]);
                Ok(())
            }
            "fail" => Err("it failed on purpose".into()),
            "bad-range" => host::replace(99, 100, "x"),
            "spin" => {
                let mut n: u64 = 0;
                loop {
                    n = std::hint::black_box(n.wrapping_add(1));
                }
            }
            "trap" => panic!("trapped"),
            "grow" => {
                let mut hoard: Vec<Vec<u8>> = Vec::new();
                loop {
                    hoard.push(vec![1; 16 << 20]);
                    std::hint::black_box(&hoard);
                }
            }
            // Counts its runs in a setting, and says what the greeting is.
            "settings" => {
                let runs: u32 = host::setting("runs")
                    .and_then(|runs| runs.parse().ok())
                    .unwrap_or(0);
                host::set_setting("runs", &(runs + 1).to_string())?;
                let greeting = host::setting("greeting").unwrap_or_else(|| "(none)".into());
                host::message(&format!("greeting {greeting}, runs {}", runs + 1));
                Ok(())
            }
            // Copies the file the setting `source` names into `target`, then opens the copy.
            "files" => {
                let source = host::setting("source").ok_or("no source")?;
                let target = host::setting("target").ok_or("no target")?;
                let text = host::read_file(&source)?;
                host::write_file(&target, &text.to_uppercase())?;
                host::open_file(&target)
            }
            "controls" => {
                host::show_controls(
                    "Probe controls",
                    &[
                        Control::Label("A label".into()),
                        Control::Button(ButtonControl {
                            id: "go".into(),
                            title: "Go".into(),
                        }),
                        Control::Separator,
                        Control::TextField(TextControl {
                            id: "name".into(),
                            value: "typed".into(),
                        }),
                        Control::Checkbox(CheckControl {
                            id: "loud".into(),
                            title: "Loud".into(),
                            checked: true,
                        }),
                    ],
                );
                Ok(())
            }
            // Starts the tasks named by the selected text, separated by spaces.
            "tasks" => {
                let text = host::text();
                for task in text.split_whitespace() {
                    host::start_task(task, "2 3 4")?;
                }
                Ok(())
            }
            other => Err(format!("unknown command {other}")),
        }
    }

    fn on_event(event: Event) {
        host::message(&format!("event {event:?}"));
    }

    fn on_panel_click(line: u32) {
        host::message(&format!("clicked {line}"));
    }

    fn on_control(id: String, value: String) {
        host::message(&format!("control {id} = {value}"));
    }

    fn on_message(message: String) {
        host::message(&format!("message {message}"));
    }

    fn run_task(task: String, input: String) -> Result<String, String> {
        match task.as_str() {
            "sum" => {
                let sum: u64 = input
                    .split_whitespace()
                    .filter_map(|number| number.parse::<u64>().ok())
                    .sum();
                host::message(&format!("summing, document of {} bytes", host::length()));
                Ok(sum.to_string())
            }
            "fail" => Err("the task failed on purpose".into()),
            "spin" => {
                let mut n: u64 = 0;
                loop {
                    n = std::hint::black_box(n.wrapping_add(1));
                }
            }
            other => Err(format!("unknown task {other}")),
        }
    }

    fn on_task_done(task: String, outcome: Result<String, String>) {
        match outcome {
            Ok(value) => host::message(&format!("task {task} gave {value}")),
            Err(error) => host::message(&format!("task {task} failed: {error}")),
        }
    }
}

export!(Probe);
