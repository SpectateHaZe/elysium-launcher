//! Building the game command line and running the game process.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;

use crate::account::Account;
use crate::error::{IoContext, Result};
use crate::install::Prepared;
use crate::instance::Instance;
use crate::logs::{LogLine, LogParser};
use crate::rules::{Arch, Environment, OsName};
use crate::settings::EffectiveSettings;

pub const LAUNCHER_NAME: &str = "elysium-launcher";
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

/// Builds the full Java command line for `instance`.
pub fn build_command(
    prepared: &Prepared,
    instance: &Instance,
    account: &Account,
    settings: &EffectiveSettings,
) -> LaunchCommand {
    let version = &prepared.version;
    let game_dir = instance.game_dir();
    let mut env = Environment::current();
    if settings.resolution.is_some() {
        env = env.with_feature("has_custom_resolution", true);
    }

    let sep = if cfg!(windows) { ";" } else { ":" };
    let classpath = prepared
        .classpath
        .iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join(sep);
    let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
    let uuid = account.uuid.simple().to_string();
    let game_assets = prepared
        .legacy_assets_dir
        .clone()
        .unwrap_or_else(|| prepared.assets_root.clone());

    let mut vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", account.username.clone()),
        ("auth_uuid", uuid.clone()),
        ("auth_access_token", account.access_token().to_owned()),
        (
            "auth_session",
            format!("token:{}:{uuid}", account.access_token()),
        ),
        ("auth_xuid", "0".to_owned()),
        ("clientid", "0".to_owned()),
        ("user_type", account.user_type().to_owned()),
        ("user_properties", "{}".to_owned()),
        ("version_name", version.id.clone()),
        ("version_type", version.kind.as_arg().to_owned()),
        ("game_directory", path(&game_dir)),
        ("assets_root", path(&prepared.assets_root)),
        ("game_assets", path(&game_assets)),
        ("assets_index_name", prepared.assets_index.clone()),
        ("natives_directory", path(&prepared.natives_dir)),
        (
            "library_directory",
            path(&prepared.assets_root.with_file_name("libraries")),
        ),
        ("classpath", classpath),
        ("classpath_separator", sep.to_owned()),
        ("launcher_name", LAUNCHER_NAME.to_owned()),
        ("launcher_version", LAUNCHER_VERSION.to_owned()),
    ]);
    if let Some(r) = settings.resolution {
        vars.insert("resolution_width", r.width.to_string());
        vars.insert("resolution_height", r.height.to_string());
    }

    let mut jvm = vec![
        format!("-Xms{}M", settings.min_memory_mb),
        format!("-Xmx{}M", settings.max_memory_mb),
    ];
    jvm.extend(settings.jvm_args.iter().cloned());
    if let Some(arg) = &prepared.logging_arg {
        jvm.push(arg.clone());
    }

    let mut version_jvm = Vec::new();
    let mut game = Vec::new();
    match &version.arguments {
        Some(args) => {
            args.jvm
                .iter()
                .for_each(|a| a.collect_into(&env, &mut version_jvm));
            args.game
                .iter()
                .for_each(|a| a.collect_into(&env, &mut game));
        }
        None => {
            version_jvm = legacy_jvm_args(&env);
            game = version
                .minecraft_arguments
                .as_deref()
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            // Legacy-format 1.6–1.12 versions accept a window size; older ones don't.
            if version.main_class == "net.minecraft.client.main.Main"
                && settings.resolution.is_some()
            {
                game.extend(
                    [
                        "--width",
                        "${resolution_width}",
                        "--height",
                        "${resolution_height}",
                    ]
                    .map(String::from),
                );
            }
        }
    }

    let mut args: Vec<String> = jvm;
    args.extend(version_jvm.iter().map(|a| substitute(a, &vars)));
    args.push(version.main_class.clone());
    args.extend(game.iter().map(|a| substitute(a, &vars)));

    LaunchCommand {
        program: prepared.java.clone(),
        args,
        cwd: game_dir,
    }
}

/// JVM arguments the vanilla launcher adds for versions without `arguments.jvm`.
fn legacy_jvm_args(env: &Environment) -> Vec<String> {
    let mut out = Vec::new();
    match env.os {
        OsName::Osx => out.push("-XstartOnFirstThread".to_owned()),
        OsName::Windows => out.push(
            "-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump".to_owned(),
        ),
        OsName::Linux => {}
    }
    if env.arch == Arch::X86 {
        out.push("-Xss1M".to_owned());
    }
    out.extend(
        [
            "-Djava.library.path=${natives_directory}",
            "-Dminecraft.launcher.brand=${launcher_name}",
            "-Dminecraft.launcher.version=${launcher_version}",
            "-cp",
            "${classpath}",
        ]
        .map(String::from),
    );
    out
}

/// Replaces every `${name}` with its value; unknown placeholders are left untouched.
pub fn substitute(input: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => out.push_str(&rest[start..start + 3 + end]),
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Prevents a console window from flashing up on Windows.
pub fn hide_console(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

#[derive(Debug, Clone)]
pub struct GameExit {
    /// Exit code; `None` if the process was killed by a signal.
    pub code: Option<i32>,
    pub duration: Duration,
    pub killed: bool,
}

impl GameExit {
    pub fn success(&self) -> bool {
        self.killed || self.code == Some(0)
    }
}

/// Runs the game until it exits or `kill` fires, streaming parsed log lines to `on_log`.
pub async fn run(
    command: &LaunchCommand,
    on_log: Arc<dyn Fn(LogLine) + Send + Sync>,
    kill: oneshot::Receiver<()>,
) -> Result<GameExit> {
    std::fs::create_dir_all(&command.cwd).at(&command.cwd)?;
    let mut cmd = tokio::process::Command::new(&command.program);
    cmd.args(&command.args)
        .current_dir(&command.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // The game must outlive the launcher if the user closes it mid-session.
        .kill_on_drop(false);
    hide_console(&mut cmd);

    let started = Instant::now();
    let mut child = cmd.spawn().at(&command.program)?;
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(tokio::spawn(pump(BufReader::new(out), Arc::clone(&on_log))));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(tokio::spawn(pump(BufReader::new(err), Arc::clone(&on_log))));
    }

    let (status, killed) = tokio::select! {
        status = child.wait() => (status.at(&command.program)?, false),
        _ = kill => {
            // Best effort: the process may already be exiting.
            let _ = child.start_kill();
            (child.wait().await.at(&command.program)?, true)
        }
    };
    for r in readers {
        let _ = r.await;
    }
    Ok(GameExit {
        code: status.code(),
        duration: started.elapsed(),
        killed,
    })
}

async fn pump<R: tokio::io::AsyncRead + Unpin>(
    reader: BufReader<R>,
    on_log: Arc<dyn Fn(LogLine) + Send + Sync>,
) {
    let mut parser = LogParser::default();
    let mut lines = reader.lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => parser.feed(&line).into_iter().for_each(|l| on_log(l)),
            Ok(None) => break,
            Err(e) => {
                // Non-UTF-8 output: report once and stop reading this stream.
                tracing::warn!("stopped reading game output: {e}");
                break;
            }
        }
    }
    parser.finish().into_iter().for_each(|l| on_log(l));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::InstanceManager;
    use crate::settings::{Resolution, Settings};

    fn prepared(version_json: &str, dir: &std::path::Path) -> Prepared {
        Prepared {
            version: serde_json::from_str(version_json).unwrap(),
            java: PathBuf::from("/java"),
            classpath: vec![dir.join("a.jar"), dir.join("client.jar")],
            natives_dir: dir.join("natives"),
            assets_root: dir.join("assets"),
            assets_index: "legacy".into(),
            legacy_assets_dir: Some(dir.join("assets/virtual/legacy")),
            logging_arg: Some("-Dlog4j.configurationFile=/cfg.xml".into()),
        }
    }

    #[test]
    fn substitution() {
        let vars = HashMap::from([("a", "1".to_owned()), ("bb", "2".to_owned())]);
        assert_eq!(
            substitute("x${a}y${bb}${unknown}${", &vars),
            "x1y2${unknown}${"
        );
    }

    #[test]
    fn legacy_command() {
        let dir = tempfile::tempdir().unwrap();
        let inst = InstanceManager::new(dir.path())
            .create("Old", "1.6.4")
            .unwrap();
        let p = prepared(
            r#"{"id":"1.6.4","type":"release","mainClass":"net.minecraft.client.main.Main",
               "minecraftArguments":"--username ${auth_player_name} --session ${auth_session} --version ${version_name} --gameDir ${game_directory} --assetsDir ${game_assets}"}"#,
            dir.path(),
        );
        let account = Account::offline("Steve").unwrap();
        let mut settings = Settings::default().effective(&Default::default());
        settings.resolution = Some(Resolution {
            width: 1280,
            height: 720,
        });
        let cmd = build_command(&p, &inst, &account, &settings);
        let args = cmd.args.join(" ");

        assert_eq!(cmd.args[0], "-Xms512M");
        assert!(args.contains("-Dlog4j.configurationFile=/cfg.xml"));
        assert!(args.contains(&format!(
            "-Djava.library.path={}",
            dir.path().join("natives").display()
        )));
        let main = cmd
            .args
            .iter()
            .position(|a| a == "net.minecraft.client.main.Main")
            .unwrap();
        let cp = cmd.args.iter().position(|a| a == "-cp").unwrap();
        assert!(cp < main);
        assert!(cmd.args[cp + 1].ends_with("client.jar"));
        assert!(args.contains("--username Steve"));
        assert!(args.contains(&format!("--session token:0:{}", account.uuid.simple())));
        assert!(args.contains(&format!(
            "--assetsDir {}",
            dir.path().join("assets/virtual/legacy").display()
        )));
        assert!(args.ends_with("--width 1280 --height 720"));
        assert!(!args.contains("${"), "unresolved placeholder in: {args}");
        assert_eq!(cmd.cwd, inst.game_dir());
    }

    #[test]
    fn modern_command_respects_rules() {
        let dir = tempfile::tempdir().unwrap();
        let inst = InstanceManager::new(dir.path())
            .create("New", "1.21")
            .unwrap();
        let p = prepared(
            r#"{"id":"1.21","type":"release","mainClass":"net.minecraft.client.main.Main",
               "arguments":{
                 "game":["--username","${auth_player_name}","--uuid","${auth_uuid}",
                   {"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"},
                   {"rules":[{"action":"allow","features":{"has_custom_resolution":true}}],"value":["--width","${resolution_width}"]}],
                 "jvm":[{"rules":[{"action":"allow","os":{"name":"osx"}}],"value":["-XstartOnFirstThread"]},
                   "-Djava.library.path=${natives_directory}","-cp","${classpath}"]}}"#,
            dir.path(),
        );
        let account = Account::offline("Alex").unwrap();
        let settings = Settings::default().effective(&Default::default());
        let args = build_command(&p, &inst, &account, &settings).args.join(" ");
        assert!(!args.contains("--demo"));
        assert!(!args.contains("--width"), "no resolution set");
        assert_eq!(
            args.contains("-XstartOnFirstThread"),
            cfg!(target_os = "macos")
        );
        assert!(args.contains(&format!("--uuid {}", account.uuid.simple())));
    }
}
