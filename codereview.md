# Code Review: iftpfm2

## Общая архитектура

Проект представляет собой утилиту для переноса файлов между FTP/FTPS/SFTP серверами с поддержкой параллелизма, graceful shutdown и single-instance режима. Архитектура в целом разумная: разделение на модули по ответственности, trait-based абстракция протоколов, использование `lib.rs` + `main.rs`.

---

## Критические проблемы

### 1. Загрузка всего файла в память (`ftp_ops.rs`)

```rust
let transfer_result = ftp_from.retr(filename.as_str(), |stream| {
    let mut data = Vec::new();
    let reader = stream;
    reader.read_to_end(&mut data)
        .map_err(suppaftp::FtpError::ConnectionError)?;
    Ok(data)
});
```

Весь файл загружается в `Vec<u8>`, затем оборачивается в `Cursor` и передаётся на upload. Для больших файлов (гигабайты) это приведёт к OOM. Нужен streaming-подход — pipe между `retr` и `put_file`, либо временный файл на диске.

### 2. Race condition в single-instance логике (`instance.rs`)

```rust
// Clean up any stale socket file
let _ = std::fs::remove_file(&socket_path);

// Create a new socket for shutdown requests
let listener = UnixListener::bind(&socket_path)?;
```

Между `remove_file` и `bind` есть окно для race condition. Если два процесса одновременно прошли `flock`, оба попытаются bind на один и тот же путь. Хотя `flock` должен это предотвращать, полагаться на два механизма одновременно (flock + socket) — хрупко.

### 3. Вызов `lsof` и `kill` через `Command` (`instance.rs`)

```rust
let output = Command::new("lsof")
    .arg("-t")
    .arg(socket_path)
    .output()?;
```

Зависимость от внешних утилит (`lsof`, `kill`) — это ненадёжно: они могут отсутствовать, вести себя по-разному на разных ОС, а `socket_path` не экранируется. Вместо `lsof` можно хранить PID в lock-файле (что уже делается!) и читать его оттуда. Вместо `Command::new("kill")` — использовать `nix::sys::signal::kill` или `libc::kill`.

### 4. Отсутствие таймаута на SFTP-операции

В `sftp.rs` таймауты устанавливаются только на TCP-стрим:

```rust
stream.set_read_timeout(Some(timeout))
```

Но ssh2-сессия может зависнуть на handshake или на операциях с файлами. `Session::set_timeout` из ssh2 не вызывается.

---

## Серьёзные проблемы

### 5. Дублирование кода в `Client` enum (`protocols/mod.rs`)

Каждый метод `Client` — это match по трём вариантам с идентичным телом:

```rust
pub fn login(&mut self, user: &str, password: &str) -> Result<(), FtpError> {
    match self {
        Client::Ftp(client) => client.login(user, password),
        Client::Ftps(client) => client.login(user, password),
        Client::Sftp(client) => client.login(user, password),
    }
}
```

Это повторяется ~12 раз. Макрос `delegate!` или crate `enum_dispatch` сильно сократят boilerplate и уменьшат вероятность ошибки при добавлении нового протокола.

### 6. Массивное дублирование в `ftp_ops.rs` — rename-логика

Блоки обработки rename (успешного и после fallback через rm + rename) практически идентичны (~40 строк каждый): verify → log → increment → delete. Это нужно вынести в отдельную функцию:

```rust
fn handle_successful_rename(
    ftp_from: &mut Client,
    ftp_to: &mut Client,
    filename: &str,
    file_size: usize,
    delete: bool,
    thread_id: usize,
) -> bool { ... }
```

### 7. `transfer_files` — функция на ~300 строк

Функция `transfer_files` делает слишком много: connect src, login src, cwd src, connect dst, login dst, cwd dst, set binary mode, list files, filter, iterate, download, upload, verify, rename, verify again, delete. Её нужно декомпозировать.

### 8. Пароль передаётся как пустая строка при отсутствии

```rust
ftp_from.login(
    config.login_from.as_str(),
    config.password_from.as_deref().unwrap_or(""),
)
```

Для SFTP `login` — no-op, но для FTP/FTPS отправка пустого пароля может привести к неожиданному поведению. Валидация в `config.rs` требует пароль для FTP/FTPS, но если валидация обходится — это потенциальная дыра.

### 9. `use crate::shutdown::{is_shutdown_requested, get_signal_type}` в `main.rs`

```rust
let signal_watch_thread = std::thread::spawn(|| {
    use crate::shutdown::{is_shutdown_requested, get_signal_type};
```

В `main.rs` это бинарный crate, а `crate::` ссылается на сам бинарь, не на библиотеку. Это скомпилируется только потому что `use iftpfm2::*` реэкспортирует эти функции в scope, но `crate::shutdown` не существует в бинарном crate. Этот код **не компилируется**. Должно быть `iftpfm2::shutdown::...` или просто использовать уже импортированные имена.

---

## Проблемы среднего уровня

### 10. Логирование ошибок игнорируется повсеместно

```rust
let _ = log_with_thread(...);
```

Если логирование упало (диск полон, IO error), это молча проглатывается. Как минимум для критических сообщений стоит иметь fallback на stderr.

### 11. `FtpError` как единый тип ошибки

```rust
pub type FtpError = suppaftp::FtpError;
```

SFTP-ошибки оборачиваются в `FtpError::ConnectionError(std::io::Error)`, теряя семантику. Лучше создать собственный enum ошибок с вариантами для FTP, SFTP, IO, TLS и т.д., с реализацией `From` для каждого.

### 12. SFTP: `nlst` фильтрует директории, но FTP — нет

В `sftp.rs` `nlst` фильтрует `is_dir()`, а в `ftp.rs`/`ftps.rs` — просто прокидывает результат `stream.nlst()`. Это разное поведение одного и того же trait-метода. Если в FTP-директории есть поддиректории, `transfer_files` попытается их скачать как файлы.

### 13. `NoCertificateVerification` в `ftps.rs`

```rust
pub struct NoCertificateVerification;
```

Модуль `danger` виден только внутри `ftps.rs` (ok), но нет никакого предупреждения пользователю при запуске с `--insecure-skip-verify`. Стоит выводить заметное предупреждение в лог.

### 14. `Config::Drop` zeroize — неполная защита

```rust
impl Drop for Config {
    fn drop(&mut self) {
        if let Some(ref mut p) = self.password_from {
            p.zeroize();
        }
    }
}
```

Хорошая идея, но `String` в Rust может быть скопирован (clone), перемещён, а старая память не зануляется. Также `serde_json::from_str` создаёт промежуточные строки. Для серьёзной защиты нужен `secrecy::Secret<String>` или аналог.

### 15. Rename fallback — удаление существующего файла

```rust
if ftp_to.rm(filename.as_str()).is_ok() {
    let _ = log_with_thread(
        format!("Replaced existing file {}", filename),
        Some(thread_id),
    );
}
```

Между `rm` и `rename` файл отсутствует на целевом сервере. Если процесс упадёт в этот момент — данные потеряны. Это фундаментальная проблема FTP (нет атомарного overwrite), но стоит хотя бы задокументировать этот риск.

### 16. `check_single_instance` — привязка к `/tmp`

Hardcoded пути `/tmp/{PROGRAM_NAME}.sock` и `.pid` не работают, если `/tmp` смонтирован с `noexec` или если несколько пользователей запускают утилиту. Стоит использовать `$XDG_RUNTIME_DIR` или хотя бы включать UID в имя файла.

---

## Мелкие замечания

### 17. `as_str()` и `as_deref()` где не нужны

```rust
ftp_from.login(config.login_from.as_str(), ...)
ftp_from.cwd(config.path_from.as_str())
```

`&String` автоматически deref'ится в `&str`, поэтому `.as_str()` избыточен:

```rust
ftp_from.login(&config.login_from, ...)
```

### 18. `filename.as_str()` в цикле

```rust
for filename in file_list {
    // ...
    ftp_from.mdtm(filename.as_str())
```

`filename` — `String`, можно просто `&filename`.

### 19. `format!` для конкатенации путей в SFTP

```rust
let full_path = format!("{}/{}", self.current_dir.trim_end_matches('/'), filename);
```

Повторяется в каждом методе. Стоит вынести в приватный хелпер `fn full_path(&self, filename: &str) -> String`.

### 20. Тесты минимальны

Тесты покрывают: парсинг конфигурации, валидацию, shutdown-флаг, компиляцию regex. Нет интеграционных тестов для `transfer_files` (даже с mock-FTP сервером). Нет тестов для `logging`, `instance` в реальных сценариях. `test_ftp_client_send` — просто проверяет trait bound, не функциональность.

### 21. `process::exit()` в `parse_args`

```rust
process::exit(1);
```

Вызов `process::exit` в библиотечном коде (`cli.rs` часть `lib.rs`) — антипаттерн. Это делает код нетестируемым. Лучше возвращать `Result<CliArgs, CliError>` и вызывать `exit` только в `main`.

### 22. Неиспользуемый параметр `_stdout`

```rust
let cli::CliArgs { ..., stdout: _, ... } = parse_args();
```

Флаг `-s` парсится, но в `main.rs` игнорируется (связывается с `_`). Если stdout — это альтернатива логфайлу, то при `-s` не нужно вызывать `set_log_file` (что и так происходит), но флаг всё равно нефункционален — по умолчанию логи и так идут в stdout.

### 23. `e.to_string().replace("\n", "")`

```rust
format!("Error getting modified time for file '{}': {}, skipping",
    filename, e.to_string().replace("\n", ""))
```

Sanitization новых строк в одном месте, но не в остальных. Если это важно (log injection), нужно делать это единообразно в `log_with_thread`.

### 24. `expect` в runtime-коде

```rust
let regex = Regex::new(&config.filename_regexp)
    .expect("Regex pattern should be valid (validated in config parser)");
```

Даже с комментарием, `expect` в production-коде — плохая практика. Если валидация обойдена (например, через программный API), это паника.

### 25. Документация `TransferMode::Binary`

```rust
/// Binary mode (untransferred)
Binary,
```

Опечатка: "untransferred" → "untranslated" или "raw".

---

## Что сделано хорошо

Отмечу положительные аспекты: атомарная загрузка через временный файл с rename, обязательная верификация размера после upload, graceful shutdown через atomic-флаги (async-signal-safe), zeroize паролей в `Drop`, поддержка комментариев в конфиг-файле, валидация конфигурации с адекватными сообщениями об ошибках, использование `BufWriter` для логов, обработка poisoned mutex.

---

## Рекомендуемые приоритеты

По критичности: (1) исправить `crate::shutdown` в `main.rs` — код не компилируется, (2) добавить streaming для больших файлов, (3) убрать зависимость от `lsof`/`kill`, (4) декомпозировать `transfer_files`, (5) убрать `process::exit` из библиотечного кода, (6) унифицировать поведение `nlst` между протоколами, (7) добавить интеграционные тесты.
