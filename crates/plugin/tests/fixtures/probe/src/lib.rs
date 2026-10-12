//! A plugin that exercises every function of the plugin API, for the tests.

wit_bindgen::generate!({
    path: "../../../wit",
    world: "plugin",
});

use birchpad::plugin::host;

struct Probe;

fn command(id: &str, title: &str) -> Command {
    Command {
        id: id.into(),
        title: title.into(),
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
            other => Err(format!("unknown command {other}")),
        }
    }

    fn on_event(event: Event) {
        host::message(&format!("event {event:?}"));
    }

    fn on_panel_click(line: u32) {
        host::message(&format!("clicked {line}"));
    }
}

export!(Probe);
