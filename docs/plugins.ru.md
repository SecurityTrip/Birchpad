# Плагины и скрипты

[English](plugins.md) · Русский

Birchpad выполняет два вида расширений:

- **Плагины**: компоненты WebAssembly, собранные на любом языке, который умеет собирать
  компоненты, устанавливаются через Plugins > Plugins Admin
  ([ADR 0036](adr/0036-webassembly-plugins.md), [ADR 0038](adr/0038-plugin-manager-and-allowlist.md),
  [ADR 0046](adr/0046-plugin-api-0-2.md)).
- **Скрипты**: файлы Rhai в папке `scripts`, запускаются из Plugins > Scripts
  ([ADR 0037](adr/0037-rhai-scripts.md)).

Оба видят один и тот же API: активный документ, его выделения, команды Birchpad, сообщения и
панель. Плагины вдобавок хранят настройки, показывают элементы управления, слышат набор текста,
работают в фоне и, если пользователь разрешил, работают с файлами. Ни те, ни другие не могут
выходить в сеть или работать бесконечно.

## Плагины

Плагин — это папка:

```text
word-count/
├── plugin.toml
└── plugin.wasm
```

`plugin.toml`:

```toml
id = "word-count"            # строчные буквы, цифры и дефисы; так называется папка
name = "Word Count"          # подменю в меню Plugins
version = "1.0.0"
description = "Counts the words of the document."
author = "Jane Doe"
api = "0.2"                  # API плагинов, для которого он собран
entry = "plugin.wasm"        # по умолчанию
events = ["saved"]           # что слышит on-event (и on-message), см. ниже
permissions = ["files"]      # просит у пользователя разрешения работать с файлами

[[settings]]                 # показывается в Preferences > Plugins
key = "count-numbers"        # строчные буквы, цифры, дефисы, точки, подчёркивания
title = "Count numbers as words"
kind = "bool"                # bool, number или text
default = "true"
```

`events` принимает `opened`, `saved`, `closed` и `activated`; `changed` (текст изменился: набор,
команда, отмена) и `selection-changed` (сдвинулись курсор или выделение) — каждое отправляется
после того, как изменение закончено; и `message` — для `-pluginMessage` командной строки.

Компонент реализует мир `plugin` из
[`crates/plugin/wit/birchpad.wit`](../crates/plugin/wit/birchpad.wit):

| Экспорт | Когда вызывается |
|---|---|
| `commands()` | один раз при загрузке: пункты его меню, `{ id, title, context-menu }`; с `context-menu` они есть и в контекстном меню текста |
| `run(command)` | когда выбран один из них (или назначен на клавишу, или проигран в макросе) |
| `on-event(event)` | для событий, перечисленных в `plugin.toml`, с документом, которого они касаются |
| `on-panel-click(line)` | при щелчке по строке панели плагина |
| `on-control(id, value)` | когда используется элемент управления его панели |
| `on-message(message)` | с `-pluginMessage`, если в `events` есть `message` |
| `run-task(task, input)` | в другом потоке, для задачи, которую запустил плагин; возвращает её результат |
| `on-task-done(task, outcome)` | с тем, что вернула задача, на активном документе |

Через интерфейс `host` он читает и правит документ (позиции — смещения в байтах в тексте
UTF-8), задаёт выделения, выполняет команды Birchpad по идентификатору с аргументами в JSON
(`edit.select-all`, `{}`), показывает сообщения и заполняет свою панель строками
(`show-panel`) или элементами управления (`show-controls`: подписи, кнопки, поля ввода,
флажки, разделители). Всё, что вызов делает с документом, — один шаг отмены; команды,
сообщения, панель, файлы для открытия и задачи срабатывают после возврата из вызова. Вызов,
завершившийся ошибкой, ничего не меняет.

- **Настройки**: `setting(key)` и `set-setting(key, value)` хранят текст между запусками
  Birchpad в `plugins/<id>.settings.toml`. Настройки, объявленные в `plugin.toml`, видны в
  Preferences > Plugins и начинаются со значения по умолчанию; остальные — собственные данные
  плагина.
- **Файлы**: `read-file`, `write-file` и `open-file` принимают абсолютные пути. Они работают
  только у плагина, который указал `permissions = ["files"]` и которому пользователь разрешил
  это в Plugins Admin (Allow Files); до тех пор они отклоняются.
- **Работа в фоне**: `start-task(task, input)` выполняет `run-task` вне окна, без документа; её
  результат возвращается через `on-task-done`.

Вызов может работать несколько секунд и занимать 256 МиБ; сверх этого плагин останавливается с
сообщением. Задача может работать в двенадцать раз дольше.

### На Rust

[`crates/plugin/tests/fixtures/probe`](../crates/plugin/tests/fixtures/probe) — плагин, который
использует все функции. С [`wit-bindgen`](https://crates.io/crates/wit-bindgen):

```rust
wit_bindgen::generate!({ path: "wit", world: "plugin" });

use birchpad::plugin::host;

struct WordCount;

impl Guest for WordCount {
    fn commands() -> Vec<Command> {
        vec![Command {
            id: "count".into(),
            title: "Count Words".into(),
            context_menu: true,
        }]
    }

    fn run(_command: String) -> Result<(), String> {
        let words = host::text().split_whitespace().count();
        host::message(&format!("{words} words"));
        Ok(())
    }

    fn on_event(_event: Event) {}
    fn on_panel_click(_line: u32) {}
    fn on_control(_id: String, _value: String) {}
    fn on_message(_message: String) {}
    fn run_task(_task: String, _input: String) -> Result<String, String> {
        Err("no tasks".into())
    }
    fn on_task_done(_task: String, _outcome: Result<String, String>) {}
}

export!(WordCount);
```

Соберите его для `wasm32-unknown-unknown` как `cdylib` и сделайте компонент с помощью
[`wasm-tools`](https://crates.io/crates/wasm-tools):

```bash
cargo build --release --target wasm32-unknown-unknown
```

```bash
wasm-tools component new target/wasm32-unknown-unknown/release/word_count.wasm -o plugin.wasm
```

### Установка

Plugins Admin также показывает плагины онлайн-каталога Birchpad (Refresh List,
`packaging/catalog/catalog.json`), устанавливает их и предлагает обновления; чтобы плагин попал
в список, откройте pull request, добавляющий его с адресом, размером и SHA-256 его ZIP.

Plugins > Plugins Admin... > Install... принимает `plugin.toml` плагина и копирует его папку в
`plugins/<id>` рядом с настройками; Settings > Import > Import plugin(s)... принимает несколько
сразу. Повторная установка того же идентификатора обновляет плагин и сохраняет его настройки;
удаление убирает их и выданное разрешение. Администратор может разрешить только некоторые
плагины или ни одного (`PluginInstall`, `AllowedPlugins`).

## Скрипты

Сохраните файл `.rhai` в папку `scripts` рядом с настройками (Plugins > Scripts > Open Scripts
Folder); он появится в Plugins > Scripts. Например, `title-case.rhai`:

```rhai
let words = selected_text().split(" ");
let out = [];
for word in words {
    if word.len() > 0 {
        out.push(word.sub_string(0, 1).to_upper() + word.sub_string(1).to_lower());
    }
}
insert(out.reduce(|sum, w| if sum == () { w } else { sum + " " + w }));
```

| Функция | Что делает |
|---|---|
| `text()`, `set_text(s)` | весь текст |
| `length()` | его длина в байтах |
| `text_range(start, end)`, `replace(start, end, s)` | его часть, по смещениям в байтах |
| `selected_text()`, `insert(s)` | основное выделение; ввод поверх каждого выделения |
| `selections()`, `set_selection(anchor, head)` | выделения как словари `#{anchor, head}` |
| `path()`, `language()` | файл документа (или `()`) и его язык |
| `run_command(id)`, `run_command(id, #{...})` | команда Birchpad, после завершения скрипта |
| `message(s)`, `print(s)` | сообщение |
| `show_panel(title, [lines])` | панель скриптов |

Скрипт может объявить команды, по строке комментария на каждую: `// command: <функция>
<название>`. Тогда в Plugins > Scripts он показывается подменю: Run выполняет его сверху, как
раньше, а каждая команда выполняет его и затем вызывает свою функцию. Settings > Shortcut
Mapper > Plugin Commands назначает клавиши скриптам и их командам, а также командам плагинов.

```rhai
// command: upper Upper Case
// command: lower Lower Case
fn upper() { insert(selected_text().to_upper()); }
fn lower() { insert(selected_text().to_lower()); }
```

Скрипт останавливается после 50 миллионов операций. Администратор может отключить скрипты
(`UserScripts`).
