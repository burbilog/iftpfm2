# Code Review: iftpfm2 (после рефакторинга)

## Что исправлено с прошлого ревью

Значительная часть замечаний была адресована: `lsof`/`kill` заменены на `nix::sys::signal`, пароли обёрнуты в `secrecy::Secret`, `parse_args` возвращает `Result` вместо `process::exit`, добавлен `delegate!` макрос, дублирование rename-логики устранено через `handle_successful_rename`, `full_path` хелпер в SFTP, `session.set_timeout` для SFTP, streaming через `TransferBuffer` (RAM/disk), фильтрация директорий в nlst для FTP/FTPS, user-isolated lock paths, stderr fallback при ошибках логирования, strip newlines в логах, debug mode. Качество кода заметно выросло.

---

## Критические проблемы

### 1. `crate::shutdown` в main.rs — не компилируется

```rust
let signal_watch_thread = std::thread::spawn(|| {
    use crate::shutdown::{is_shutdown_requested, get_signal_type};
    // ...
});
```

`main.rs` — бинарный crate. У него нет модуля `shutdown`. `crate::` ссылается на сам бинарь, не на библиотеку `iftpfm2`. Кроме того, `get_signal_type` не реэкспортируется из `lib.rs`:

```rust
// lib.rs
pub use shutdown::{is_shutdown_requested, request_shutdown};
// get_signal_type отсутствует ↑
```

Нужно либо добавить `get_signal_type` в реэкспорт, либо использовать `iftpfm2::shutdown::get_signal_type`.

### 2. Regex ошибка возвращает 1 вместо 0

```rust
let regex = match Regex::new(&config.filename_regexp) {
    Ok(re) => re,
    Err(e) => {
        let _ = log_with_thread(...);
        return 1; // ← BUG
    }
};
```

`transfer_files` возвращает количество успешных трансферов. `return 1` при ошибке regex означает, что один файл якобы был успешно передан. Должно быть `return 0`.

---

## Серьёзные проблемы

### 3. `process::exit(1)` в библиотечном коде (`TransferBuffer`)

```rust
fn into_reader(self) -> Box<dyn Read + Send> {
    match self {
        TransferBuffer::Disk(temp_file) => {
            match temp_file.reopen() {
                Ok(reader) => Box::new(reader),
                Err(_) => {
                    Box::new(std::fs::File::open(temp_file.path()).unwrap_or_else(|_| {
                        std::io::stderr().write_all(b"Critical error: ...").ok();
                        std::process::exit(1);  // ← убивает процесс из библиотеки
                    }))
                }
            }
        }
    }
}
```

Проблемы: `process::exit` в библиотечном коде — антипаттерн; fallback через `File::open` после неудачного `reopen()` на тот же файл скорее всего тоже упадёт. Сигнатура должна быть `fn into_reader(self) -> Result<Box<dyn Read + Send>, io::Error>`, а ошибка — пробрасываться наверх.

### 4. `nlst` с SIZE-фильтрацией — O(n) сетевых вызовов

```rust
// ftp.rs и ftps.rs
let files_only: Vec<String> = all_names
    .into_iter()
    .filter(|name| {
        if name == "." || name == ".." { return false; }
        self.stream.size(name).is_ok()  // ← один SIZE запрос на каждый файл
    })
    .collect();
```

Для директории с 1000 записей это 1000 дополнительных SIZE-команд. На высоколатентных соединениях это превратит листинг в многоминутную операцию. Альтернатива — использовать `LIST` и парсить вывод (хотя это сложнее из-за различий между серверами). Как минимум стоит задокументировать этот trade-off и добавить предупреждение в лог, если количество entries велико.

### 5. `OpenOptions::truncate(true)` перед `flock`

```rust
let mut lock_file = match OpenOptions::new()
    .write(true)
    .create(true)
    .truncate(true)  // ← обнуляет файл ДО получения блокировки
    .open(&pid_path)
```

Как обсуждалось ранее, `flock` защищает от race condition на socket. Но `truncate(true)` при `open()` обнуляет PID-файл ещё до попытки блокировки. Если процесс B открывает файл с truncate — PID процесса A стирается, хотя процесс A всё ещё работает и держит блокировку. Блокировка привязана к fd, а не к содержимому, поэтому flock продолжает работать. Но `signal_process_to_terminate` читает PID из этого файла:

```rust
let pid_str = std::fs::read_to_string(&pid_path)?;
```

Если файл уже обнулён truncate'ом процесса B — PID будет пустым, и сигнал не отправится. Решение: убрать `.truncate(true)`, записывать PID только после успешного flock, при этом делать `lock_file.set_len(0)` + `write_all` после получения блокировки.

---

## Проблемы среднего уровня

### 6. `connect_and_login` — запутанная логика пароля

```rust
let password_for_login = match proto {
    Protocol::Sftp if keyfile.is_some() => password.map(|s| s.as_str()).unwrap_or(""),
    _ => password.map(|s| s.as_str()).ok_or_else(|| {
        format!("BUG: Password required for {} but was None ...", proto)
    })?,
};
```

Для SFTP `login()` — no-op, поэтому значение `password_for_login` для SFTP+keyfile не имеет значения. Но код вычисляет его, создавая ложное впечатление, что пустой пароль передаётся на сервер. Проще сделать:

```rust
// Для SFTP login() — no-op, пароль не нужен
let password_for_login = password.map(|s| s.as_str()).unwrap_or("");
```

### 7. Мёртвый код: `age == 0` в `check_file_should_transfer`

Документация:
```rust
/// - `min_age_seconds == 0`: Age checking is disabled, all files pass age check
```

Но `config.validate()` отвергает `age == 0`:
```rust
if self.age == 0 {
    return Err(Error::new(ErrorKind::InvalidInput, "age cannot be 0"));
}
```

Комментарий описывает невозможный сценарий. Либо убрать ветку для `age == 0`, либо убрать проверку в validate (если хотите поддержать "без фильтра по возрасту").

### 8. `TransferBuffer::size()` получает размер повторно

```rust
match transfer_result {
    Ok(buffer) => {
        let file_size = buffer.size();  // ← Для Disk вызывает metadata()
        let file_size = usize::try_from(file_size).unwrap_or(usize::MAX);
```

`file_size` уже известен из `check_file_should_transfer` (через SIZE команду), а тут он перевычисляется из буфера. Для RAM-буфера это `vec.len()`, но для Disk — это `metadata().len()`, что может отличаться от FTP SIZE (например, если запись не была полностью flushed). Лучше передавать ожидаемый размер напрямую.

### 9. `-s` флаг по-прежнему функционально бесполезен

```rust
if stdout {
    let _ = log("Logging to stdout (explicit via -s flag)");
}
```

Когда не задан ни `-s`, ни `-l`, логи и так идут в stdout. Флаг `-s` только печатает одну строку. Если он предназначен для явного намерения — ок, но стоит задокументировать это в `print_usage`:

```
-s                 Write logs to stdout (this is the default behavior;
                   use this flag to make it explicit)
```

---

## Мелкие замечания

### 10. `as_str()` по-прежнему избыточен в нескольких местах

```rust
ftp_to.rename(&tmp_filename, &filename);   // ← ок, уже исправлено в некоторых
ftp_to.rm(tmp_filename.as_str());          // ← .as_str() не нужен
```

Непоследовательно: в одних местах `&filename`, в других `filename.as_str()`.

### 11. Тест `test_parse_config` — непрямое сравнение

```rust
assert_eq!(configs[0].ip_address_from, expected[0].ip_address_from);
assert_eq!(configs[0].port_from, expected[0].port_from);
// ... 14 строк assert_eq
```

Поскольку `Secret` не поддерживает `PartialEq`, нельзя сравнить Config целиком. Но можно реализовать вручную `PartialEq` для тестов через `cfg(test)`, или вынести сравнение в хелпер.

### 12. `TransferBuffer` не реализует `Drop` для cleanup при ошибках

Если `transfer_result` — `Ok(buffer)`, но затем upload падает, `buffer` (если Disk) дропается, и `NamedTempFile` автоматически удаляет файл — это правильно. Но стоит это задокументировать, потому что поведение зависит от `NamedTempFile::drop`.

### 13. Непоследовательное форматирование `keyfile_pass_from` в тестах

```rust
let config = Config {
    password_from: Some(Secret::new("pass".to_string())),
    keyfile_from: None,
        keyfile_pass_from: None,  // ← лишний отступ
    path_from: "/path/".to_string(),
```

Встречается во всех тестах config.rs — похоже на ошибку автоформатирования.

---

## Итог

Кодовая база значительно улучшилась. Из прошлых ~25 замечаний исправлено ~15. По приоритетам: (1) исправить `crate::shutdown` в main.rs — не скомпилируется, (2) `return 1` → `return 0` при ошибке regex, (3) убрать `truncate(true)` перед flock и переписать как truncate-after-lock, (4) изменить сигнатуру `TransferBuffer::into_reader` на `Result`, (5) задокументировать производительность nlst+SIZE.
